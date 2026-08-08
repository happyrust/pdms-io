use std::cmp::Ordering;
use std::fs::File;

use crate::core::{EngineError, PageId, RecordLoc, RefNo};
use crate::db1::PageStore;

const INDEX_PAGE_NOUN: u32 = 0x00CC47DF;
/// core.dll 的表页搜索（3.1 IDB `sub_5B014E0`）在入口处硬校验 `*page != 5`，
/// 不满足直接置错误码 659 并终止，因此索引页只能是 5。
const INDEX_PAGE_TYPE: u32 = 5;
const INDEX_PAGE_HEADER_DWORDS: usize = 7;
const INDEX_PAGE_HEADER_SIZE: usize = INDEX_PAGE_HEADER_DWORDS * 4;
const REFNO_KEY_DWORDS: u32 = 2;
const RECORD_VALUE_DWORDS: u32 = 2;
/// 内部节点的值宽由 core.dll 固定为 2，忽略页头声明。
const INTERNAL_VALUE_DWORDS: u32 = 2;
const START_MARKER_HI: u32 = 0x8000_0001;
const START_MARKER_LO: u32 = 0x8000_0001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexEntry {
    pub refno: RefNo,
    pub page_no: u32,
    pub offset_words: u32,
    pub flag: u16,
}

impl IndexEntry {
    pub fn is_start_marker(&self) -> bool {
        self.refno.hi() == START_MARKER_HI && self.refno.lo() == START_MARKER_LO
    }

