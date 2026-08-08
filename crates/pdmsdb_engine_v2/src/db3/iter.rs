use std::collections::HashSet;
use std::fs::File;

use crate::core::{EngineError, PageId, RecordLoc, RefNo};
use crate::db1::PageStore;

use super::index::{IndexEntry, IndexPageView};

#[derive(Debug, Clone)]
pub struct IndexIteratorEntry {
    pub refno: RefNo,
    pub loc: RecordLoc,
}

/// 中序遍历整棵 RefNo 索引树。
///
/// 内部页的每个条目（含起始标记）各指向一棵独立子树，因此下降路径可以有任意深度，
/// 回溯也必须能一路退到根；只退一层会在两层以上的树上把绝大多数叶子甩掉。
pub struct IndexTableIterator {
    /// 尚未走完的内部页：(页号, 已经下降过的条目下标)
    stack: Vec<(PageId, usize)>,
    pending: Vec<IndexEntry>,
    pending_idx: usize,
    /// 当前叶子所在的扩展文件号；条目本身只存页号，位置要靠它补全。
    pending_ext_no: u32,
    visited: HashSet<(u32, u32)>,
    finished: bool,
}

impl IndexTableIterator {
    pub fn new(file: &mut File, store: &mut PageStore, root: PageId) -> Result<Self, EngineError> {
        let mut iter = Self {
            stack: Vec::new(),
            pending: Vec::new(),
            pending_idx: 0,
            pending_ext_no: root.ext_no,
            visited: HashSet::new(),
            finished: false,
        };

        iter.descend(file, store, root)?;
        Ok(iter)
    }

    pub fn next(
        &mut self,
        file: &mut File,
        store: &mut PageStore,
    ) -> Result<Option<IndexIteratorEntry>, EngineError> {
        loop {
            if self.finished {
                return Ok(None);
            }

            while self.pending_idx < self.pending.len() {
                let entry = self.pending[self.pending_idx];
                self.pending_idx += 1;

                if entry.is_start_marker() {
                    continue;
                }

                return Ok(Some(IndexIteratorEntry {
                    refno: entry.refno,
                    loc: entry.to_record_loc(self.pending_ext_no),
                }));
            }

            if !self.advance_to_next_leaf(file, store)? {
                self.finished = true;
                return Ok(None);
            }
        }
    }

    pub fn collect_all(
        &mut self,
        file: &mut File,
        store: &mut PageStore,
    ) -> Result<Vec<IndexIteratorEntry>, EngineError> {
        let mut result = Vec::new();
        while let Some(entry) = self.next(file, store)? {
            result.push(entry);
        }
        Ok(result)
    }

    /// 从 `page_id` 一路下降到最左叶子，途经的内部页压栈备用。
    fn descend(
        &mut self,
        file: &mut File,
        store: &mut PageStore,
        page_id: PageId,
    ) -> Result<(), EngineError> {
        let mut current = page_id;
        loop {
            self.pending_ext_no = current.ext_no;
            if !self.visited.insert((current.ext_no, current.page_no)) {
                self.pending = Vec::new();
                self.pending_idx = 0;
                return Ok(());
            }

            let page = store.read_page(file, current)?;
            let Ok(parsed) = IndexPageView::from_page(&page) else {
                self.pending = Vec::new();
                self.pending_idx = 0;
                return Ok(());
            };

            if parsed.level == 0 {
                self.pending = parsed.entries;
                self.pending_idx = 0;
                return Ok(());
            }

            let Some(first_child) = parsed.entries.first().map(|entry| entry.page_no) else {
                self.pending = Vec::new();
                self.pending_idx = 0;
                return Ok(());
            };

            self.stack.push((current, 0));
            current = PageId {
                ext_no: current.ext_no,
                page_no: first_child,
            };
        }
    }

    /// 逐层回溯，直到找到还有未访问子树的祖先并降到它下一棵子树的最左叶子。
    fn advance_to_next_leaf(
        &mut self,
        file: &mut File,
        store: &mut PageStore,
    ) -> Result<bool, EngineError> {
        while let Some((parent, child_idx)) = self.stack.pop() {
            let page = store.read_page(file, parent)?;
            let Ok(parsed) = IndexPageView::from_page(&page) else {
                continue;
            };

            let next_idx = child_idx + 1;
            let Some(next_entry) = parsed.entries.get(next_idx) else {
                continue;
            };

            self.stack.push((parent, next_idx));
            self.descend(
                file,
                store,
                PageId {
                    ext_no: parent.ext_no,
                    page_no: next_entry.page_no,
                },
            )?;
            return Ok(true);
        }

        Ok(false)
    }
}

pub fn scan_all_entries(
    file: &mut File,
    store: &mut PageStore,
    root: PageId,
) -> Result<Vec<IndexIteratorEntry>, EngineError> {
    let mut iter = IndexTableIterator::new(file, store, root)?;
    iter.collect_all(file, store)
}

/// Stream every live leaf entry under `root` to `visitor` without retaining a
/// database-sized result vector.
pub fn visit_all_entries<F>(
    file: &mut File,
    store: &mut PageStore,
    root: PageId,
    mut visitor: F,
) -> Result<(), EngineError>
where
    F: FnMut(IndexIteratorEntry) -> Result<(), EngineError>,
{
    let mut iter = IndexTableIterator::new(file, store, root)?;
    while let Some(entry) = iter.next(file, store)? {
        visitor(entry)?;
    }
    Ok(())
}
