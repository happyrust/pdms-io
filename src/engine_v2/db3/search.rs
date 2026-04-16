use crate::engine_v2::db1::PageCache;
use crate::engine_v2::io_layer::FileHandle;
use crate::engine_v2::types::*;
use super::btree::{BTreeNode, START_MARKER};

/// B-树搜索 (对齐 FHSRCH)
///
/// 从根节点出发，沿 根→中间→叶 三级路径定位目标 RefNo。
pub struct BTreeSearch;

impl BTreeSearch {
    /// 精确搜索：返回 (page_no, offset) 或 None
    pub fn find(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        root_pgno: u32,
        target: RefNo,
    ) -> DbResult<Option<RefnoDataLoc>> {
        Self::search_recursive(cache, handle, dbno, extent, root_pgno, &target)
    }

    fn search_recursive(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        page_no: u32,
        target: &RefNo,
    ) -> DbResult<Option<RefnoDataLoc>> {
        let data = cache.get_page(handle, dbno, extent, page_no)?;
        let node = BTreeNode::from_page_data(page_no, data);
        let page_id = PageId::new(dbno, page_no, extent);
        cache.unlock_page(&page_id);

        if node.is_leaf() {
            return Ok(Self::search_leaf(&node, target, dbno));
        }

        Self::search_internal(cache, handle, dbno, extent, &node, target)
    }

    /// 叶子节点精确匹配
    fn search_leaf(node: &BTreeNode, target: &RefNo, dbno: u32) -> Option<RefnoDataLoc> {
        for entry in &node.entries {
            if entry.refno == *target {
                return Some(RefnoDataLoc {
                    refno: *target,
                    dbno,
                    page_no: entry.page_no,
                    offset: entry.offset(),
                });
            }
        }
        None
    }

    /// 非叶子节点：选择子树并递归
    ///
    /// 逻辑对齐 core.dll 的 FHSRCH:
    /// 1. 如果 target < 第一个有效条目，走起始标记分支
    /// 2. 否则找到最后一个 refno <= target 的条目，走该分支
    /// 3. 如果 target > 所有条目，走最后一个分支
    fn search_internal(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        node: &BTreeNode,
        target: &RefNo,
    ) -> DbResult<Option<RefnoDataLoc>> {
        let valid: Vec<_> = node.entries.iter()
            .filter(|e| e.refno != START_MARKER)
            .collect();

        if valid.is_empty() {
            if let Some(child) = node.start_marker_page() {
                return Self::search_recursive(cache, handle, dbno, extent, child, target);
            }
            return Ok(None);
        }

        let first = valid[0];
        let cmp_first = (target.hi, target.lo).cmp(&(first.refno.hi, first.refno.lo));

        if cmp_first == std::cmp::Ordering::Less {
            if let Some(child) = node.start_marker_page() {
                return Self::search_recursive(cache, handle, dbno, extent, child, target);
            }
        }

        let mut selected_child = valid.last().unwrap().page_no;
        for entry in &valid {
            let cmp = (target.hi, target.lo).cmp(&(entry.refno.hi, entry.refno.lo));
            if cmp == std::cmp::Ordering::Less || cmp == std::cmp::Ordering::Equal {
                selected_child = entry.page_no;
                break;
            }
        }

        Self::search_recursive(cache, handle, dbno, extent, selected_child, target)
    }
}
