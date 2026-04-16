use crate::engine_v2::db1::PageCache;
use crate::engine_v2::io_layer::FileHandle;
use crate::engine_v2::types::*;
use super::btree::{BTreeNode, START_MARKER, max_entries_per_page};
use super::split::BTreeSplit;

/// B-树索引插入 (对齐 FHXPND)
///
/// 将 IndexEntry 插入到 B-树叶子节点，保持排序。
/// 当节点满时触发分裂 (FHSPLT)。
pub struct BTreeInsert;

impl BTreeInsert {
    /// 插入条目到 B-树
    ///
    /// 返回是否发生了根分裂 (需要更新根页号)
    pub fn insert(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        root_pgno: u32,
        entry: IndexEntry,
        page_size: usize,
    ) -> DbResult<Option<u32>> {
        let result = Self::insert_recursive(
            cache, handle, dbno, extent, root_pgno, entry, page_size,
        )?;

        match result {
            InsertResult::Done => Ok(None),
            InsertResult::Split { promoted, new_page } => {
                let new_root = BTreeSplit::split_root(
                    cache, handle, dbno, extent, root_pgno, promoted, new_page, page_size,
                )?;
                Ok(Some(new_root))
            }
        }
    }

    fn insert_recursive(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        page_no: u32,
        entry: IndexEntry,
        page_size: usize,
    ) -> DbResult<InsertResult> {
        let data = cache.get_page(handle, dbno, extent, page_no)?;
        let node = BTreeNode::from_page_data(page_no, data);
        let page_id = PageId::new(dbno, page_no, extent);
        cache.unlock_page(&page_id);

        if node.is_leaf() {
            return Self::insert_into_leaf(
                cache, handle, dbno, extent, page_no, node, entry, page_size,
            );
        }

        let valid: Vec<_> = node.entries.iter()
            .filter(|e| e.refno != START_MARKER)
            .collect();

        let child_page = if valid.is_empty() {
            node.start_marker_page().unwrap_or(page_no)
        } else {
            let mut child = valid.last().unwrap().page_no;
            for e in &valid {
                if (entry.refno.hi, entry.refno.lo) <= (e.refno.hi, e.refno.lo) {
                    child = e.page_no;
                    break;
                }
            }
            child
        };

        let result = Self::insert_recursive(
            cache, handle, dbno, extent, child_page, entry, page_size,
        )?;

        match result {
            InsertResult::Done => {
                Self::update_internal_boundary(cache, handle, dbno, extent, page_no, child_page)?;
                Ok(InsertResult::Done)
            }
            InsertResult::Split { promoted, new_page } => {
                Self::insert_into_internal(
                    cache, handle, dbno, extent, page_no, node, promoted, new_page, page_size,
                )
            }
        }
    }

    fn insert_into_leaf(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        page_no: u32,
        node: BTreeNode,
        entry: IndexEntry,
        page_size: usize,
    ) -> DbResult<InsertResult> {
        let max = max_entries_per_page(page_size);

        if node.entries.len() < max {
            let data = cache.get_page_mut(handle, dbno, extent, page_no)?;
            Self::sorted_insert_entry(data, &node, &entry, page_size);
            Ok(InsertResult::Done)
        } else {
            let (promoted, new_page) = BTreeSplit::split_leaf(
                cache, handle, dbno, extent, page_no, node, entry, page_size,
            )?;
            Ok(InsertResult::Split { promoted, new_page })
        }
    }

    fn insert_into_internal(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        page_no: u32,
        node: BTreeNode,
        promoted: IndexEntry,
        _new_child: u32,
        page_size: usize,
    ) -> DbResult<InsertResult> {
        let max = max_entries_per_page(page_size);

        if node.entries.len() < max {
            let data = cache.get_page_mut(handle, dbno, extent, page_no)?;
            Self::sorted_insert_entry(data, &node, &promoted, page_size);
            Ok(InsertResult::Done)
        } else {
            let (re_promoted, new_page) = BTreeSplit::split_internal(
                cache, handle, dbno, extent, page_no, node, promoted, page_size,
            )?;
            Ok(InsertResult::Split { promoted: re_promoted, new_page: new_page })
        }
    }

    /// 排序插入条目到页面数据
    fn sorted_insert_entry(
        data: &mut [u8],
        node: &BTreeNode,
        entry: &IndexEntry,
        _page_size: usize,
    ) {
        let mut entries = node.entries.clone();
        let pos = entries.partition_point(|e| {
            (e.refno.hi, e.refno.lo) < (entry.refno.hi, entry.refno.lo)
        });
        entries.insert(pos, *entry);

        let count = entries.len() as u32;
        data[16..20].copy_from_slice(&count.to_be_bytes());

        for (i, e) in entries.iter().enumerate() {
            let offset = INDEX_PAGE_HEADER_SIZE + i * IndexEntry::SIZE;
            data[offset..offset + IndexEntry::SIZE].copy_from_slice(&e.to_be_bytes());
        }
    }

    fn update_internal_boundary(
        _cache: &mut PageCache,
        _handle: &mut FileHandle,
        _dbno: u32,
        _extent: u32,
        _parent_page: u32,
        _child_page: u32,
    ) -> DbResult<()> {
        // TODO: 更新父节点中 child_page 对应条目的边界值
        Ok(())
    }
}

use crate::engine_v2::types::INDEX_PAGE_HEADER_SIZE;

enum InsertResult {
    Done,
    Split { promoted: IndexEntry, new_page: u32 },
}
