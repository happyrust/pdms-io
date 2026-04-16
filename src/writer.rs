//! 数据库写入模块
//!
//! 基于 IDA Pro 逆向分析实现的页面写入和元素序列化功能。
//!
//! # 设计参考
//! - db1_write_page: 写入单个页面到磁盘
//! - db5_save_work: 批量刷新所有脏页
//! - FHDBWN: Fortran 风格的底层写入封装

use std::cmp::Ordering;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};

use crate::defines::*;
use crate::page_manager::PageManager;

const PRIMARY_EXT_NO: u32 = 1;
// 现有读取侧 IndexPageData 将 refno_locs 视作从 0x1C 开始；
// 写端若保留 0x24 头长，reopen 后会把中间 8 字节零填充误判为“空索引页”。
const INDEX_PAGE_HEADER_SIZE: usize = 0x1C;
const INDEX_PAGE_NOUN: u32 = 0x00CC47DF;
const START_MARKER_REFNO_0: u32 = 0x80000001;
const START_MARKER_REFNO_1: u32 = 0x80000001;

/// 写入操作错误类型
#[derive(Debug)]
pub enum WriteError {
    /// I/O 错误
    Io(std::io::Error),
    /// 页面大小不匹配
    PageSizeMismatch { expected: usize, actual: usize },
    /// 索引未找到
    IndexNotFound(u64),
    /// 序列化错误
    SerializationError(String),
}

impl From<std::io::Error> for WriteError {
    fn from(err: std::io::Error) -> Self {
        WriteError::Io(err)
    }
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::Io(e) => write!(f, "I/O 错误: {}", e),
            WriteError::PageSizeMismatch { expected, actual } => {
                write!(
                    f,
                    "页面大小不匹配: 期望 {} 字节, 实际 {} 字节",
                    expected, actual
                )
            }
            WriteError::IndexNotFound(refno) => write!(f, "索引未找到: refno={}", refno),
            WriteError::SerializationError(msg) => write!(f, "序列化错误: {}", msg),
        }
    }
}

impl std::error::Error for WriteError {}

/// 写入统计
#[derive(Debug, Default, Clone)]
pub struct WriteStats {
    /// 已写入页面数
    pub pages_written: u64,
    /// 已写入字节数
    pub bytes_written: u64,
    /// 索引更新次数
    pub index_updates: u64,
}

/// 元素写入器
///
/// 提供元素级别的写入操作，封装页面管理和索引更新逻辑。
pub struct ElementWriter {
    /// 页面管理器
    page_manager: PageManager,
    /// 页面大小
    page_size: usize,
    /// 当前索引根页号
    current_index_root_pgno: Option<u32>,
    /// 当前会话首个新分配页面号，用作最小 claim 头
    current_claim_pgno: Option<u32>,
    /// 写入统计
    stats: WriteStats,
}

#[derive(Debug, Clone)]
struct ParsedIndexPage {
    page_type: u32,
    level: u32,
    unknowns: [u32; 3],
    pfno: u32,
    entries: Vec<RefnoDataLoc>,
}

#[derive(Debug)]
struct ChildSelection {
    child_pgno: u32,
    boundary_idx: usize,
}

#[derive(Debug)]
struct IndexInsertResult {
    child_max: RefnoDataLoc,
    split_right: Option<RefnoDataLoc>,
}

impl ElementWriter {
    fn is_start_marker(loc: &RefnoDataLoc) -> bool {
        loc.refno_0 == START_MARKER_REFNO_0 && loc.refno_1 == START_MARKER_REFNO_1
    }

    fn compare_refno(a: &RefnoDataLoc, b: &RefnoDataLoc) -> Ordering {
        (a.refno_0, a.refno_1).cmp(&(b.refno_0, b.refno_1))
    }

    fn max_entries_per_index_page(&self) -> usize {
        (self.page_size.saturating_sub(INDEX_PAGE_HEADER_SIZE)) / 16
    }

    fn parse_index_page(&self, page_data: &[u8]) -> Result<ParsedIndexPage, WriteError> {
        if page_data.len() < INDEX_PAGE_HEADER_SIZE {
            return Err(WriteError::SerializationError("索引页面数据太短".into()));
        }

        let page_type =
            u32::from_be_bytes([page_data[0], page_data[1], page_data[2], page_data[3]]);
        let noun = u32::from_be_bytes([page_data[4], page_data[5], page_data[6], page_data[7]]);
        if noun != INDEX_PAGE_NOUN {
            return Err(WriteError::SerializationError(format!(
                "索引页面 noun 非法: 0x{:X}",
                noun
            )));
        }

        let level = u32::from_be_bytes([page_data[8], page_data[9], page_data[10], page_data[11]]);
        let unknowns = [
            u32::from_be_bytes([page_data[12], page_data[13], page_data[14], page_data[15]]),
            u32::from_be_bytes([page_data[16], page_data[17], page_data[18], page_data[19]]),
            u32::from_be_bytes([page_data[20], page_data[21], page_data[22], page_data[23]]),
        ];
        let pfno = u32::from_be_bytes([page_data[24], page_data[25], page_data[26], page_data[27]]);

        let mut entries = Vec::new();
        for offset in (INDEX_PAGE_HEADER_SIZE..page_data.len()).step_by(16) {
            if offset + 16 > page_data.len() {
                break;
            }
            let slot = &page_data[offset..offset + 16];
            if slot.iter().all(|&b| b == 0) {
                break;
            }

            let refno_0 = u32::from_be_bytes(slot[0..4].try_into().unwrap());
            let refno_1 = u32::from_be_bytes(slot[4..8].try_into().unwrap());
            let pgno = u32::from_be_bytes(slot[8..12].try_into().unwrap());
            let packed = u32::from_be_bytes(slot[12..16].try_into().unwrap());
            entries.push(RefnoDataLoc {
                refno_0,
                refno_1,
                pgno,
                offset: packed >> 12,
                flag: (packed & 0xFFF) as u16,
            });
        }

        Ok(ParsedIndexPage {
            page_type,
            level,
            unknowns,
            pfno,
            entries,
        })
    }

    fn serialize_index_page(&self, page: &ParsedIndexPage) -> Result<Vec<u8>, WriteError> {
        let max_entries = self.max_entries_per_index_page();
        if page.entries.len() > max_entries {
            return Err(WriteError::SerializationError(format!(
                "索引页面条目数 {} 超过容量 {}",
                page.entries.len(),
                max_entries
            )));
        }

        let mut data = vec![0u8; self.page_size];
        data[0..4].copy_from_slice(&page.page_type.to_be_bytes());
        data[4..8].copy_from_slice(&INDEX_PAGE_NOUN.to_be_bytes());
        data[8..12].copy_from_slice(&page.level.to_be_bytes());
        data[12..16].copy_from_slice(&page.unknowns[0].to_be_bytes());
        data[16..20].copy_from_slice(&page.unknowns[1].to_be_bytes());
        data[20..24].copy_from_slice(&page.unknowns[2].to_be_bytes());
        data[24..28].copy_from_slice(&page.pfno.to_be_bytes());

        for (index, entry) in page.entries.iter().enumerate() {
            let offset = INDEX_PAGE_HEADER_SIZE + index * 16;
            data[offset..offset + 4].copy_from_slice(&entry.refno_0.to_be_bytes());
            data[offset + 4..offset + 8].copy_from_slice(&entry.refno_1.to_be_bytes());
            data[offset + 8..offset + 12].copy_from_slice(&entry.pgno.to_be_bytes());
            let packed = (entry.offset << 12) | entry.flag as u32;
            data[offset + 12..offset + 16].copy_from_slice(&packed.to_be_bytes());
        }

        Ok(data)
    }

    fn insert_or_replace_sorted(entries: &mut Vec<RefnoDataLoc>, loc: RefnoDataLoc) {
        if let Some(existing_idx) = entries
            .iter()
            .position(|entry| entry.refno_0 == loc.refno_0 && entry.refno_1 == loc.refno_1)
        {
            entries[existing_idx] = loc;
            return;
        }

        let insert_at = entries
            .iter()
            .position(|entry| Self::compare_refno(&loc, entry).is_lt())
            .unwrap_or(entries.len());
        entries.insert(insert_at, loc);
    }

    fn leaf_boundary_entry(entry: &RefnoDataLoc, child_pgno: u32) -> RefnoDataLoc {
        RefnoDataLoc {
            refno_0: entry.refno_0,
            refno_1: entry.refno_1,
            pgno: child_pgno,
            offset: 0,
            flag: 0,
        }
    }

    fn start_marker_entry(child_pgno: u32) -> RefnoDataLoc {
        RefnoDataLoc {
            refno_0: START_MARKER_REFNO_0,
            refno_1: START_MARKER_REFNO_1,
            pgno: child_pgno,
            offset: 0,
            flag: 0,
        }
    }

    fn max_boundary_entry(
        entries: &[RefnoDataLoc],
        page_no: u32,
    ) -> Result<RefnoDataLoc, WriteError> {
        let last = entries
            .iter()
            .rfind(|entry| !Self::is_start_marker(entry))
            .ok_or_else(|| WriteError::SerializationError("索引页缺少边界条目".into()))?;
        Ok(Self::leaf_boundary_entry(last, page_no))
    }

