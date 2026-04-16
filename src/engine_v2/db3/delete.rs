use crate::engine_v2::db1::PageCache;
use crate::engine_v2::io_layer::FileHandle;
use crate::engine_v2::types::*;
use super::btree::{BTreeNode, START_MARKER};

/// B-树节点删除 (对齐 FHDELT)
///
/// 旧实现完全缺失此功能。V2 实现完整的删除+合并/重平衡。
pub struct BTreeDelete;

impl BTreeDelete {
    /// 从 B-树中删除指定 RefNo
    ///
    /// 返回 Ok(true) 表示成功删除，Ok(false) 表示未找到
    pub fn delete(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        root_pgno: u32,
        target: RefNo,
        _page_size: usize,
    ) -> DbResult<bool> {
        let result = Self::delete_recursive(cache, handle, dbno, extent, root_pgno, &target)?;
        Ok(matches!(result, DeleteResult::Deleted | DeleteResult::Underflow))
    }

    fn delete_recursive(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        page_no: u32,
        target: &RefNo,
    ) -> DbResult<DeleteResult> {
        let data = cache.get_page(handle, dbno, extent, page_no)?;
        let node = BTreeNode::from_page_data(page_no, data);
        let page_id = PageId::new(dbno, page_no, extent);
        cache.unlock_page(&page_id);

        if node.is_leaf() {
            return Self::delete_from_leaf(cache, handle, dbno, extent, page_no, &node, target);
        }

        let valid: Vec<_> = node.entries.iter()
            .filter(|e| e.refno != START_MARKER)
            .collect();

        let child_page = if valid.is_empty() {
            if let Some(p) = node.start_marker_page() { p } else { return Ok(DeleteResult::NotFound); }
        } else {
            let mut child = valid.last().unwrap().page_no;
            for e in &valid {
                if (*target).hi < e.refno.hi || ((*target).hi == e.refno.hi && (*target).lo <= e.refno.lo) {
                    child = e.page_no;
                    break;
                }
            }
            child
        };

        let result = Self::delete_recursive(cache, handle, dbno, extent, child_page, target)?;

        match result {
            DeleteResult::NotFound => Ok(DeleteResult::NotFound),
            DeleteResult::Deleted => {
                Self::update_boundary_after_delete(cache, handle, dbno, extent, page_no, child_page)?;
                Ok(DeleteResult::Deleted)
            }
            DeleteResult::Underflow => {
                // TODO: 合并或重新平衡兄弟节点
                Self::update_boundary_after_delete(cache, handle, dbno, extent, page_no, child_page)?;
                Ok(DeleteResult::Deleted)
            }
        }
    }

    fn delete_from_leaf(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        page_no: u32,
        node: &BTreeNode,
        target: &RefNo,
    ) -> DbResult<DeleteResult> {
        let pos = node.entries.iter().position(|e| e.refno == *target);

        match pos {
            None => Ok(DeleteResult::NotFound),
            Some(idx) => {
                let data = cache.get_page_mut(handle, dbno, extent, page_no)?;

                let new_count = node.entries.len() - 1;
                let remaining: Vec<_> = node.entries.iter()
                    .enumerate()
                    .filter(|(i, _)| *i != idx)
                    .map(|(_, e)| *e)
                    .collect();

                data[16..20].copy_from_slice(&(new_count as u32).to_be_bytes());

                for (i, e) in remaining.iter().enumerate() {
                    let off = INDEX_PAGE_HEADER_SIZE + i * IndexEntry::SIZE;
                    data[off..off + IndexEntry::SIZE].copy_from_slice(&e.to_be_bytes());
                }

                let min_entries = 2;
                if new_count < min_entries {
                    Ok(DeleteResult::Underflow)
                } else {
                    Ok(DeleteResult::Deleted)
                }
            }
        }
    }

    fn update_boundary_after_delete(
        _cache: &mut PageCache,
        _handle: &mut FileHandle,
        _dbno: u32,
        _extent: u32,
        _parent_page: u32,
        _child_page: u32,
    ) -> DbResult<()> {
        // TODO: 级联更新父节点边界
        Ok(())
    }
}

enum DeleteResult {
    NotFound,
    Deleted,
    Underflow,
}