    /// 条目里只有页号和页内偏移，扩展文件号由承载该条目的索引页决定，必须由调用方传入。
    pub fn to_record_loc(&self, ext_no: u32) -> RecordLoc {
        RecordLoc {
            ext_no,
            page_no: self.page_no,
            byte_offset: self.offset_words.saturating_mul(2),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexPageView {
    pub page_type: u32,
    pub noun: u32,
    pub level: u32,
    pub key_dwords: u32,
    pub value_dwords: u32,
    pub reserved: u32,
    /// 页内剩余可用空间，单位为 4 字节字。core.dll 用它反推条目数：
    /// `count = (page_dwords - 7 - free_dwords) / (key_dwords + value_dwords)`
    pub free_dwords: u32,
    pub entries: Vec<IndexEntry>,
}

pub struct IndexCursor;

#[derive(Debug)]
struct ChildSelection {
    child_page: PageId,
    boundary_idx: usize,
}

#[derive(Debug)]
struct IndexInsertResult {
    child_max: IndexEntry,
    split_right: Option<IndexEntry>,
}

/// 条目的键宽与值宽，单位为 4 字节字。
///
/// 页头声明的宽度为 0 时（早期版本写出的页从不填这两个字段）回退到 RefNo 索引的
/// 2 + 2。内部节点的值宽由 core.dll 固定为 2，不看页头声明。
///
/// 键宽同时决定值在条目内的偏移，所以步长和值偏移必须都从这里取，不能各写各的。
fn entry_widths(
    level: u32,
    key_dwords: u32,
    value_dwords: u32,
) -> Result<(usize, usize), EngineError> {
    if (value_dwords as i32) < 0 {
        return Err(EngineError::Format(format!(
            "索引页声明了变长值宽 {}，当前不支持",
            value_dwords as i32
        )));
    }

    let key = if key_dwords > 0 {
        key_dwords
    } else {
        REFNO_KEY_DWORDS
    };
    let value = if level != 0 {
        INTERNAL_VALUE_DWORDS
    } else if value_dwords > 0 {
        value_dwords
    } else {
        RECORD_VALUE_DWORDS
    };

    // 键区前两个字读 RefNo，值区前两个字读页号与打包字段。
    if key < REFNO_KEY_DWORDS || value < RECORD_VALUE_DWORDS {
        return Err(EngineError::Format(format!(
            "索引页声明的键/值宽 {}/{} 放不下 RefNo 与记录位置",
            key, value
        )));
    }

    Ok((key as usize, value as usize))
}

fn entry_stride_dwords(
    level: u32,
    key_dwords: u32,
    value_dwords: u32,
) -> Result<usize, EngineError> {
    let (key, value) = entry_widths(level, key_dwords, value_dwords)?;
    Ok(key + value)
}

impl IndexPageView {
    pub fn from_page(page: &[u8]) -> Result<Self, EngineError> {
        if page.len() < INDEX_PAGE_HEADER_SIZE {
            return Err(EngineError::Format("索引页面数据太短".into()));
        }

        let page_type = u32::from_be_bytes(page[0..4].try_into().unwrap());
        let noun = u32::from_be_bytes(page[4..8].try_into().unwrap());
        if noun != INDEX_PAGE_NOUN {
            return Err(EngineError::Format(format!(
                "索引页面 noun 非法: 0x{:X}",
                noun
            )));
        }

        let level = u32::from_be_bytes(page[8..12].try_into().unwrap());
        let key_dwords = u32::from_be_bytes(page[12..16].try_into().unwrap());
        let value_dwords = u32::from_be_bytes(page[16..20].try_into().unwrap());
        let reserved = u32::from_be_bytes(page[20..24].try_into().unwrap());
        let free_dwords = u32::from_be_bytes(page[24..28].try_into().unwrap());

        let (key_width, value_width) = entry_widths(level, key_dwords, value_dwords)?;
        let stride = key_width + value_width;
        let page_dwords = page.len() / 4;
        let entry_count = page_dwords
            .saturating_sub(INDEX_PAGE_HEADER_DWORDS)
            .saturating_sub(free_dwords as usize)
            / stride;
        let entry_count = entry_count.min(max_entries_per_index_page(page.len(), stride));

        let mut entries = Vec::with_capacity(entry_count);
        for index in 0..entry_count {
            let offset = INDEX_PAGE_HEADER_SIZE + index * stride * 4;
            let refno = RefNo::from_parts(
                u32::from_be_bytes(page[offset..offset + 4].try_into().unwrap()),
                u32::from_be_bytes(page[offset + 4..offset + 8].try_into().unwrap()),
            );
            let value_at = offset + key_width * 4;
            let page_no = u32::from_be_bytes(page[value_at..value_at + 4].try_into().unwrap());
            let packed = u32::from_be_bytes(page[value_at + 4..value_at + 8].try_into().unwrap());
            entries.push(IndexEntry {
                refno,
                page_no,
                offset_words: packed >> 12,
                flag: (packed & 0x0FFF) as u16,
            });
        }

        Ok(Self {
            page_type,
            noun,
            level,
            key_dwords,
            value_dwords,
            reserved,
            free_dwords,
            entries,
        })
    }

    fn stride_dwords(&self) -> Result<usize, EngineError> {
        entry_stride_dwords(self.level, self.key_dwords, self.value_dwords)
    }

    fn empty_leaf() -> Self {
        Self {
            page_type: INDEX_PAGE_TYPE,
            noun: INDEX_PAGE_NOUN,
            level: 0,
            key_dwords: REFNO_KEY_DWORDS,
            value_dwords: RECORD_VALUE_DWORDS,
            reserved: 0,
            free_dwords: 0,
            entries: Vec::new(),
        }
    }
}

fn compare_refno(a: RefNo, b: RefNo) -> Ordering {
    (a.hi(), a.lo()).cmp(&(b.hi(), b.lo()))
}

fn max_entries_per_index_page(page_size: usize, stride_dwords: usize) -> usize {
    (page_size / 4).saturating_sub(INDEX_PAGE_HEADER_DWORDS) / stride_dwords
}

fn insert_or_replace_sorted(entries: &mut Vec<IndexEntry>, entry: IndexEntry) {
    if let Some(existing) = entries
        .iter()
        .position(|current| current.refno == entry.refno)
    {
        entries[existing] = entry;
        return;
    }

    let insert_at = entries
        .iter()
        .position(|current| compare_refno(entry.refno, current.refno).is_lt())
        .unwrap_or(entries.len());
    entries.insert(insert_at, entry);
}

fn start_marker_entry(child_page_no: u32) -> IndexEntry {
    IndexEntry {
        refno: RefNo::from_parts(START_MARKER_HI, START_MARKER_LO),
        page_no: child_page_no,
        offset_words: 0,
        flag: 0,
    }
}

fn leaf_boundary_entry(entry: IndexEntry, child_page_no: u32) -> IndexEntry {
    IndexEntry {
        refno: entry.refno,
        page_no: child_page_no,
        offset_words: 0,
        flag: 0,
    }
}

fn max_boundary_entry(entries: &[IndexEntry], page_no: u32) -> Result<IndexEntry, EngineError> {
    let last = entries
        .iter()
        .rfind(|entry| !entry.is_start_marker())
        .copied()
        .ok_or_else(|| EngineError::Format("索引页缺少边界条目".into()))?;
    Ok(leaf_boundary_entry(last, page_no))
}

fn split_leaf_entries(entries: &[IndexEntry]) -> (Vec<IndexEntry>, Vec<IndexEntry>) {
    let mid = entries.len() / 2;
    (entries[..mid].to_vec(), entries[mid..].to_vec())
}

fn split_internal_entries(
    entries: &[IndexEntry],
) -> Result<(Vec<IndexEntry>, Vec<IndexEntry>), EngineError> {
    if entries.is_empty() || !entries[0].is_start_marker() {
        return Err(EngineError::Format("内部索引页缺少起始标记".into()));
    }

    let normals = &entries[1..];
    if normals.len() < 2 {
        return Err(EngineError::Format("内部索引页条目不足，无法分裂".into()));
    }

    let mid = normals.len() / 2;
    let left_normals = normals[..mid].to_vec();
    let right_normals = normals[mid..].to_vec();
    if left_normals.is_empty() || right_normals.is_empty() {
        return Err(EngineError::Format("内部索引页分裂后出现空子树".into()));
    }

    let mut left_entries = Vec::with_capacity(left_normals.len() + 1);
    left_entries.push(start_marker_entry(entries[0].page_no));
    left_entries.extend(left_normals);

    let mut right_entries = Vec::with_capacity(right_normals.len() + 1);
    right_entries.push(start_marker_entry(right_normals[0].page_no));
    right_entries.extend(right_normals);

    Ok((left_entries, right_entries))
}

pub(crate) fn serialize_index_page(
    page_size: usize,
    page: &IndexPageView,
) -> Result<Vec<u8>, EngineError> {
    let (key_width, value_width) = entry_widths(page.level, page.key_dwords, page.value_dwords)?;
    let stride = key_width + value_width;
    let max_entries = max_entries_per_index_page(page_size, stride);
    if page.entries.len() > max_entries {
        return Err(EngineError::Format(format!(
            "索引页面条目数 {} 超过容量 {}",
            page.entries.len(),
            max_entries
        )));
    }

    // 页头声明的宽度可能是 0（早期版本写出的页），这里统一落盘为实际生效的宽度，
    // 保证 free_dwords 与读侧推导用的是同一个步长。
    let key_dwords = key_width as u32;
    let value_dwords = value_width as u32;
    let free_dwords = (page_size / 4)
        .saturating_sub(INDEX_PAGE_HEADER_DWORDS)
        .saturating_sub(page.entries.len() * stride);

    let mut data = vec![0u8; page_size];
    data[0..4].copy_from_slice(&INDEX_PAGE_TYPE.to_be_bytes());
    data[4..8].copy_from_slice(&INDEX_PAGE_NOUN.to_be_bytes());
    data[8..12].copy_from_slice(&page.level.to_be_bytes());
    data[12..16].copy_from_slice(&key_dwords.to_be_bytes());
    data[16..20].copy_from_slice(&value_dwords.to_be_bytes());
    data[20..24].copy_from_slice(&page.reserved.to_be_bytes());
    data[24..28].copy_from_slice(&(free_dwords as u32).to_be_bytes());

    for (index, entry) in page.entries.iter().enumerate() {
        let offset = INDEX_PAGE_HEADER_SIZE + index * stride * 4;
        data[offset..offset + 4].copy_from_slice(&entry.refno.hi().to_be_bytes());
        data[offset + 4..offset + 8].copy_from_slice(&entry.refno.lo().to_be_bytes());
        let value_at = offset + key_width * 4;
        data[value_at..value_at + 4].copy_from_slice(&entry.page_no.to_be_bytes());
        let packed = (entry.offset_words << 12) | entry.flag as u32;
        data[value_at + 4..value_at + 8].copy_from_slice(&packed.to_be_bytes());
    }

    Ok(data)
}

/// 子页与父页同属一个扩展文件，`ext_no` 由调用方按父页传入。
fn select_child(entries: &[IndexEntry], target: RefNo, ext_no: u32) -> Option<PageId> {
    let start_marker = entries
        .iter()
        .find(|entry| entry.is_start_marker())
        .copied();
    let normals: Vec<IndexEntry> = entries
        .iter()
        .copied()
        .filter(|entry| !entry.is_start_marker())
        .collect();

    if normals.is_empty() {
        // A freshly promoted or sparsely populated internal page may contain
        // only core.dll's start sentinel.  The sentinel is still a real
        // left-most child pointer; rejecting it makes exact lookups fail even
        // though the streaming iterator can descend through the same page.
        return start_marker.map(|entry| PageId {
            ext_no,
            page_no: entry.page_no,
        });
    }

    if compare_refno(target, normals[0].refno).is_lt() {
        return Some(PageId {
            ext_no,
            page_no: start_marker.unwrap_or(normals[0]).page_no,
        });
    }

    let mut previous = normals[0];
    for entry in &normals {
        match compare_refno(target, entry.refno) {
            Ordering::Less => {
                return Some(PageId {
                    ext_no,
                    page_no: previous.page_no,
                });
            }
            Ordering::Equal => {
                return Some(PageId {
                    ext_no,
                    page_no: entry.page_no,
                });
            }
            Ordering::Greater => previous = *entry,
        }
    }

    Some(PageId {
        ext_no,
        page_no: previous.page_no,
    })
}

/// 子页与父页同属一个扩展文件，`ext_no` 由调用方按父页传入。
fn select_child_entry(
    entries: &[IndexEntry],
    target: RefNo,
    ext_no: u32,
) -> Result<ChildSelection, EngineError> {
    let start_marker = entries.iter().position(IndexEntry::is_start_marker);
    let normal_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(idx, entry)| (!entry.is_start_marker()).then_some(idx))
        .collect();

    if normal_indices.is_empty() {
        return Err(EngineError::Format("内部索引页缺少普通边界条目".into()));
    }

    if let Some(&first_idx) = normal_indices.first() {
        let first_entry = &entries[first_idx];
        if compare_refno(target, first_entry.refno).is_lt() {
            let marker_idx = start_marker.unwrap_or(first_idx);
            return Ok(ChildSelection {
                child_page: PageId {
                    ext_no,
                    page_no: entries[marker_idx].page_no,
                },
                boundary_idx: first_idx,
            });
        }
    }

    let mut prev_boundary_idx = normal_indices[0];
    for &idx in &normal_indices {
        let entry = &entries[idx];
        match compare_refno(target, entry.refno) {
            Ordering::Less => {
                return Ok(ChildSelection {
                    child_page: PageId {
                        ext_no,
                        page_no: entries[prev_boundary_idx].page_no,
                    },
                    boundary_idx: prev_boundary_idx,
                });
            }
            Ordering::Equal => {
                return Ok(ChildSelection {
                    child_page: PageId {
                        ext_no,
                        page_no: entry.page_no,
                    },
                    boundary_idx: idx,
                });
            }
            Ordering::Greater => prev_boundary_idx = idx,
        }
    }

    Ok(ChildSelection {
        child_page: PageId {
            ext_no,
            page_no: entries[prev_boundary_idx].page_no,
        },
        boundary_idx: prev_boundary_idx,
    })
}

pub fn search_refno(
    file: &mut File,
    store: &mut PageStore,
    root: PageId,
    target: RefNo,
) -> Result<Option<RecordLoc>, EngineError> {
    IndexCursor::search(file, store, root, target)
}

pub fn write_empty_root(
    file: &mut File,
    store: &mut PageStore,
    page_id: PageId,
) -> Result<(), EngineError> {
    let bytes = serialize_index_page(store.page_size(), &IndexPageView::empty_leaf())?;
    store.write_page(file, page_id, &bytes)?;
    store.flush_dirty(file)?;
    Ok(())
}

pub fn upsert_refno(
    file: &mut File,
    store: &mut PageStore,
    root: PageId,
    refno: RefNo,
    loc: RecordLoc,
) -> Result<PageId, EngineError> {
    // 条目里没有扩展文件号，只有页号，所以记录必须与索引页落在同一个扩展文件里，
    // 否则写出去的条目会指向索引所在文件的同号页。
    if loc.ext_no != root.ext_no {
        return Err(EngineError::Format(format!(
            "记录在扩展文件 {} 而索引根在 {}，索引条目无法表达跨扩展引用",
            loc.ext_no, root.ext_no
        )));
    }

    let new_entry = IndexEntry {
        refno,
        page_no: loc.page_no,
        offset_words: loc.byte_offset / 2,
        flag: 0,
    };

    let page = store.read_page(file, root)?;
    let parsed = IndexPageView::from_page(&page)?;
    insert_into_index_page(file, store, root, parsed, new_entry, true)?;
    store.flush_dirty(file)?;
    Ok(root)
}

fn insert_into_leaf_page(
    file: &mut File,
    store: &mut PageStore,
    page_id: PageId,
    page: IndexPageView,
    new_entry: IndexEntry,
) -> Result<IndexInsertResult, EngineError> {
    let mut entries = page.entries.clone();
    insert_or_replace_sorted(&mut entries, new_entry);

    if entries.len() <= max_entries_per_index_page(store.page_size(), page.stride_dwords()?) {
        let updated = IndexPageView {
            entries: entries.clone(),
            ..page
        };
        let bytes = serialize_index_page(store.page_size(), &updated)?;
        store.write_page(file, page_id, &bytes)?;
        return Ok(IndexInsertResult {
            child_max: leaf_boundary_entry(*entries.last().unwrap(), page_id.page_no),
            split_right: None,
        });
    }

    let (left_entries, right_entries) = split_leaf_entries(&entries);
    let right_page = store.allocate_page(file, page_id.ext_no)?;

    let left_page = IndexPageView {
        entries: left_entries.clone(),
        ..page.clone()
    };
    let right_view = IndexPageView {
        entries: right_entries.clone(),
        ..page
    };

    let left_bytes = serialize_index_page(store.page_size(), &left_page)?;
    let right_bytes = serialize_index_page(store.page_size(), &right_view)?;
    store.write_page(file, page_id, &left_bytes)?;
    store.write_page(file, right_page, &right_bytes)?;

    Ok(IndexInsertResult {
        child_max: leaf_boundary_entry(*left_entries.last().unwrap(), page_id.page_no),
        split_right: Some(leaf_boundary_entry(
            *right_entries.last().unwrap(),
            right_page.page_no,
        )),
    })
}

fn split_root_leaf_page(
    file: &mut File,
    store: &mut PageStore,
    root: PageId,
    page: IndexPageView,
    new_entry: IndexEntry,
) -> Result<(), EngineError> {
    let mut leaf_entries = page.entries.clone();
    insert_or_replace_sorted(&mut leaf_entries, new_entry);

    let (left_entries, right_entries) = split_leaf_entries(&leaf_entries);
    let left_page = store.allocate_page(file, root.ext_no)?;
    let right_page = store.allocate_page(file, root.ext_no)?;

    let left_view = IndexPageView {
        level: 0,
        entries: left_entries.clone(),
        ..page.clone()
    };
    let right_view = IndexPageView {
        level: 0,
        entries: right_entries.clone(),
        ..page.clone()
    };

    let left_bytes = serialize_index_page(store.page_size(), &left_view)?;
    let right_bytes = serialize_index_page(store.page_size(), &right_view)?;
    store.write_page(file, left_page, &left_bytes)?;
    store.write_page(file, right_page, &right_bytes)?;

    let root_entries = vec![
        start_marker_entry(left_page.page_no),
        leaf_boundary_entry(*left_entries.last().unwrap(), left_page.page_no),
        leaf_boundary_entry(*right_entries.last().unwrap(), right_page.page_no),
    ];
    let root_view = IndexPageView {
        level: 1,
        entries: root_entries,
        ..page
    };
    let root_bytes = serialize_index_page(store.page_size(), &root_view)?;
    store.write_page(file, root, &root_bytes)?;
    Ok(())
}

fn split_internal_page(
    file: &mut File,
    store: &mut PageStore,
    page_id: PageId,
    page: IndexPageView,
    entries: Vec<IndexEntry>,
) -> Result<IndexInsertResult, EngineError> {
    let (left_entries, right_entries) = split_internal_entries(&entries)?;
    let right_page = store.allocate_page(file, page_id.ext_no)?;

    let left_view = IndexPageView {
        entries: left_entries.clone(),
        ..page.clone()
    };
    let right_view = IndexPageView {
        entries: right_entries.clone(),
        ..page
    };

    let left_bytes = serialize_index_page(store.page_size(), &left_view)?;
    let right_bytes = serialize_index_page(store.page_size(), &right_view)?;
    store.write_page(file, page_id, &left_bytes)?;
    store.write_page(file, right_page, &right_bytes)?;

    Ok(IndexInsertResult {
        child_max: max_boundary_entry(&left_entries, page_id.page_no)?,
        split_right: Some(max_boundary_entry(&right_entries, right_page.page_no)?),
    })
}

fn split_root_internal_page(
    file: &mut File,
    store: &mut PageStore,
    root: PageId,
    page: IndexPageView,
    entries: Vec<IndexEntry>,
) -> Result<(), EngineError> {
    let (left_entries, right_entries) = split_internal_entries(&entries)?;
    let left_page = store.allocate_page(file, root.ext_no)?;
    let right_page = store.allocate_page(file, root.ext_no)?;

    let left_view = IndexPageView {
        entries: left_entries.clone(),
        ..page.clone()
    };
    let right_view = IndexPageView {
        entries: right_entries.clone(),
        ..page.clone()
    };

    let left_bytes = serialize_index_page(store.page_size(), &left_view)?;
    let right_bytes = serialize_index_page(store.page_size(), &right_view)?;
    store.write_page(file, left_page, &left_bytes)?;
    store.write_page(file, right_page, &right_bytes)?;

    let root_entries = vec![
        start_marker_entry(left_page.page_no),
        max_boundary_entry(&left_entries, left_page.page_no)?,
        max_boundary_entry(&right_entries, right_page.page_no)?,
    ];
    let root_view = IndexPageView {
        level: page.level + 1,
        entries: root_entries,
        ..page
    };
    let root_bytes = serialize_index_page(store.page_size(), &root_view)?;
    store.write_page(file, root, &root_bytes)?;
    Ok(())
}

fn insert_into_index_page(
    file: &mut File,
    store: &mut PageStore,
    page_id: PageId,
    page: IndexPageView,
    new_entry: IndexEntry,
    is_root: bool,
) -> Result<IndexInsertResult, EngineError> {
    let capacity = max_entries_per_index_page(store.page_size(), page.stride_dwords()?);

    if page.level == 0 {
        if page.entries.len() < capacity {
            return insert_into_leaf_page(file, store, page_id, page, new_entry);
        }
        if is_root {
            split_root_leaf_page(file, store, page_id, page, new_entry)?;
            return Ok(IndexInsertResult {
                child_max: IndexEntry {
                    refno: RefNo::new(0),
                    page_no: page_id.page_no,
                    offset_words: 0,
                    flag: 0,
                },
                split_right: None,
            });
        }
        return insert_into_leaf_page(file, store, page_id, page, new_entry);
    }

    let child = select_child_entry(&page.entries, new_entry.refno, page_id.ext_no)?;
    let child_page = store.read_page(file, child.child_page)?;
    let child_view = IndexPageView::from_page(&child_page)?;
    let outcome =
        insert_into_index_page(file, store, child.child_page, child_view, new_entry, false)?;

    let mut entries = page.entries.clone();
    entries[child.boundary_idx] = outcome.child_max;
    if let Some(split_right) = outcome.split_right {
        entries.insert(child.boundary_idx + 1, split_right);
    }

    if entries.len() <= capacity {
        let updated = IndexPageView {
            entries: entries.clone(),
            ..page
        };
        let bytes = serialize_index_page(store.page_size(), &updated)?;
        store.write_page(file, page_id, &bytes)?;
        return Ok(IndexInsertResult {
            child_max: max_boundary_entry(&entries, page_id.page_no)?,
            split_right: None,
        });
    }

    if is_root {
        split_root_internal_page(file, store, page_id, page, entries)?;
        return Ok(IndexInsertResult {
            child_max: IndexEntry {
                refno: RefNo::new(0),
                page_no: page_id.page_no,
                offset_words: 0,
                flag: 0,
            },
            split_right: None,
        });
    }

    split_internal_page(file, store, page_id, page, entries)
}

impl IndexCursor {
    pub fn search(
        file: &mut File,
        store: &mut PageStore,
        root: PageId,
        target: RefNo,
    ) -> Result<Option<RecordLoc>, EngineError> {
        Self::search_recursive(file, store, root, target)
    }

    fn search_recursive(
        file: &mut File,
        store: &mut PageStore,
        page_id: PageId,
        target: RefNo,
    ) -> Result<Option<RecordLoc>, EngineError> {
        let page = store.read_page(file, page_id)?;
        let index = IndexPageView::from_page(&page)?;

        if index.level == 0 {
            return Ok(index
                .entries
                .iter()
                .find(|entry| !entry.is_start_marker() && entry.refno == target)
                .map(|entry| entry.to_record_loc(page_id.ext_no)));
        }

        let child = select_child(&index.entries, target, page_id.ext_no)
            .ok_or_else(|| EngineError::Format("内部索引页缺少有效子页".into()))?;
        Self::search_recursive(file, store, child, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// core.dll 用 `sam7200_0001` 这类真实库；测试在缺库时自动跳过。
    const REAL_DB: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../pdms-test-data/sam7200_0001"
    );

    fn read_u32(page: &[u8], byte_offset: usize) -> u32 {
        u32::from_be_bytes(page[byte_offset..byte_offset + 4].try_into().unwrap())
    }

    fn build_page(page_size: usize, level: u32, entries: &[IndexEntry]) -> Vec<u8> {
        let view = IndexPageView {
            page_type: INDEX_PAGE_TYPE,
            noun: INDEX_PAGE_NOUN,
            level,
            key_dwords: REFNO_KEY_DWORDS,
            value_dwords: RECORD_VALUE_DWORDS,
            reserved: 0,
            free_dwords: 0,
            entries: entries.to_vec(),
        };
        serialize_index_page(page_size, &view).unwrap()
    }

    fn entry(refno: u64, page_no: u32) -> IndexEntry {
        IndexEntry {
            refno: RefNo::new(refno),
            page_no,
            offset_words: 0,
            flag: 0,
        }
    }

    #[test]
    fn serialized_index_page_is_type_5() {
        let page = build_page(2048, 0, &[entry(1, 10)]);
        assert_eq!(
            read_u32(&page, 0),
            5,
            "core.dll sub_5B014E0 只接受 page_type 5"
        );
    }

    #[test]
    fn single_start_marker_internal_page_selects_its_only_child() {
        let entries = vec![start_marker_entry(42)];
        assert_eq!(
            select_child(&entries, RefNo::new(16_191u64 << 32), 7),
            Some(PageId {
                ext_no: 7,
                page_no: 42,
            })
        );
    }

    #[test]
    fn empty_root_declares_full_page_as_free() {
        let page = serialize_index_page(2048, &IndexPageView::empty_leaf()).unwrap();
        // 旧实现写 free=0，core.dll 会把整页零槽当成 (512-0-7)/4 = 126 条有效条目。
        assert_eq!(read_u32(&page, 0x18), 512 - INDEX_PAGE_HEADER_DWORDS as u32);
        assert_eq!(read_u32(&page, 0x0C), REFNO_KEY_DWORDS);
        assert_eq!(read_u32(&page, 0x10), RECORD_VALUE_DWORDS);
        assert!(IndexPageView::from_page(&page).unwrap().entries.is_empty());
    }

    #[test]
    fn free_dwords_tracks_entry_count() {
        for count in [0usize, 1, 7, 126] {
            let entries: Vec<IndexEntry> = (0..count)
                .map(|i| entry(i as u64 + 1, i as u32 + 1))
                .collect();
            let page = build_page(2048, 0, &entries);
            let free = read_u32(&page, 0x18) as usize;
            assert_eq!(free, 512 - INDEX_PAGE_HEADER_DWORDS - count * 4);
            // 与 core.dll sub_5B010F0 的反推公式互为逆运算
            assert_eq!((512 - free - INDEX_PAGE_HEADER_DWORDS) / 4, count);
            assert_eq!(
                IndexPageView::from_page(&page).unwrap().entries.len(),
                count
            );
        }
    }

    #[test]
    fn released_slots_are_not_counted_as_entries() {
        let mut page = build_page(128, 0, &[entry(11, 1), entry(22, 2)]);
        // 模拟真实库里的已释放槽位：内容仍是上一次写入的残留，并非全零。
        let stale = INDEX_PAGE_HEADER_SIZE + 2 * 16;
        page[stale..stale + 16].copy_from_slice(&[0xAB; 16]);

        let parsed = IndexPageView::from_page(&page).unwrap();
        assert_eq!(
            parsed.entries.len(),
            2,
            "条目数必须由 free_dwords 反推，而不是扫描到全零槽为止"
        );
        assert_eq!(parsed.entries[1].refno, RefNo::new(22));
    }

    #[test]
    fn legacy_page_is_upgraded_on_rewrite() {
        // 旧实现写出的页：page_type=8、键值宽为 0、0x18 放的是页号而不是空闲字数。
        let mut page = vec![0u8; 2048];
        page[0..4].copy_from_slice(&8u32.to_be_bytes());
        page[4..8].copy_from_slice(&INDEX_PAGE_NOUN.to_be_bytes());
        page[0x18..0x1C].copy_from_slice(&1383u32.to_be_bytes());

        // 0x18 被当成页号时，(512 - 1383 - 7) 为负，反推出的条目数只能钳到 0。
        let parsed = IndexPageView::from_page(&page).unwrap();
        assert_eq!(parsed.page_type, 8);
        assert!(parsed.entries.is_empty());

        let rewritten = serialize_index_page(2048, &parsed).unwrap();
        assert_eq!(read_u32(&rewritten, 0), INDEX_PAGE_TYPE);
        assert_eq!(read_u32(&rewritten, 0x0C), REFNO_KEY_DWORDS);
        assert_eq!(read_u32(&rewritten, 0x10), RECORD_VALUE_DWORDS);
        assert_eq!(
            read_u32(&rewritten, 0x18),
            512 - INDEX_PAGE_HEADER_DWORDS as u32
        );
    }

    #[test]
    fn variable_width_value_is_rejected() {
        let mut page = vec![0u8; 2048];
        page[0..4].copy_from_slice(&INDEX_PAGE_TYPE.to_be_bytes());
        page[4..8].copy_from_slice(&INDEX_PAGE_NOUN.to_be_bytes());
        page[0x0C..0x10].copy_from_slice(&2u32.to_be_bytes());
        page[0x10..0x14].copy_from_slice(&(-1i32).to_be_bytes());

        assert!(IndexPageView::from_page(&page).is_err());
    }

    #[test]
    fn narrow_key_width_is_rejected() {
        let mut page = vec![0u8; 2048];
        page[0..4].copy_from_slice(&INDEX_PAGE_TYPE.to_be_bytes());
        page[4..8].copy_from_slice(&INDEX_PAGE_NOUN.to_be_bytes());
        page[0x0C..0x10].copy_from_slice(&1u32.to_be_bytes());
        page[0x10..0x14].copy_from_slice(&RECORD_VALUE_DWORDS.to_be_bytes());

        // 键宽 1 放不下 8 字节的 RefNo，宁可报错也不要按 2 硬读越界的字节。
        assert!(IndexPageView::from_page(&page).is_err());
    }

    /// 键宽决定值在条目内的偏移。读写两侧都必须从同一处取宽度，否则宽键的表会错位。
    #[test]
    fn value_offset_follows_declared_key_width() {
        let view = IndexPageView {
            page_type: INDEX_PAGE_TYPE,
            noun: INDEX_PAGE_NOUN,
            level: 0,
            key_dwords: 3,
            value_dwords: RECORD_VALUE_DWORDS,
            reserved: 0,
            free_dwords: 0,
            entries: vec![entry(11, 77), entry(22, 88)],
        };
        let page = serialize_index_page(2048, &view).unwrap();

        assert_eq!(read_u32(&page, 0x0C), 3);
        // 步长 3+2=5 个字，值区从条目起始的第 3 个字开始。
        assert_eq!(read_u32(&page, INDEX_PAGE_HEADER_SIZE + 12), 77);
        assert_eq!(read_u32(&page, INDEX_PAGE_HEADER_SIZE + 20 + 12), 88);
        assert_eq!(
            read_u32(&page, 0x18),
            512 - INDEX_PAGE_HEADER_DWORDS as u32 - 2 * 5
        );

        let parsed = IndexPageView::from_page(&page).unwrap();
        assert_eq!(parsed.entries, view.entries);
    }

    /// 条目只存页号，扩展文件号必须从承载它的索引页继承，不能假定是 1 号扩展。
    #[test]
    fn locations_inherit_the_extension_of_their_page() {
        assert_eq!(entry(1, 10).to_record_loc(7).ext_no, 7);

        let entries = vec![start_marker_entry(100), entry(10, 101), entry(20, 102)];
        let picked = select_child(&entries, RefNo::new(15), 7).unwrap();
        assert_eq!(picked.ext_no, 7);
        assert_eq!(picked.page_no, 101);

        let selected = select_child_entry(&entries, RefNo::new(5), 7).unwrap();
        assert_eq!(selected.child_page.ext_no, 7);
        assert_eq!(selected.child_page.page_no, 100);
    }

    /// 把 core.dll 亲手写出的索引页读进来再序列化回去，头部与生效条目区必须逐字节一致。
    #[test]
    fn real_db_index_pages_round_trip_byte_for_byte() {
        let Ok(blob) = std::fs::read(REAL_DB) else {
            println!("缺少测试库，跳过: {}", REAL_DB);
            return;
        };

        let page_size = read_u32(&blob, 0x34) as usize * 4;
        let mut checked = 0usize;
        let mut pages_with_stale_slots = 0usize;

        for base in (0..blob.len() - page_size + 1).step_by(page_size) {
            let page = &blob[base..base + page_size];
            if read_u32(page, 4) != INDEX_PAGE_NOUN {
                continue;
            }

            let parsed = IndexPageView::from_page(page).unwrap();
            assert_eq!(parsed.page_type, INDEX_PAGE_TYPE, "页 {}", base / page_size);

            let stride = parsed.stride_dwords().unwrap();
            let live_len = INDEX_PAGE_HEADER_SIZE + parsed.entries.len() * stride * 4;
            let rewritten = serialize_index_page(page_size, &parsed).unwrap();
            assert_eq!(
                &rewritten[..live_len],
                &page[..live_len],
                "页 {} 的头部或生效条目区与 core.dll 的写法不一致",
                base / page_size
            );

            if page[live_len..].iter().any(|&b| b != 0) {
                pages_with_stale_slots += 1;
            }
            checked += 1;
        }

        assert!(checked > 0, "测试库里没有索引页");
        assert!(
            pages_with_stale_slots > 0,
            "该库应当存在带残留槽位的页，否则这条测试无法证伪扫描到全零为止的读法"
        );
        println!(
            "校验索引页 {} 个，其中 {} 个带残留槽位",
            checked, pages_with_stale_slots
        );
    }

    /// 遍历最新会话的整棵树，收集叶子条目。`count_by_flag` 为真时改用槽位有效
    /// 标志决定条目数，用来对照 `free_dwords` 的读法。
    fn walk_live_tree(blob: &[u8], page_size: usize, root: u32, count_by_flag: bool) -> Vec<RefNo> {
        let capacity = max_entries_per_index_page(page_size, 4);
        let mut found = Vec::new();
        let mut stack = vec![root];
        let mut seen = std::collections::HashSet::new();

        while let Some(pgno) = stack.pop() {
            if !seen.insert(pgno) {
                continue;
            }
            let base = pgno as usize * page_size;
            if base + page_size > blob.len() {
                continue;
            }
            let page = &blob[base..base + page_size];
            if read_u32(page, 4) != INDEX_PAGE_NOUN {
                continue;
            }

            let parsed = IndexPageView::from_page(page).unwrap();
            let entries = if count_by_flag {
                (0..capacity)
                    .map(|i| {
                        let off = INDEX_PAGE_HEADER_SIZE + i * 16;
                        IndexEntry {
                            refno: RefNo::from_parts(read_u32(page, off), read_u32(page, off + 4)),
                            page_no: read_u32(page, off + 8),
                            offset_words: read_u32(page, off + 12) >> 12,
                            flag: (read_u32(page, off + 12) & 0x0FFF) as u16,
                        }
                    })
                    .filter(|e| e.flag != 0)
                    .collect()
            } else {
                parsed.entries.clone()
            };

            for entry in entries {
                if parsed.level != 0 {
                    stack.push(entry.page_no);
                } else if !entry.is_start_marker() {
                    found.push(entry.refno);
                }
            }
        }
        found
    }

    /// core.dll 不会清理离开生效区的槽位，所以槽位有效标志会读出分裂/删除后的
    /// 残留。`free_dwords` 才是权威：按它遍历整棵树不会撞见重复 refno，按标志
    /// 遍历会。这条测试守住条目数的来源，防止退回按标志或扫描到全零的读法。
    #[test]
    fn free_dwords_walk_of_live_tree_has_no_duplicate_refnos() {
        let Ok(blob) = std::fs::read(REAL_DB) else {
            println!("缺少测试库，跳过: {}", REAL_DB);
            return;
        };

        let page_size = read_u32(&blob, 0x34) as usize * 4;
        let session_page = read_u32(&blob, 0x28) as usize * page_size;
        let root = read_u32(&blob[session_page..], 0x1C);

        let by_free = walk_live_tree(&blob, page_size, root, false);
        let unique: std::collections::HashSet<RefNo> = by_free.iter().copied().collect();
        assert_eq!(
            by_free.len(),
            unique.len(),
            "按 free_dwords 遍历生效树时出现了 {} 个重复 refno",
            by_free.len() - unique.len()
        );

        let by_flag = walk_live_tree(&blob, page_size, root, true);
        let unique_flag: std::collections::HashSet<RefNo> = by_flag.iter().copied().collect();
        assert!(
            by_flag.len() > unique_flag.len(),
            "该库应当存在残留标志导致的重复条目，否则这条测试无法证伪按标志计数的读法"
        );
        println!(
            "生效树叶子条目 {}（无重复）；按标志计数会多出 {} 条，其中 {} 条重复",
            by_free.len(),
            by_flag.len() - by_free.len(),
            by_flag.len() - unique_flag.len()
        );
    }
}