    fn split_leaf_entries(entries: &[RefnoDataLoc]) -> (Vec<RefnoDataLoc>, Vec<RefnoDataLoc>) {
        let mid = entries.len() / 2;
        (entries[..mid].to_vec(), entries[mid..].to_vec())
    }

    fn split_internal_entries(
        entries: &[RefnoDataLoc],
    ) -> Result<(Vec<RefnoDataLoc>, Vec<RefnoDataLoc>), WriteError> {
        if entries.is_empty() || !Self::is_start_marker(&entries[0]) {
            return Err(WriteError::SerializationError(
                "内部索引页缺少起始标记".into(),
            ));
        }

        let normals = &entries[1..];
        if normals.len() < 2 {
            return Err(WriteError::SerializationError(
                "内部索引页条目不足，无法分裂".into(),
            ));
        }

        let mid = normals.len() / 2;
        let left_normals = normals[..mid].to_vec();
        let right_normals = normals[mid..].to_vec();
        if left_normals.is_empty() || right_normals.is_empty() {
            return Err(WriteError::SerializationError(
                "内部索引页分裂后出现空子树".into(),
            ));
        }

        let mut left_entries = Vec::with_capacity(left_normals.len() + 1);
        left_entries.push(Self::start_marker_entry(entries[0].pgno));
        left_entries.extend(left_normals);

        let mut right_entries = Vec::with_capacity(right_normals.len() + 1);
        right_entries.push(Self::start_marker_entry(right_normals[0].pgno));
        right_entries.extend(right_normals);

        Ok((left_entries, right_entries))
    }

    fn split_root_leaf_page(
        &mut self,
        file: &mut File,
        ext_no: u32,
        root_page_no: u32,
        page: ParsedIndexPage,
        new_loc: &RefnoDataLoc,
    ) -> Result<(), WriteError> {
        let mut leaf_entries = page.entries.clone();
        Self::insert_or_replace_sorted(&mut leaf_entries, new_loc.clone());

        let (left_entries, right_entries) = Self::split_leaf_entries(&leaf_entries);
        let left_pgno = self.allocate_page(file)?;
        let right_pgno = self.allocate_page(file)?;

        let left_page = ParsedIndexPage {
            page_type: page.page_type,
            level: 0,
            unknowns: page.unknowns,
            pfno: 0,
            entries: left_entries.clone(),
        };
        let right_page = ParsedIndexPage {
            page_type: page.page_type,
            level: 0,
            unknowns: page.unknowns,
            pfno: left_pgno,
            entries: right_entries.clone(),
        };

        let left_bytes = self.serialize_index_page(&left_page)?;
        let right_bytes = self.serialize_index_page(&right_page)?;
        self.page_manager
            .write_page(file, ext_no, left_pgno, &left_bytes)?;
        self.page_manager
            .write_page(file, ext_no, right_pgno, &right_bytes)?;

        let root_entries = vec![
            Self::start_marker_entry(left_pgno),
            Self::leaf_boundary_entry(left_entries.last().unwrap(), left_pgno),
            Self::leaf_boundary_entry(right_entries.last().unwrap(), right_pgno),
        ];
        let root_page = ParsedIndexPage {
            page_type: page.page_type,
            level: 1,
            unknowns: page.unknowns,
            pfno: 0,
            entries: root_entries,
        };
        let root_bytes = self.serialize_index_page(&root_page)?;
        self.page_manager
            .write_page(file, ext_no, root_page_no, &root_bytes)?;

        Ok(())
    }

    fn split_internal_page(
        &mut self,
        file: &mut File,
        ext_no: u32,
        page_no: u32,
        page: ParsedIndexPage,
        entries: Vec<RefnoDataLoc>,
    ) -> Result<IndexInsertResult, WriteError> {
        let (left_entries, right_entries) = Self::split_internal_entries(&entries)?;
        let right_pgno = self.allocate_page(file)?;

        let left_page = ParsedIndexPage {
            entries: left_entries.clone(),
            ..page.clone()
        };
        let right_page = ParsedIndexPage {
            entries: right_entries.clone(),
            pfno: page_no,
            ..page
        };

        let left_bytes = self.serialize_index_page(&left_page)?;
        let right_bytes = self.serialize_index_page(&right_page)?;
        self.page_manager
            .write_page(file, ext_no, page_no, &left_bytes)?;
        self.page_manager
            .write_page(file, ext_no, right_pgno, &right_bytes)?;

        Ok(IndexInsertResult {
            child_max: Self::max_boundary_entry(&left_entries, page_no)?,
            split_right: Some(Self::max_boundary_entry(&right_entries, right_pgno)?),
        })
    }

    fn split_root_internal_page(
        &mut self,
        file: &mut File,
        ext_no: u32,
        root_page_no: u32,
        page: ParsedIndexPage,
        entries: Vec<RefnoDataLoc>,
    ) -> Result<(), WriteError> {
        let (left_entries, right_entries) = Self::split_internal_entries(&entries)?;
        let left_pgno = self.allocate_page(file)?;
        let right_pgno = self.allocate_page(file)?;

        let left_page = ParsedIndexPage {
            entries: left_entries.clone(),
            ..page.clone()
        };
        let right_page = ParsedIndexPage {
            entries: right_entries.clone(),
            pfno: left_pgno,
            ..page.clone()
        };

        let left_bytes = self.serialize_index_page(&left_page)?;
        let right_bytes = self.serialize_index_page(&right_page)?;
        self.page_manager
            .write_page(file, ext_no, left_pgno, &left_bytes)?;
        self.page_manager
            .write_page(file, ext_no, right_pgno, &right_bytes)?;

        let root_entries = vec![
            Self::start_marker_entry(left_pgno),
            Self::max_boundary_entry(&left_entries, left_pgno)?,
            Self::max_boundary_entry(&right_entries, right_pgno)?,
        ];
        let root_page = ParsedIndexPage {
            page_type: page.page_type,
            level: page.level + 1,
            unknowns: page.unknowns,
            pfno: 0,
            entries: root_entries,
        };
        let root_bytes = self.serialize_index_page(&root_page)?;
        self.page_manager
            .write_page(file, ext_no, root_page_no, &root_bytes)?;

        Ok(())
    }

    fn select_child_entry(
        entries: &[RefnoDataLoc],
        target: &RefnoDataLoc,
    ) -> Result<ChildSelection, WriteError> {
        let start_marker = entries.iter().position(Self::is_start_marker);
        let normal_indices: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| (!Self::is_start_marker(entry)).then_some(idx))
            .collect();

        if normal_indices.is_empty() {
            return Err(WriteError::SerializationError(
                "内部索引页缺少普通边界条目".into(),
            ));
        }

        if let Some(&first_idx) = normal_indices.first() {
            let first_entry = &entries[first_idx];
            if Self::compare_refno(target, first_entry).is_lt() {
                let marker_idx = start_marker.unwrap_or(first_idx);
                let marker_pg = entries[marker_idx].pgno;
                return Ok(ChildSelection {
                    child_pgno: marker_pg,
                    boundary_idx: first_idx,
                });
            }
        }

        let mut prev_boundary_idx = normal_indices[0];
        for &idx in &normal_indices {
            let entry = &entries[idx];
            match Self::compare_refno(target, entry) {
                Ordering::Less => {
                    return Ok(ChildSelection {
                        child_pgno: entries[prev_boundary_idx].pgno,
                        boundary_idx: prev_boundary_idx,
                    });
                }
                Ordering::Equal => {
                    return Ok(ChildSelection {
                        child_pgno: entry.pgno,
                        boundary_idx: idx,
                    });
                }
                Ordering::Greater => {
                    prev_boundary_idx = idx;
                }
            }
        }

