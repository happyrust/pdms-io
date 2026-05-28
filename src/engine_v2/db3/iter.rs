use super::btree::{BTreeNode, START_MARKER};
use crate::engine_v2::db1::PageCache;
use crate::engine_v2::io_layer::FileHandle;
use crate::engine_v2::types::*;

/// B-树迭代器 (对齐 DB_IndexTableIterator)
///
/// 按 RefNo 顺序遍历所有叶子节点的索引条目。
pub struct TableIterator {
    /// 待访问的叶子页面栈 (DFS 遍历)
    leaf_pages: Vec<u32>,
    /// 当前叶子页的条目
    current_entries: Vec<IndexEntry>,
    /// 当前条目的索引
    current_idx: usize,
    dbno: u32,
    extent: u32,
}

impl TableIterator {
    /// 创建迭代器 (对齐 DB_IndexTableIterator::ctor)
    pub fn new(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        root_pgno: u32,
    ) -> DbResult<Self> {
        let mut iter = Self {
            leaf_pages: Vec::new(),
            current_entries: Vec::new(),
            current_idx: 0,
            dbno,
            extent,
        };

        iter.collect_leaves(cache, handle, root_pgno)?;
        iter.leaf_pages.reverse();

        if let Some(first_page) = iter.leaf_pages.pop() {
            iter.load_leaf(cache, handle, first_page)?;
        }

        Ok(iter)
    }

    /// 获取下一个条目 (对齐 DB_IndexTableIterator::increment)
    pub fn next(
        &mut self,
        cache: &mut PageCache,
        handle: &mut FileHandle,
    ) -> DbResult<Option<IndexEntry>> {
        loop {
            if self.current_idx < self.current_entries.len() {
                let entry = self.current_entries[self.current_idx];
                self.current_idx += 1;
                return Ok(Some(entry));
            }

            match self.leaf_pages.pop() {
                Some(page_no) => self.load_leaf(cache, handle, page_no)?,
                None => return Ok(None),
            }
        }
    }

    /// 收集所有条目到 Vec (便捷方法)
    pub fn collect_all(
        &mut self,
        cache: &mut PageCache,
        handle: &mut FileHandle,
    ) -> DbResult<Vec<IndexEntry>> {
        let mut result = Vec::new();
        while let Some(entry) = self.next(cache, handle)? {
            result.push(entry);
        }
        Ok(result)
    }

    /// DFS 收集所有叶子页面号
    fn collect_leaves(
        &mut self,
        cache: &mut PageCache,
        handle: &mut FileHandle,
        page_no: u32,
    ) -> DbResult<()> {
        let data = cache.get_page(handle, self.dbno, self.extent, page_no)?;
        let node = BTreeNode::from_page_data(page_no, data);
        let page_id = PageId::new(self.dbno, page_no, self.extent);
        cache.unlock_page(&page_id);

        if node.is_leaf() {
            self.leaf_pages.push(page_no);
            return Ok(());
        }

        if let Some(start_page) = node.start_marker_page() {
            self.collect_leaves(cache, handle, start_page)?;
        }

        for entry in node.valid_entries() {
            self.collect_leaves(cache, handle, entry.page_no)?;
        }

        Ok(())
    }

    fn load_leaf(
        &mut self,
        cache: &mut PageCache,
        handle: &mut FileHandle,
        page_no: u32,
    ) -> DbResult<()> {
        let data = cache.get_page(handle, self.dbno, self.extent, page_no)?;
        let node = BTreeNode::from_page_data(page_no, data);
        let page_id = PageId::new(self.dbno, page_no, self.extent);
        cache.unlock_page(&page_id);

        self.current_entries = node.entries;
        self.current_idx = 0;
        Ok(())
    }
}

/// 单页扫描 (对齐 FHITER)
pub fn scan_page(
    cache: &mut PageCache,
    handle: &mut FileHandle,
    dbno: u32,
    extent: u32,
    page_no: u32,
) -> DbResult<Vec<IndexEntry>> {
    let data = cache.get_page(handle, dbno, extent, page_no)?;
    let node = BTreeNode::from_page_data(page_no, data);
    let page_id = PageId::new(dbno, page_no, extent);
    cache.unlock_page(&page_id);
    Ok(node.entries)
}
