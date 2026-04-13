use std::cmp::Ordering;
use std::fs::File;

use crate::core::{EngineError, PageId, RecordLoc, RefNo};
use crate::db1::PageStore;

const INDEX_PAGE_NOUN: u32 = 0x00CC47DF;
const INDEX_PAGE_HEADER_SIZE: usize = 0x1C;
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

    pub fn to_record_loc(&self) -> RecordLoc {
        RecordLoc {
            ext_no: 1,
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
    pub unknowns: [u32; 3],
    pub pfno: u32,
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
        let unknowns = [
            u32::from_be_bytes(page[12..16].try_into().unwrap()),
            u32::from_be_bytes(page[16..20].try_into().unwrap()),
            u32::from_be_bytes(page[20..24].try_into().unwrap()),
        ];
        let pfno = u32::from_be_bytes(page[24..28].try_into().unwrap());

        let mut entries = Vec::new();
        for offset in (INDEX_PAGE_HEADER_SIZE..page.len()).step_by(16) {
            if offset + 16 > page.len() {
                break;
            }
            let slot = &page[offset..offset + 16];
            if slot.iter().all(|&b| b == 0) {
                break;
            }

            let refno = RefNo::from_parts(
                u32::from_be_bytes(slot[0..4].try_into().unwrap()),
                u32::from_be_bytes(slot[4..8].try_into().unwrap()),
            );
            let page_no = u32::from_be_bytes(slot[8..12].try_into().unwrap());
            let packed = u32::from_be_bytes(slot[12..16].try_into().unwrap());
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
            unknowns,
            pfno,
            entries,
        })
    }

    fn empty_leaf() -> Self {
        Self {
            page_type: 8,
            noun: INDEX_PAGE_NOUN,
            level: 0,
            unknowns: [0; 3],
            pfno: 0,
            entries: Vec::new(),
        }
    }
}

fn compare_refno(a: RefNo, b: RefNo) -> Ordering {
    (a.hi(), a.lo()).cmp(&(b.hi(), b.lo()))
}

fn max_entries_per_index_page(page_size: usize) -> usize {
    page_size.saturating_sub(INDEX_PAGE_HEADER_SIZE) / 16
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

pub(crate) fn serialize_index_page(page_size: usize, page: &IndexPageView) -> Result<Vec<u8>, EngineError> {
    let max_entries = max_entries_per_index_page(page_size);
    if page.entries.len() > max_entries {
        return Err(EngineError::Format(format!(
            "索引页面条目数 {} 超过容量 {}",
            page.entries.len(),
            max_entries
        )));
    }

    let mut data = vec![0u8; page_size];
    data[0..4].copy_from_slice(&page.page_type.to_be_bytes());
    data[4..8].copy_from_slice(&INDEX_PAGE_NOUN.to_be_bytes());
    data[8..12].copy_from_slice(&page.level.to_be_bytes());
    data[12..16].copy_from_slice(&page.unknowns[0].to_be_bytes());
    data[16..20].copy_from_slice(&page.unknowns[1].to_be_bytes());
    data[20..24].copy_from_slice(&page.unknowns[2].to_be_bytes());
    data[24..28].copy_from_slice(&page.pfno.to_be_bytes());

    for (index, entry) in page.entries.iter().enumerate() {
        let offset = INDEX_PAGE_HEADER_SIZE + index * 16;
        data[offset..offset + 4].copy_from_slice(&entry.refno.hi().to_be_bytes());
        data[offset + 4..offset + 8].copy_from_slice(&entry.refno.lo().to_be_bytes());
        data[offset + 8..offset + 12].copy_from_slice(&entry.page_no.to_be_bytes());
        let packed = (entry.offset_words << 12) | entry.flag as u32;
        data[offset + 12..offset + 16].copy_from_slice(&packed.to_be_bytes());
    }

    Ok(data)
}

fn select_child(entries: &[IndexEntry], target: RefNo) -> Option<PageId> {
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
        return None;
    }

    if compare_refno(target, normals[0].refno).is_lt() {
        return Some(PageId {
            ext_no: 1,
            page_no: start_marker.unwrap_or(normals[0]).page_no,
        });
    }

    let mut previous = normals[0];
    for entry in &normals {
        match compare_refno(target, entry.refno) {
            Ordering::Less => {
                return Some(PageId {
                    ext_no: 1,
                    page_no: previous.page_no,
                });
            }
            Ordering::Equal => {
                return Some(PageId {
                    ext_no: 1,
                    page_no: entry.page_no,
                });
            }
            Ordering::Greater => previous = *entry,
        }
    }

    Some(PageId {
        ext_no: 1,
        page_no: previous.page_no,
    })
}

fn select_child_entry(
    entries: &[IndexEntry],
    target: RefNo,
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
                    ext_no: 1,
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
                        ext_no: 1,
                        page_no: entries[prev_boundary_idx].page_no,
                    },
                    boundary_idx: prev_boundary_idx,
                });
            }
            Ordering::Equal => {
                return Ok(ChildSelection {
                    child_page: PageId {
                        ext_no: 1,
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
            ext_no: 1,
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

    if entries.len() <= max_entries_per_index_page(store.page_size()) {
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
        pfno: page_id.page_no,
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
        page_type: page.page_type,
        noun: page.noun,
        level: 0,
        unknowns: page.unknowns,
        pfno: 0,
        entries: left_entries.clone(),
    };
    let right_view = IndexPageView {
        page_type: page.page_type,
        noun: page.noun,
        level: 0,
        unknowns: page.unknowns,
        pfno: left_page.page_no,
        entries: right_entries.clone(),
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
        page_type: page.page_type,
        noun: page.noun,
        level: 1,
        unknowns: page.unknowns,
        pfno: 0,
        entries: root_entries,
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
        pfno: page_id.page_no,
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
        pfno: left_page.page_no,
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
        page_type: page.page_type,
        noun: page.noun,
        level: page.level + 1,
        unknowns: page.unknowns,
        pfno: 0,
        entries: root_entries,
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
    if page.level == 0 {
        if page.entries.len() < max_entries_per_index_page(store.page_size()) {
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

    let child = select_child_entry(&page.entries, new_entry.refno)?;
    let child_page = store.read_page(file, child.child_page)?;
    let child_view = IndexPageView::from_page(&child_page)?;
    let outcome =
        insert_into_index_page(file, store, child.child_page, child_view, new_entry, false)?;

    let mut entries = page.entries.clone();
    entries[child.boundary_idx] = outcome.child_max;
    if let Some(split_right) = outcome.split_right {
        entries.insert(child.boundary_idx + 1, split_right);
    }

    if entries.len() <= max_entries_per_index_page(store.page_size()) {
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
                .map(IndexEntry::to_record_loc));
        }

        let child = select_child(&index.entries, target)
            .ok_or_else(|| EngineError::Format("内部索引页缺少有效子页".into()))?;
        Self::search_recursive(file, store, child, target)
    }
}