        Ok(ChildSelection {
            child_pgno: entries[prev_boundary_idx].pgno,
            boundary_idx: prev_boundary_idx,
        })
    }

    fn insert_into_leaf_page(
        &mut self,
        file: &mut File,
        ext_no: u32,
        page_no: u32,
        page: ParsedIndexPage,
        new_loc: &RefnoDataLoc,
    ) -> Result<IndexInsertResult, WriteError> {
        let mut entries = page.entries.clone();
        Self::insert_or_replace_sorted(&mut entries, new_loc.clone());

        if entries.len() <= self.max_entries_per_index_page() {
            let updated_page = ParsedIndexPage {
                entries: entries.clone(),
                ..page
            };
            let bytes = self.serialize_index_page(&updated_page)?;
            self.page_manager
                .write_page(file, ext_no, page_no, &bytes)?;
            return Ok(IndexInsertResult {
                child_max: Self::leaf_boundary_entry(entries.last().unwrap(), page_no),
                split_right: None,
            });
        }

        let (left_entries, right_entries) = Self::split_leaf_entries(&entries);
        let right_pgno = self.allocate_page(file)?;

        let left_page = ParsedIndexPage {
            entries: left_entries.clone(),
            ..page.clone()
        };
        let right_page = ParsedIndexPage {
            entries: right_entries.clone(),
            pfno: page_no,
            ..page
        };

        let left_bytes = self.serialize_index_page(&left_page)?;
        let right_bytes = self.serialize_index_page(&right_page)?;
        self.page_manager
            .write_page(file, ext_no, page_no, &left_bytes)?;
        self.page_manager
            .write_page(file, ext_no, right_pgno, &right_bytes)?;

        Ok(IndexInsertResult {
            child_max: Self::leaf_boundary_entry(left_entries.last().unwrap(), page_no),
            split_right: Some(Self::leaf_boundary_entry(
                right_entries.last().unwrap(),
                right_pgno,
            )),
        })
    }

    fn insert_into_index_page(
        &mut self,
        file: &mut File,
        ext_no: u32,
        page_no: u32,
        page: ParsedIndexPage,
        new_loc: &RefnoDataLoc,
        is_root: bool,
    ) -> Result<IndexInsertResult, WriteError> {
        if page.level == 0 {
            if page.entries.len() < self.max_entries_per_index_page() {
                return self.insert_into_leaf_page(file, ext_no, page_no, page, new_loc);
            }
            if is_root {
                self.split_root_leaf_page(file, ext_no, page_no, page, new_loc)?;
                return Ok(IndexInsertResult {
                    child_max: RefnoDataLoc {
                        refno_0: 0,
                        refno_1: 0,
                        pgno: page_no,
                        offset: 0,
                        flag: 0,
                    },
                    split_right: None,
                });
            }
            return self.insert_into_leaf_page(file, ext_no, page_no, page, new_loc);
        }

        let child = Self::select_child_entry(&page.entries, new_loc)?;
        let child_page_data = self
            .page_manager
            .get_page(file, ext_no, child.child_pgno)
            .map_err(WriteError::Io)?
            .to_vec();
        let child_page = self.parse_index_page(&child_page_data)?;
        let outcome = self.insert_into_index_page(
            file,
            ext_no,
            child.child_pgno,
            child_page,
            new_loc,
            false,
        )?;

        let mut entries = page.entries.clone();
        entries[child.boundary_idx] = outcome.child_max;
        if let Some(split_right) = outcome.split_right {
            entries.insert(child.boundary_idx + 1, split_right);
        }

        if entries.len() <= self.max_entries_per_index_page() {
            let updated_page = ParsedIndexPage {
                entries: entries.clone(),
                ..page
            };
            let bytes = self.serialize_index_page(&updated_page)?;
            self.page_manager
                .write_page(file, ext_no, page_no, &bytes)?;
            return Ok(IndexInsertResult {
                child_max: Self::max_boundary_entry(&entries, page_no)?,
                split_right: None,
            });
        }

        if is_root {
            self.split_root_internal_page(file, ext_no, page_no, page, entries)?;
            return Ok(IndexInsertResult {
                child_max: RefnoDataLoc {
                    refno_0: 0,
                    refno_1: 0,
                    pgno: page_no,
                    offset: 0,
                    flag: 0,
                },
                split_right: None,
            });
        }

        self.split_internal_page(file, ext_no, page_no, page, entries)
    }

    fn update_index_entry_recursive(
        &mut self,
        file: &mut File,
        ext_no: u32,
        page_no: u32,
        refno_loc: &RefnoDataLoc,
    ) -> Result<(), WriteError> {
        let page_data = self
            .page_manager
            .get_page(file, ext_no, page_no)
            .map_err(WriteError::Io)?
            .to_vec();
        let parsed = self.parse_index_page(&page_data)?;

        if parsed.level == 0 {
            let mut updated_page = parsed.clone();
            if let Some(entry) = updated_page.entries.iter_mut().find(|entry| {
                entry.refno_0 == refno_loc.refno_0 && entry.refno_1 == refno_loc.refno_1
            }) {
                *entry = refno_loc.clone();
                let bytes = self.serialize_index_page(&updated_page)?;
                self.page_manager
                    .write_page(file, ext_no, page_no, &bytes)?;
                return Ok(());
            }
            return Err(WriteError::IndexNotFound(
                ((refno_loc.refno_0 as u64) << 32) | (refno_loc.refno_1 as u64),
            ));
        }

        let preferred = Self::select_child_entry(&parsed.entries, refno_loc)?.child_pgno;
        let mut child_pgno_candidates = vec![preferred];
        for entry in &parsed.entries {
            if !child_pgno_candidates.contains(&entry.pgno) {
                child_pgno_candidates.push(entry.pgno);
            }
        }

        let mut last_not_found = None;
        for child_pgno in child_pgno_candidates {
            match self.update_index_entry_recursive(file, ext_no, child_pgno, refno_loc) {
                Ok(()) => return Ok(()),
                Err(WriteError::IndexNotFound(_)) => {
                    last_not_found = Some(WriteError::IndexNotFound(
                        ((refno_loc.refno_0 as u64) << 32) | (refno_loc.refno_1 as u64),
                    ));
                }
                Err(err) => return Err(err),
            }
        }

        Err(last_not_found.unwrap_or_else(|| {
            WriteError::IndexNotFound(
                ((refno_loc.refno_0 as u64) << 32) | (refno_loc.refno_1 as u64),
            )
        }))
    }

    /// 创建新的元素写入器
    pub fn new(page_size: usize) -> Self {
        let page_manager = PageManager::new(256, page_size);
        Self {
            page_manager,
            page_size,
            current_index_root_pgno: None,
            current_claim_pgno: None,
            stats: WriteStats::default(),
        }
    }

    /// 使用 512 字节页面大小创建
    pub fn new_512() -> Self {
        Self::new(PAGE_SIZE_512)
    }

    /// 使用 2K 字节页面大小创建
    pub fn new_2k() -> Self {
        Self::new(PAGE_SIZE_2K)
    }

    /// 获取写入统计
    pub fn stats(&self) -> &WriteStats {
        &self.stats
    }

    /// 获取页面管理器的可变引用
    pub fn page_manager_mut(&mut self) -> &mut PageManager {
        &mut self.page_manager
    }

    /// 获取页面管理器的引用
    pub fn page_manager(&self) -> &PageManager {
        &self.page_manager
    }

    pub fn current_index_root_pgno(&self) -> Option<u32> {
        self.current_index_root_pgno
    }

    pub fn current_claim_pgno(&self) -> Option<u32> {
        self.current_claim_pgno
    }

    pub fn begin_session_tracking(&mut self) {
        self.current_claim_pgno = None;
    }

    /// 写入单个页面
    ///
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `ext_no` - 扩展号
    /// * `page_no` - 页面号
    /// * `data` - 页面数据
    pub fn write_page(
        &mut self,
        file: &mut File,
        ext_no: u32,
        page_no: u32,
        data: &[u8],
    ) -> Result<(), WriteError> {
        if data.len() != self.page_size {
            return Err(WriteError::PageSizeMismatch {
                expected: self.page_size,
                actual: data.len(),
            });
        }

        self.page_manager.write_page(file, ext_no, page_no, data)?;
        self.stats.pages_written += 1;
        self.stats.bytes_written += data.len() as u64;

        Ok(())
    }

    /// 刷新所有脏页到磁盘
    pub fn flush(&mut self, file: &mut File) -> Result<usize, WriteError> {
        let written = self.page_manager.flush_dirty_pages(file)?;
        self.stats.pages_written += written as u64;
        self.stats.bytes_written += (written * self.page_size) as u64;
        Ok(written)
    }

    /// 分配新页面
    ///
    /// # 参数
    /// * `file` - 数据库文件句柄
    ///
    /// # 返回值
    /// 新分配的页面号
    pub fn allocate_page(&mut self, file: &mut File) -> Result<u32, WriteError> {
        let new_page_no = self.page_manager.allocate_page(file, PRIMARY_EXT_NO)?;
        self.current_claim_pgno.get_or_insert(new_page_no);
        Ok(new_page_no)
    }

    /// 更新参考号索引位置
    ///
    /// 更新 B+ 树索引中的参考号位置信息
    ///
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `ext_no` - 扩展号
    /// * `index_page_no` - 索引页面号
    /// * `refno_loc` - 新的参考号位置信息
    pub fn update_index_entry(
        &mut self,
        file: &mut File,
        ext_no: u32,
        index_page_no: u32,
        refno_loc: &RefnoDataLoc,
    ) -> Result<(), WriteError> {
        self.update_index_entry_recursive(file, ext_no, index_page_no, refno_loc)?;
        self.current_index_root_pgno = Some(index_page_no);
        self.stats.index_updates += 1;
        Ok(())
    }

    /// 插入新的索引项到叶子页面
    ///
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `ext_no` - 扩展号
    /// * `index_page_no` - 索引叶子页号
    /// * `refno_loc` - 参考号位置信息
    ///
    /// # 返回值
    /// * `Ok(())` - 插入成功
    /// * `Err(WriteError)` - 页面已满或其他错误
    pub fn insert_index_entry(
        &mut self,
        file: &mut File,
        ext_no: u32,
        index_page_no: u32,
        refno_loc: &RefnoDataLoc,
    ) -> Result<(), WriteError> {
        // 读取索引页面
        let page_data = self
            .page_manager
            .get_page(file, ext_no, index_page_no)
            .map_err(WriteError::Io)?
            .to_vec();

        let parsed = self.parse_index_page(&page_data)?;
        self.insert_into_index_page(file, ext_no, index_page_no, parsed, refno_loc, true)?;
        self.current_index_root_pgno = Some(index_page_no);
        self.stats.index_updates += 1;
        Ok(())
    }

    /// 创建 RefnoDataLoc 结构
    ///
    /// # 参数
    /// * `refno` - 完整的 64 位参考号
    /// * `page_no` - 数据页号
    /// * `offset` - 页内偏移（以 2 字节为单位，最大 20 位）
    /// * `flag` - 标志位（最大 12 位，通常为 0）
    pub fn create_refno_loc(refno: u64, page_no: u32, offset: u32, flag: u16) -> RefnoDataLoc {
        RefnoDataLoc {
            refno_0: (refno >> 32) as u32,
            refno_1: refno as u32,
            pgno: page_no,
            offset,
            flag,
        }
    }
}

impl Default for ElementWriter {
    fn default() -> Self {
        Self::new_512()
    }
}

// ==================================================================================
// 数据页写入层 (Data Page Writer)
// ==================================================================================

/// 数据页类型常量
pub const DATA_PAGE_TYPE: u32 = 5;
/// 主数据页子类型
pub const MAIN_DATA_SUBTYPE: u32 = 0x00743F49;
/// 特殊/续页类型
pub const SPECIAL_PAGE_TYPE: u32 = 7;
/// 数据页头长度
pub const DATA_PAGE_HEADER_SIZE: usize = 24;
/// 特殊页 segment 头长度
pub const SPECIAL_SEGMENT_HEADER_SIZE: usize = 24;

/// 数据页位置信息
#[derive(Debug, Clone, Default)]
pub struct DataPageLocation {
    /// 扩展号
    pub ext_no: u32,
    /// 页号
    pub page_no: u32,
    /// 页内偏移（以字节为单位）
    pub offset: usize,
}

impl DataPageLocation {
    /// 创建新的位置信息
    pub fn new(ext_no: u32, page_no: u32, offset: usize) -> Self {
        Self {
            ext_no,
            page_no,
            offset,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DataPageWriteResult {
    pub start: DataPageLocation,
    pub total_len: usize,
    pub end_page_no: u32,
    pub pages_used: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataWriterPageKind {
    Data,
    Special,
}

/// 数据页写入器
///
/// 管理数据页的分配和元素写入
#[derive(Debug)]
pub struct DataPageWriter {
    /// 当前数据页缓冲区
    current_page: Vec<u8>,
    current_page_kind: DataWriterPageKind,
    /// 当前页内偏移
    current_offset: usize,
    /// 当前页号
    current_page_no: u32,
    /// 扩展号
    ext_no: u32,
    /// 页面大小
    page_size: usize,
    /// 已写入的页面列表
    written_pages: Vec<(u32, Vec<u8>)>,
    /// 写入统计
    elements_written: u64,
    bytes_written: u64,
}

impl DataPageWriter {
    fn current_payload_start(&self) -> usize {
        match self.current_page_kind {
            DataWriterPageKind::Data => DATA_PAGE_HEADER_SIZE,
            DataWriterPageKind::Special => SPECIAL_SEGMENT_HEADER_SIZE,
        }
    }

    fn init_data_page(page_size: usize, page_no: u32, ext_no: u32) -> Vec<u8> {
        let mut page = vec![0u8; page_size];
        page[0..4].copy_from_slice(&DATA_PAGE_TYPE.to_be_bytes());
        page[4..8].copy_from_slice(&MAIN_DATA_SUBTYPE.to_be_bytes());
        page[8..12].copy_from_slice(&ext_no.to_be_bytes());
        page[12..16].copy_from_slice(&page_no.to_be_bytes());
        page[16..20].copy_from_slice(&0u32.to_be_bytes());
        let bucket_id = DataPageSubtype::MainVariant.get_bucket_id();
        page[20..24].copy_from_slice(&bucket_id.to_be_bytes());
        page
    }

    fn init_special_page(page_size: usize) -> Vec<u8> {
        vec![0u8; page_size]
    }

    /// 创建新的数据页写入器
    ///
    /// # 参数
    /// * `page_size` - 页面大小
    /// * `start_page_no` - 起始页号
    /// * `ext_no` - 扩展号
    pub fn new(page_size: usize, start_page_no: u32, ext_no: u32) -> Self {
        let current_page = Self::init_data_page(page_size, start_page_no, ext_no);

        Self {
            current_page,
            current_page_kind: DataWriterPageKind::Data,
            current_offset: DATA_PAGE_HEADER_SIZE,
            current_page_no: start_page_no,
            ext_no,
            page_size,
            written_pages: Vec::new(),
            elements_written: 0,
            bytes_written: 0,
        }
    }

    /// 使用 512 字节页面创建
    pub fn new_512(start_page_no: u32) -> Self {
        Self::new(PAGE_SIZE_512, start_page_no, 1)
    }

    /// 使用 2K 字节页面创建
    pub fn new_2k(start_page_no: u32) -> Self {
        Self::new(PAGE_SIZE_2K, start_page_no, 1)
    }

    /// 获取当前页剩余空间
    pub fn remaining_space(&self) -> usize {
        self.page_size.saturating_sub(self.current_offset)
    }

    /// 获取当前位置信息
    pub fn current_location(&self) -> DataPageLocation {
        DataPageLocation::new(self.ext_no, self.current_page_no, self.current_offset)
    }

    /// 检查是否有足够空间写入指定大小的数据
    pub fn has_space_for(&self, size: usize) -> bool {
        self.remaining_space() >= size
    }

    fn current_page_has_payload(&self) -> bool {
        self.current_offset > self.current_payload_start()
    }

    fn push_current_page(&mut self) {
        if self.current_page_has_payload() {
            self.written_pages
                .push((self.current_page_no, self.current_page.clone()));
        }
    }

    fn start_new_data_page(&mut self) {
        self.push_current_page();
        self.current_page_no += 1;
        self.current_page_kind = DataWriterPageKind::Data;
        self.current_page = Self::init_data_page(self.page_size, self.current_page_no, self.ext_no);
        self.current_offset = DATA_PAGE_HEADER_SIZE;
    }

    fn start_special_page(&mut self) {
        self.push_current_page();
        self.current_page_no += 1;
        self.current_page_kind = DataWriterPageKind::Special;
        self.current_page = Self::init_special_page(self.page_size);
        self.current_offset = 0;
    }

    fn ensure_data_page(&mut self) {
        if self.current_page_kind != DataWriterPageKind::Data {
            self.start_new_data_page();
        }
    }

    fn append_bytes(&mut self, bytes: &[u8]) {
        self.current_page[self.current_offset..self.current_offset + bytes.len()]
            .copy_from_slice(bytes);
        self.current_offset += bytes.len();
    }

    fn append_segment_header(&mut self, flag: u8, len_words: u16, self_ref: &[u8]) {
        self.append_bytes(&SPECIAL_PAGE_TYPE.to_be_bytes());
        self.current_page[self.current_offset] = 0x00;
        self.current_page[self.current_offset + 1] = flag;
        self.current_page[self.current_offset + 2..self.current_offset + 4]
            .copy_from_slice(&len_words.to_be_bytes());
        self.current_offset += 4;
        self.append_bytes(self_ref);
        self.append_bytes(&0u32.to_be_bytes());
        self.append_bytes(&0u32.to_be_bytes());
    }

    fn skip_padding_len(input: &[u8]) -> usize {
        let mut pos = 0;
        while pos + 4 <= input.len() {
            let next = &input[pos..pos + 4];
            if next == [0, 0, 0, 0] || next == [0, 0, 0, 7] {
                pos += 4;
            } else {
                break;
            }
        }
        pos
    }

    fn extend_impl_len(declared: usize, input: &[u8]) -> usize {
        let mut actual = declared;
        while actual + 4 <= input.len() {
            let next = &input[actual..actual + 4];
            if next == [0, 0, 0, 0] || next == [0, 0, 0, 7] {
                actual += 4;
            } else {
                break;
            }
        }
        actual
    }

    fn parse_block_len(data: &[u8], pos: usize) -> Result<Option<(u8, usize)>, WriteError> {
        if pos + 4 > data.len() {
            return Ok(None);
        }

        let flag = u16::from_be_bytes([data[pos], data[pos + 1]]);
        if flag != 0x0001 && flag != 0x0002 {
            return Ok(None);
        }

        let len_words = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        if len_words == 0 {
            return Err(WriteError::SerializationError(format!(
                "block len_words=0 at pos={}",
                pos
            )));
        }

        let block_len = len_words * 4;
        if pos + block_len > data.len() {
            return Err(WriteError::SerializationError(format!(
                "block overruns record boundary at pos={}",
                pos
            )));
        }

        Ok(Some((flag as u8, block_len)))
    }

    fn write_block_with_segments(
        &mut self,
        flag: u8,
        block: &[u8],
        trailing_reserve: usize,
    ) -> Result<(), WriteError> {
        let self_ref = &block[4..12];
        self.ensure_data_page();

        let mut available = self.remaining_space();
        if available <= trailing_reserve + 12 {
            self.start_new_data_page();
            available = self.remaining_space();
        }

        let must_reserve_for_first_segment =
            if block.len() > available.saturating_sub(trailing_reserve) {
                SPECIAL_SEGMENT_HEADER_SIZE + 4
            } else {
                0
            };
        let effective_available = available
            .saturating_sub(trailing_reserve)
            .saturating_sub(must_reserve_for_first_segment);
        if effective_available < 12 {
            return Err(WriteError::SerializationError(
                "当前数据页空间不足，无法写入 block 首段".into(),
            ));
        }

        let first_chunk_len = block.len().min(effective_available & !0x3);
        if first_chunk_len < 12 {
            return Err(WriteError::SerializationError(
                "block 首段长度不足，无法安全切分".into(),
            ));
        }

        let first_chunk = &block[..first_chunk_len];
        let first_words = (first_chunk_len / 4) as u16;
        let mut first_header = first_chunk[..4].to_vec();
        first_header[2..4].copy_from_slice(&first_words.to_be_bytes());
        self.append_bytes(&first_header);
        self.append_bytes(&first_chunk[4..]);

        let mut remaining_payload = &block[first_chunk_len..];
        while !remaining_payload.is_empty() {
            if self.remaining_space() < SPECIAL_SEGMENT_HEADER_SIZE + 4 {
                self.start_special_page();
            }

            let max_payload = self
                .remaining_space()
                .saturating_sub(SPECIAL_SEGMENT_HEADER_SIZE)
                .saturating_sub(
                    if remaining_payload.len()
                        <= self
                            .remaining_space()
                            .saturating_sub(SPECIAL_SEGMENT_HEADER_SIZE)
                    {
                        trailing_reserve
                    } else {
                        0
                    },
                );
            let payload_len = remaining_payload.len().min(max_payload & !0x3);
            if payload_len == 0 {
                self.start_special_page();
                continue;
            }

            let seg_words = ((payload_len + 20) / 4) as u16;
            self.append_segment_header(flag, seg_words, self_ref);
            self.append_bytes(&remaining_payload[..payload_len]);
            remaining_payload = &remaining_payload[payload_len..];
        }

        Ok(())
    }

    /// 写入原始数据到当前页
    ///
    /// 如果当前页空间不足，会自动刷新并创建新页
    ///
    /// # 返回值
    /// 返回写入结果
    pub fn write_data(&mut self, data: &[u8]) -> Result<DataPageWriteResult, WriteError> {
        self.ensure_data_page();

        let prefix = Self::skip_padding_len(data);
        if prefix + 4 > data.len() {
            return Err(WriteError::SerializationError(
                "元素记录缺少 impl_len".into(),
            ));
        }

        let impl_len_words = i32::from_be_bytes(data[prefix..prefix + 4].try_into().unwrap());
        if impl_len_words <= 0 {
            return Err(WriteError::SerializationError(format!(
                "impl_len 非法: {}",
                impl_len_words
            )));
        }

        let declared_impl_len = impl_len_words as usize * 4;
        if prefix + declared_impl_len > data.len() {
            return Err(WriteError::SerializationError(
                "隐含区长度超出元素记录边界".into(),
            ));
        }
        let implicit_len = prefix + Self::extend_impl_len(declared_impl_len, &data[prefix..]);

        let minimum_tail = 8usize;
        if self.remaining_space() < implicit_len + minimum_tail {
            self.start_new_data_page();
        }
        if self.remaining_space() < implicit_len {
            return Err(WriteError::SerializationError(
                "单个元素隐含区超过数据页容量，当前实现不支持".into(),
            ));
        }

        let start = self.current_location();
        self.append_bytes(&data[..implicit_len]);

        let mut pos = implicit_len;
        while pos < data.len() {
            if pos + 8 <= data.len()
                && &data[pos..pos + 4] == [0, 0, 0, 0]
                && &data[pos + 4..pos + 8] == [0, 0, 0, 7]
            {
                if self.remaining_space() < 8 {
                    return Err(WriteError::SerializationError(
                        "元素结束标记无法落在当前页，需调整分段策略".into(),
                    ));
                }
                self.append_bytes(&data[pos..pos + 8]);
                pos += 8;
                break;
            }

            if let Some((flag, block_len)) = Self::parse_block_len(data, pos)? {
                let trailing_reserve = if pos + block_len + 8 <= data.len()
                    && &data[pos + block_len..pos + block_len + 4] == [0, 0, 0, 0]
                    && &data[pos + block_len + 4..pos + block_len + 8] == [0, 0, 0, 7]
                {
                    8
                } else {
                    0
                };
                self.write_block_with_segments(
                    flag,
                    &data[pos..pos + block_len],
                    trailing_reserve,
                )?;
                pos += block_len;
                continue;
            }

            let remaining = data.len() - pos;
            if self.remaining_space() < remaining {
                return Err(WriteError::SerializationError(
                    "元素尾部数据无法安全跨页写入".into(),
                ));
            }
            self.append_bytes(&data[pos..]);
            pos = data.len();
        }

        self.bytes_written += data.len() as u64;

        let pages_used = self
            .written_pages
            .len()
            .saturating_add(usize::from(self.current_page_has_payload()));
        let result = DataPageWriteResult {
            start,
            total_len: data.len(),
            end_page_no: self.current_page_no,
            pages_used,
        };

        if result.end_page_no != result.start.page_no
            && self.current_page_kind == DataWriterPageKind::Special
        {
            self.start_new_data_page();
        }

        Ok(result)
    }

    /// 写入元素数据并返回位置信息
    ///
    /// # 参数
    /// * `element_data` - 已序列化的元素数据
    pub fn write_element(
        &mut self,
        element_data: &[u8],
    ) -> Result<DataPageWriteResult, WriteError> {
        let location = self.write_data(element_data)?;
        self.elements_written += 1;
        Ok(location)
    }

    /// 完成写入，返回所有已写入的页面
    ///
    /// 包括当前未满的页面
    pub fn finish(mut self) -> Vec<(u32, Vec<u8>)> {
        self.push_current_page();
        self.written_pages
    }

    /// 获取统计信息
    pub fn stats(&self) -> (u64, u64) {
        (self.elements_written, self.bytes_written)
    }

    /// 获取当前页号
    pub fn current_page_no(&self) -> u32 {
        self.current_page_no
    }
}

// ==================================================================================
// 会话写入层 (Session Writer)
// ==================================================================================

/// 会话页面构建器
///
/// 用于创建新的会话页面数据，对应 db5_save_work 中的会话创建逻辑
#[derive(Debug, Clone)]
pub struct SessionBuilder {
    /// 会话号
    pub sesno: u32,
    /// 上一个会话页号
    pub last_ses_pageno: u32,
    /// 上一个会话扩展号
    pub last_ses_extno: u32,
    /// 会话结束页号
    pub end_pgno: u32,
    /// 会话结束扩展号
    pub end_extno: u32,
    /// 索引根页号
    pub index_root_pageno: u32,
    /// 索引根扩展号
    pub index_root_extno: u32,
    /// 声明页号
    pub claim_pageno: u32,
    /// 声明扩展号
    pub claim_extno: u32,
    /// 未知数据1
    pub unknown_1: i32,
    /// 未知数据2
    pub unknown_2: i32,
    /// 计算机名
    pub computer_name: String,
    /// 注释
    pub comments: String,
}

impl SessionBuilder {
    /// 创建新的会话构建器
    pub fn new(sesno: u32, last_ses_pageno: u32) -> Self {
        Self {
            sesno,
            last_ses_pageno,
            last_ses_extno: 1,
            end_pgno: 0,
            end_extno: 1,
            index_root_pageno: 0,
            index_root_extno: 1,
            claim_pageno: 0,
            claim_extno: 1,
            unknown_1: 0,
            unknown_2: 0,
            computer_name: String::new(),
            comments: String::new(),
        }
    }

    /// 设置结束页号
    pub fn end_pgno(mut self, pgno: u32) -> Self {
        self.end_pgno = pgno;
        self
    }

    /// 设置索引根页号
    pub fn index_root(mut self, pgno: u32) -> Self {
        self.index_root_pageno = pgno;
        self
    }

    /// 设置声明页号
    pub fn claim_root(mut self, pgno: u32) -> Self {
        self.claim_pageno = pgno;
        self
    }

    /// 设置计算机名
    pub fn computer_name(mut self, name: impl Into<String>) -> Self {
        self.computer_name = name.into();
        self
    }

    /// 设置注释
    pub fn comments(mut self, comments: impl Into<String>) -> Self {
        self.comments = comments.into();
        self
    }

    /// 构建会话页面二进制数据
    ///
    /// # 参数
    /// * `page_size` - 页面大小
    ///
    /// # 返回值
    /// * 会话页面的二进制数据
    pub fn build(&self, page_size: usize) -> Vec<u8> {
        let mut data = vec![0u8; page_size];

        // 写入页面类型 (0x00000003 = 会话页)
        data[0..4].copy_from_slice(&3i32.to_be_bytes());

        // 写入上一个会话页号
        data[4..8].copy_from_slice(&(self.last_ses_pageno as i32).to_be_bytes());

        // 写入上一个会话扩展号 (通常为 1)
        data[8..12].copy_from_slice(&(self.last_ses_extno as i32).to_be_bytes());

        // 写入会话号
        data[12..16].copy_from_slice(&(self.sesno as i32).to_be_bytes());

        // 写入 unknown_0 (0xFFFFFFFF)
        data[16..20].copy_from_slice(&(-1i32).to_be_bytes());

        // 写入结束页号
        data[20..24].copy_from_slice(&self.end_pgno.to_be_bytes());

        // 写入结束扩展号 (通常为 1)
        data[24..28].copy_from_slice(&self.end_extno.to_be_bytes());

        // 写入索引根页号
        data[28..32].copy_from_slice(&self.index_root_pageno.to_be_bytes());

        // 写入索引根扩展号 (通常为 1)
        data[32..36].copy_from_slice(&self.index_root_extno.to_be_bytes());

        // 写入声明页号与扩展号
        data[0x24..0x28].copy_from_slice(&self.claim_pageno.to_be_bytes());
        data[0x28..0x2C].copy_from_slice(&self.claim_extno.to_be_bytes());

        // 写入未知字段
        data[0x2C..0x30].copy_from_slice(&self.unknown_1.to_be_bytes());
        data[0x30..0x34].copy_from_slice(&self.unknown_2.to_be_bytes());

        // 写入时间戳 (年、月、小时、秒)
        let now = chrono::Local::now();
        let year = now.year() as u32;
        let month = now.month() as u32;
        let hours = now.day() * 24 + now.hour();
        let seconds = now.minute() * 60 + now.second();

        data[0x34..0x38].copy_from_slice(&year.to_be_bytes());
        data[0x38..0x3C].copy_from_slice(&month.to_be_bytes());
        data[0x3C..0x40].copy_from_slice(&hours.to_be_bytes());
        data[0x40..0x44].copy_from_slice(&seconds.to_be_bytes());

        // 写入计算机名长度和内容
        let name_bytes = self.computer_name.as_bytes();
        let name_words = ((name_bytes.len() + 3) / 4).min(9); // 以 4 字节为单位，最多 9 words
        data[0x78..0x7C].copy_from_slice(&(name_words as u32).to_be_bytes());

        // 写入计算机名
        let name_start = 0x7C;
        let name_len = std::cmp::min(name_bytes.len(), name_words * 4);
        if name_len > 0 {
            data[name_start..name_start + name_len].copy_from_slice(&name_bytes[..name_len]);
        }

        // 写入注释长度和内容
        let comments_start = 0x7C + 36; // 名称固定 36 字节
        let comments_bytes = self.comments.as_bytes();
        let comments_words = ((comments_bytes.len() + 3) / 4).min(1024);
        if comments_start + 4 <= page_size {
            data[comments_start..comments_start + 4]
                .copy_from_slice(&(comments_words as u32).to_be_bytes());
            let max_payload = page_size - comments_start - 4;
            let comments_len = std::cmp::min(comments_words * 4, max_payload);
            let copy_len = std::cmp::min(comments_bytes.len(), comments_len);
            if copy_len > 0 {
                data[comments_start + 4..comments_start + 4 + copy_len]
                    .copy_from_slice(&comments_bytes[..copy_len]);
            }
        }

        data
    }
}

/// 数据库头更新器
///
/// 用于更新 PDMS 数据库头部信息
pub struct HeaderUpdater;

impl HeaderUpdater {
    /// 更新数据库头的最新会话页号
    ///
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `new_ses_pgno` - 新的会话页号
    pub fn update_latest_ses_pgno(file: &mut File, new_ses_pgno: u32) -> std::io::Result<()> {
        // 会话页号位于 offset 0x28 (40 字节)
        file.seek(SeekFrom::Start(0x28))?;
        file.write_all(&new_ses_pgno.to_be_bytes())?;
        file.flush()?;
        Ok(())
    }

    /// 更新数据库头的存储页数
    ///
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `page_count` - 新的页数
    pub fn update_stored_page_count(file: &mut File, page_count: u32) -> std::io::Result<()> {
        // 页数位于 offset 0x38 (56 字节)
        file.seek(SeekFrom::Start(0x38))?;
        file.write_all(&page_count.to_be_bytes())?;
        file.flush()?;
        Ok(())
    }

    /// 批量更新数据库头
    ///
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `ses_pgno` - 新的会话页号
    /// * `page_count` - 新的页数
    pub fn update_header(file: &mut File, ses_pgno: u32, page_count: u32) -> std::io::Result<()> {
        Self::update_latest_ses_pgno(file, ses_pgno)?;
        Self::update_stored_page_count(file, page_count)?;
        Ok(())
    }
}

use chrono::{Datelike, Timelike};

/// 完整的数据库写入器
///
/// 组合 ElementWriter 和会话管理功能，提供高层 API
pub struct DatabaseWriter {
    /// 元素写入器
    pub element_writer: ElementWriter,
    /// 当前会话号
    pub current_sesno: Option<u32>,
    /// 页面大小
    pub page_size: usize,
}

impl DatabaseWriter {
    /// 创建新的数据库写入器
    pub fn new(page_size: usize) -> Self {
        Self {
            element_writer: ElementWriter::new(page_size),
            current_sesno: None,
            page_size,
        }
    }

    /// 使用 512 字节页面创建
    pub fn new_512() -> Self {
        Self::new(PAGE_SIZE_512)
    }

    /// 使用 2K 字节页面创建
    pub fn new_2k() -> Self {
        Self::new(PAGE_SIZE_2K)
    }

    /// 开始新会话
    ///
    /// # 参数
    /// * `sesno` - 会话号
    pub fn begin_session(&mut self, sesno: u32) {
        self.current_sesno = Some(sesno);
        self.element_writer.begin_session_tracking();
    }

    /// 提交当前会话
    ///
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `last_ses_pgno` - 上一个会话页号
    /// * `computer_name` - 计算机名（可选）
    /// * `comments` - 注释（可选）
    ///
    /// # 返回值
    /// * 新会话页的页号
    pub fn commit_session(
        &mut self,
        file: &mut File,
        last_ses_pgno: u32,
        computer_name: Option<&str>,
        comments: Option<&str>,
    ) -> Result<u32, WriteError> {
        let sesno = self
            .current_sesno
            .ok_or_else(|| WriteError::SerializationError("未开始会话".into()))?;
        let index_root_pgno = self
            .element_writer
            .current_index_root_pgno()
            .ok_or_else(|| {
                WriteError::SerializationError("commit_session 需要已建立的当前 index_root".into())
            })?;
        let claim_pgno = self.element_writer.current_claim_pgno().unwrap_or(0);

        // 1. 刷新所有脏页
        self.element_writer.flush(file)?;

        let file_size_before_session = file.seek(SeekFrom::End(0))?;
        let last_committed_pgno = if file_size_before_session == 0 {
            0
        } else {
            ((file_size_before_session / self.page_size as u64).saturating_sub(1)) as u32
        };

        // 2. 分配会话页面
        let new_ses_pgno = self.element_writer.allocate_page(file)?;

        // 3. 构建会话页面数据
        let session = SessionBuilder::new(sesno, last_ses_pgno)
            .end_pgno(last_committed_pgno)
            .index_root(index_root_pgno)
            .claim_root(claim_pgno)
            .computer_name(computer_name.unwrap_or("PDMS-IO"))
            .comments(comments.unwrap_or(""))
            .build(self.page_size);

        // 4. 写入会话页面
        self.element_writer
            .write_page(file, PRIMARY_EXT_NO, new_ses_pgno, &session)?;

        // 5. 更新数据库头
        let file_size = file.seek(SeekFrom::End(0))?;
        let page_count = (file_size / self.page_size as u64) as u32;
        HeaderUpdater::update_header(file, new_ses_pgno, page_count)?;

        // 6. 清除会话状态
        self.current_sesno = None;
        self.element_writer.begin_session_tracking();

        Ok(new_ses_pgno)
    }

    /// 是否有活跃会话
    pub fn has_active_session(&self) -> bool {
        self.current_sesno.is_some()
    }

    /// 获取写入统计
    pub fn stats(&self) -> &WriteStats {
        self.element_writer.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element_record_reader::ElementRecordReader;
    use crate::element_serializer::EleSerializer;
    use crate::page_manager::PageManager;
    use deku::DekuContainerRead;
    use parse_pdms_db::parser::parse_element_children;
    use std::fs::OpenOptions;
    use std::io::{Read, Seek, SeekFrom};

    fn make_implicit_only_record(refno: u64, total_bytes: usize, fill: u8) -> Vec<u8> {
        let total_words = total_bytes / 4;
        let mut record = EleSerializer::serialize_element_header(total_words as u32, refno, 10, 0);
        if total_bytes > record.len() {
            record.extend(std::iter::repeat_n(fill, total_bytes - record.len()));
        }
        record
    }

    #[test]
    fn test_element_writer_creation() {
        let writer = ElementWriter::new_512();
        assert_eq!(writer.page_size, 512);
        assert_eq!(writer.stats.pages_written, 0);
    }

    #[test]
    fn test_write_page() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_writer_test.bin");
        let _ = std::fs::remove_file(&temp_file);

        // 初始化文件
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");

            let empty_data = vec![0u8; 512 * 4];
            file.write_all(&empty_data).expect("无法初始化文件");
        }

        let mut writer = ElementWriter::new_512();
        let test_data: Vec<u8> = (0..512).map(|i| (i % 256) as u8).collect();

        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");

            writer
                .write_page(&mut file, PRIMARY_EXT_NO, 1, &test_data)
                .expect("写入失败");
        }

        // 验证
        {
            let mut file = std::fs::File::open(&temp_file).expect("无法打开文件");
            file.seek(SeekFrom::Start(512)).expect("无法定位");

            let mut read_data = vec![0u8; 512];
            file.read_exact(&mut read_data).expect("无法读取");

            assert_eq!(read_data, test_data);
        }

        assert_eq!(writer.stats.pages_written, 1);
        assert_eq!(writer.stats.bytes_written, 512);

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_allocate_page() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_alloc_test.bin");
        let _ = std::fs::remove_file(&temp_file);

        // 创建初始文件 (2 页)
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");

            let data = vec![0u8; 512 * 2];
            file.write_all(&data).expect("无法初始化文件");
        }

        let mut writer = ElementWriter::new_512();

        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");

            let new_page = writer.allocate_page(&mut file).expect("分配失败");
            assert_eq!(new_page, 2); // 第三个页面 (0, 1, 2)
        }

        assert!(writer.page_manager().is_page_cached(PRIMARY_EXT_NO, 2));
        assert_eq!(writer.page_manager().dirty_count(), 1);

        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");

            writer.flush(&mut file).expect("flush 失败");
        }

        let file_size = std::fs::metadata(&temp_file).expect("无法获取元数据").len();
        assert_eq!(file_size, 512 * 3);

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_data_page_writer_creation() {
        let writer = DataPageWriter::new_512(10);
        assert_eq!(writer.current_page_no(), 10);
        assert_eq!(writer.remaining_space(), 512 - DATA_PAGE_HEADER_SIZE);
    }

    #[test]
    fn test_data_page_writer_write_element() {
        let mut writer = DataPageWriter::new_512(10);

        // 写入一个 100 字节的元素
        let refno = 0x1234_5678u64;
        let header = EleSerializer::serialize_element_header(6, refno, 10, 0);
        let element_data = [
            [0x00, 0x00, 0x00, 0x07].as_slice(),
            header.as_slice(),
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07].as_slice(),
        ]
        .concat();
        let result = writer.write_element(&element_data).expect("写入元素失败");

        assert_eq!(result.start.page_no, 10);
        assert_eq!(result.start.offset, DATA_PAGE_HEADER_SIZE);
        assert_eq!(result.total_len, element_data.len());
        assert_eq!(result.pages_used, 1);

        let (elements, bytes) = writer.stats();
        assert_eq!(elements, 1);
        assert_eq!(bytes as usize, element_data.len());
    }

    #[test]
    fn test_data_page_writer_auto_flush() {
        let mut writer = DataPageWriter::new_512(10);

        // 写入多个元素直到需要刷新页面
        // 新页头 24 字节，页面可用空间 488 字节，可容纳 4 个 100 字节元素。
        for i in 0..5 {
            let element_data = make_implicit_only_record(0x1000 + i as u64, 100, i as u8);
            let result = writer.write_element(&element_data).expect("写入元素失败");

            if i < 4 {
                assert_eq!(result.start.page_no, 10);
            } else {
                assert_eq!(result.start.page_no, 11);
            }
        }
    }

    #[test]
    fn test_data_page_writer_finish() {
        let mut writer = DataPageWriter::new_512(10);

        // 写入一个元素
        let element_data = make_implicit_only_record(0x2000, 52, 0xAB);
        writer.write_element(&element_data).expect("写入元素失败");

        // 完成
        let pages = writer.finish();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].0, 10); // 页号
        assert_eq!(pages[0].1.len(), 512); // 完整页面大小
        assert_eq!(
            u32::from_be_bytes(pages[0].1[0..4].try_into().unwrap()),
            DATA_PAGE_TYPE
        );
        assert_eq!(
            u32::from_be_bytes(pages[0].1[4..8].try_into().unwrap()),
            MAIN_DATA_SUBTYPE
        );
        assert_eq!(u32::from_be_bytes(pages[0].1[8..12].try_into().unwrap()), 1);
        assert_eq!(
            u32::from_be_bytes(pages[0].1[12..16].try_into().unwrap()),
            10
        );
    }

    #[test]
    fn test_data_page_writer_cross_page_readback() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_data_writer_cross_page.bin");
        let _ = std::fs::remove_file(&temp_file);

        let refno = 0xABCDu64;
        let children: Vec<u64> = (1..=260).map(|i| 0x1_0000u64 + i as u64).collect();
        let members = EleSerializer::serialize_members(refno, &children);
        let header = EleSerializer::serialize_element_header(6, refno, 10, 0);

        let record = [
            header.as_slice(),
            members.as_slice(),
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07].as_slice(),
        ]
        .concat();

        let mut writer = DataPageWriter::new_512(2);
        let result = writer.write_element(&record).expect("跨页写入失败");
        assert!(result.end_page_no > result.start.page_no);

        let pages = writer.finish();
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&temp_file)
            .expect("无法创建临时文件");

        let backing = vec![0u8; 512 * 64];
        file.write_all(&backing).expect("初始化失败");
        for (pgno, page) in pages {
            file.seek(SeekFrom::Start(pgno as u64 * 512))
                .expect("seek 失败");
            file.write_all(&page).expect("写入页失败");
        }
        file.flush().expect("flush 失败");
        file.seek(SeekFrom::Start(0)).expect("seek 失败");

        let mut pm = PageManager::new(64, 512);
        let read_back = ElementRecordReader::read(
            &mut file,
            &mut pm,
            1,
            512,
            (result.start.page_no as u64 * 512) + result.start.offset as u64,
        )
        .expect("读回失败");

        let (_, (original_refno, original_children)) =
            parse_element_children(&record).expect("原始 record 解析失败");
        let (_, (read_refno, read_children)) =
            parse_element_children(&read_back).expect("读回 record 解析失败");

        assert_eq!(original_refno, read_refno);
        assert_eq!(original_children.as_slice(), read_children.as_slice());

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_commit_session_requires_tracked_index_root() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_commit_requires_root.bin");
        let _ = std::fs::remove_file(&temp_file);

        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");
            let data = vec![0u8; 512 * 2];
            file.write_all(&data).expect("无法初始化文件");
        }

        let mut writer = DatabaseWriter::new_512();
        writer.begin_session(1);

        let err = {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");
            writer
                .commit_session(&mut file, 0, Some("TEST-PC"), Some("invalid"))
                .expect_err("未建立索引根时应失败")
        };

        match err {
            WriteError::SerializationError(msg) => {
                assert!(msg.contains("index_root"));
            }
            other => panic!("错误类型不符: {:?}", other),
        }

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_commit_session_uses_current_index_root_and_last_data_page() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_commit_session_auto_root.bin");
        let _ = std::fs::remove_file(&temp_file);

        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");
            let header = vec![0u8; 512];
            let mut index_page = vec![0u8; 512];
            index_page[0..4].copy_from_slice(&8u32.to_be_bytes());
            index_page[4..8].copy_from_slice(&INDEX_PAGE_NOUN.to_be_bytes());
            file.write_all(&header).expect("写入头失败");
            file.write_all(&index_page).expect("写入索引页失败");
        }

        let mut writer = DatabaseWriter::new_512();
        writer.begin_session(7);

        let new_ses_pgno = {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");

            let loc = ElementWriter::create_refno_loc(100u64, 2, 16, 1);
            writer
                .element_writer
                .insert_index_entry(&mut file, PRIMARY_EXT_NO, 1, &loc)
                .expect("插入索引失败");

            writer
                .commit_session(&mut file, 0, Some("AUTO-PC"), Some("auto root"))
                .expect("提交会话失败")
        };

        let mut file = OpenOptions::new()
            .read(true)
            .open(&temp_file)
            .expect("无法重新打开文件");
        let mut session_buf = vec![0u8; 512];
        file.seek(SeekFrom::Start(new_ses_pgno as u64 * 512))
            .expect("seek 失败");
        file.read_exact(&mut session_buf).expect("读取会话页失败");

        let session = SessionPageData::from_bytes((&session_buf, 0))
            .expect("解析会话页失败")
            .1;
        assert_eq!(session.index_root_pageno, 1);
        assert_eq!(session.end_pgno, 1);
        assert_eq!(session.claim_pageno, 0);
        assert_eq!(session.claim_extno, 1);

        let mut header_buf = vec![0u8; 64];
        file.seek(SeekFrom::Start(0)).expect("seek 头失败");
        file.read_exact(&mut header_buf).expect("读取头失败");
        assert_eq!(
            u32::from_be_bytes(header_buf[0x28..0x2C].try_into().unwrap()),
            new_ses_pgno
        );
        assert_eq!(
            u32::from_be_bytes(header_buf[0x38..0x3C].try_into().unwrap()),
            3
        );

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_commit_session_tracks_claim_page_range() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_commit_session_claim.bin");
        let _ = std::fs::remove_file(&temp_file);

        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");
            let header = vec![0u8; 512];
            let mut index_page = vec![0u8; 512];
            index_page[0..4].copy_from_slice(&8u32.to_be_bytes());
            index_page[4..8].copy_from_slice(&INDEX_PAGE_NOUN.to_be_bytes());
            file.write_all(&header).expect("写入头失败");
            file.write_all(&index_page).expect("写入索引页失败");
        }

        let mut writer = DatabaseWriter::new_512();
        writer.begin_session(8);

        let new_ses_pgno = {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");

            let loc = ElementWriter::create_refno_loc(101u64, 2, 16, 1);
            writer
                .element_writer
                .insert_index_entry(&mut file, PRIMARY_EXT_NO, 1, &loc)
                .expect("插入索引失败");

            let first_claim_pgno = writer
                .element_writer
                .allocate_page(&mut file)
                .expect("分配第一个 claim 页失败");
            let second_claim_pgno = writer
                .element_writer
                .allocate_page(&mut file)
                .expect("分配第二个 claim 页失败");
            assert_eq!(first_claim_pgno, 2);
            assert_eq!(second_claim_pgno, 3);

            writer
                .commit_session(&mut file, 1, Some("CLAIM-PC"), Some("claim root"))
                .expect("提交会话失败")
        };

        let mut file = OpenOptions::new()
            .read(true)
            .open(&temp_file)
            .expect("无法重新打开文件");
        let mut session_buf = vec![0u8; 512];
        file.seek(SeekFrom::Start(new_ses_pgno as u64 * 512))
            .expect("seek 会话页失败");
        file.read_exact(&mut session_buf).expect("读取会话页失败");

        let session = SessionPageData::from_bytes((&session_buf, 0))
            .expect("解析会话页失败")
            .1;
        assert_eq!(session.last_ses_pageno, 1);
        assert_eq!(session.index_root_pageno, 1);
        assert_eq!(session.claim_pageno, 2);
        assert_eq!(session.claim_extno, 1);
        assert_eq!(session.end_pgno, 3);
        assert_eq!(session.end_extno, 1);

        let mut header_buf = vec![0u8; 64];
        file.seek(SeekFrom::Start(0)).expect("seek 头失败");
        file.read_exact(&mut header_buf).expect("读取头失败");
        assert_eq!(
            u32::from_be_bytes(header_buf[0x28..0x2C].try_into().unwrap()),
            new_ses_pgno
        );
        assert_eq!(
            u32::from_be_bytes(header_buf[0x38..0x3C].try_into().unwrap()),
            5
        );

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_insert_index_entry_orders_leaf_entries() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_index_order.bin");
        let _ = std::fs::remove_file(&temp_file);

        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");
            let data = vec![0u8; 512 * 2];
            file.write_all(&data).expect("初始化失败");
        }

        let mut writer = ElementWriter::new_512();
        let empty_root = ParsedIndexPage {
            page_type: 8,
            level: 0,
            unknowns: [0; 3],
            pfno: 0,
            entries: Vec::new(),
        };

        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");
            let root_bytes = writer
                .serialize_index_page(&empty_root)
                .expect("序列化失败");
            writer
                .write_page(&mut file, PRIMARY_EXT_NO, 1, &root_bytes)
                .expect("写入根页失败");

            for refno in [300u64, 100u64, 200u64] {
                let loc = ElementWriter::create_refno_loc(refno, 2, 16, 1);
                writer
                    .insert_index_entry(&mut file, PRIMARY_EXT_NO, 1, &loc)
                    .expect("插入索引失败");
            }

            let root_page = writer
                .page_manager()
                .cache
                .get(&(PRIMARY_EXT_NO, 1))
                .map(|page| page.data.clone())
                .expect("根页应在缓存中");
            let parsed = writer.parse_index_page(&root_page).expect("解析根页失败");
            let ordered: Vec<u64> = parsed.entries.iter().map(|e| e.get_refno().0).collect();
            assert_eq!(ordered, vec![100, 200, 300]);
        }

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_insert_index_entry_splits_root_leaf() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_index_split.bin");
        let _ = std::fs::remove_file(&temp_file);

        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");
            let data = vec![0u8; 512 * 4];
            file.write_all(&data).expect("初始化失败");
        }

        let mut writer = ElementWriter::new_512();
        let empty_root = ParsedIndexPage {
            page_type: 8,
            level: 0,
            unknowns: [0; 3],
            pfno: 0,
            entries: Vec::new(),
        };

        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");
            let root_bytes = writer
                .serialize_index_page(&empty_root)
                .expect("序列化失败");
            writer
                .write_page(&mut file, PRIMARY_EXT_NO, 1, &root_bytes)
                .expect("写入根页失败");

            for i in 1..=40u64 {
                let loc = ElementWriter::create_refno_loc(i, (i + 10) as u32, 16, 1);
                writer
                    .insert_index_entry(&mut file, PRIMARY_EXT_NO, 1, &loc)
                    .expect("插入索引失败");
            }

            let root_page = writer
                .page_manager()
                .cache
                .get(&(PRIMARY_EXT_NO, 1))
                .map(|page| page.data.clone())
                .expect("根页应在缓存中");
            let parsed_root = writer.parse_index_page(&root_page).expect("解析根页失败");
            assert_eq!(parsed_root.level, 1);
            assert!(ElementWriter::is_start_marker(&parsed_root.entries[0]));
            assert!(parsed_root.entries.len() >= 3);

            let child_pgno = parsed_root.entries[1].pgno;
            let child_page = writer
                .page_manager()
                .cache
                .get(&(PRIMARY_EXT_NO, child_pgno))
                .map(|page| page.data.clone())
                .expect("子页应在缓存中");
            let parsed_child = writer.parse_index_page(&child_page).expect("解析子页失败");
            assert_eq!(parsed_child.level, 0);
            assert!(!parsed_child.entries.is_empty());
        }

        let _ = std::fs::remove_file(&temp_file);
    }

    fn find_index_entry_for_test(
        writer: &mut ElementWriter,
        file: &mut File,
        page_no: u32,
        refno: &RefnoDataLoc,
    ) -> Option<RefnoDataLoc> {
        let page_data = writer
            .page_manager
            .get_page(file, PRIMARY_EXT_NO, page_no)
            .expect("读取索引页失败")
            .to_vec();
        let parsed = writer.parse_index_page(&page_data).expect("解析索引页失败");
        if parsed.level == 0 {
            return parsed
                .entries
                .into_iter()
                .find(|entry| entry.refno_0 == refno.refno_0 && entry.refno_1 == refno.refno_1);
        }

        let mut child_pgno_candidates = Vec::new();
        for entry in parsed.entries {
            if !child_pgno_candidates.contains(&entry.pgno) {
                child_pgno_candidates.push(entry.pgno);
            }
        }

        for child_pgno in child_pgno_candidates {
            if let Some(found) = find_index_entry_for_test(writer, file, child_pgno, refno) {
                return Some(found);
            }
        }

        None
    }

    #[test]
    fn test_insert_index_entry_splits_root_internal() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_index_root_split.bin");
        let _ = std::fs::remove_file(&temp_file);

        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");
            let data = vec![0u8; 128 * 64];
            file.write_all(&data).expect("初始化失败");
        }

        let mut writer = ElementWriter::new(128);
        let empty_root = ParsedIndexPage {
            page_type: 8,
            level: 0,
            unknowns: [0; 3],
            pfno: 0,
            entries: Vec::new(),
        };

        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");
            let root_bytes = writer
                .serialize_index_page(&empty_root)
                .expect("序列化失败");
            writer
                .write_page(&mut file, PRIMARY_EXT_NO, 1, &root_bytes)
                .expect("写入根页失败");

            for i in 1..=24u64 {
                let loc = ElementWriter::create_refno_loc(i, (i + 100) as u32, 16, 1);
                writer
                    .insert_index_entry(&mut file, PRIMARY_EXT_NO, 1, &loc)
                    .expect("插入索引失败");
            }

            let root_page = writer
                .page_manager()
                .cache
                .get(&(PRIMARY_EXT_NO, 1))
                .map(|page| page.data.clone())
                .expect("根页应在缓存中");
            let parsed_root = writer.parse_index_page(&root_page).expect("解析根页失败");
            assert_eq!(parsed_root.level, 2);
            assert!(ElementWriter::is_start_marker(&parsed_root.entries[0]));
            assert_eq!(parsed_root.entries.len(), 3);

            let left_child_page = writer
                .page_manager()
                .cache
                .get(&(PRIMARY_EXT_NO, parsed_root.entries[1].pgno))
                .map(|page| page.data.clone())
                .expect("左子页应在缓存中");
            let parsed_left_child = writer
                .parse_index_page(&left_child_page)
                .expect("解析左子页失败");
            assert_eq!(parsed_left_child.level, 1);
        }

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_update_index_entry_handles_multi_level_tree() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_index_update_recursive.bin");
        let _ = std::fs::remove_file(&temp_file);

        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");
            let data = vec![0u8; 128 * 64];
            file.write_all(&data).expect("初始化失败");
        }

        let mut writer = ElementWriter::new(128);
        let empty_root = ParsedIndexPage {
            page_type: 8,
            level: 0,
            unknowns: [0; 3],
            pfno: 0,
            entries: Vec::new(),
        };

        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");
            let root_bytes = writer
                .serialize_index_page(&empty_root)
                .expect("序列化失败");
            writer
                .write_page(&mut file, PRIMARY_EXT_NO, 1, &root_bytes)
                .expect("写入根页失败");

            for i in 1..=24u64 {
                let loc = ElementWriter::create_refno_loc(i, (i + 200) as u32, 16, 1);
                writer
                    .insert_index_entry(&mut file, PRIMARY_EXT_NO, 1, &loc)
                    .expect("插入索引失败");
            }

            let updated = ElementWriter::create_refno_loc(18u64, 999, 24, 7);
            writer
                .update_index_entry(&mut file, PRIMARY_EXT_NO, 1, &updated)
                .expect("递归更新失败");

            let found = find_index_entry_for_test(&mut writer, &mut file, 1, &updated)
                .expect("未在多层树中找到更新后的条目");
            assert_eq!(found.pgno, 999);
            assert_eq!(found.offset, 24);
            assert_eq!(found.flag, 7);
        }

        let _ = std::fs::remove_file(&temp_file);
    }
}
