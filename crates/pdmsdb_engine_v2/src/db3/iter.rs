use std::fs::File;

use crate::core::{EngineError, PageId, RecordLoc, RefNo};
use crate::db1::PageStore;

use super::index::{IndexEntry, IndexPageView};

#[derive(Debug, Clone)]
pub struct IndexIteratorEntry {
    pub refno: RefNo,
    pub loc: RecordLoc,
}

pub struct IndexTableIterator {
    leaf_stack: Vec<(PageId, usize)>,
    pending_entries: Vec<IndexEntry>,
    current_index: usize,
    page_size: usize,
    finished: bool,
}

impl IndexTableIterator {
    pub fn new(
        file: &mut File,
        store: &mut PageStore,
        root: PageId,
    ) -> Result<Self, EngineError> {
        let mut iter = Self {
            leaf_stack: Vec::new(),
            pending_entries: Vec::new(),
            current_index: 0,
            page_size: store.page_size(),
            finished: false,
        };

        iter.descend_to_leftmost_leaf(file, store, root)?;
        Ok(iter)
    }

    pub fn next(
        &mut self,
        file: &mut File,
        store: &mut PageStore,
    ) -> Result<Option<IndexIteratorEntry>, EngineError> {
        if self.finished {
            return Ok(None);
        }

        while self.current_index < self.pending_entries.len() {
            let entry = self.pending_entries[self.current_index];
            self.current_index += 1;

            if entry.is_start_marker() {
                continue;
            }

            return Ok(Some(IndexIteratorEntry {
                refno: entry.refno,
                loc: entry.to_record_loc(),
            }));
        }

        if let Some((page_id, parent_idx)) = self.leaf_stack.pop() {
            let parent_page_id = PageId {
                ext_no: page_id.ext_no,
                page_no: page_id.page_no,
            };
            if let Ok(Some(next_leaf)) =
                self.find_next_sibling(file, store, parent_page_id, parent_idx)
            {
                self.descend_to_leftmost_leaf(file, store, next_leaf)?;
                return self.next(file, store);
            }
        }

        self.finished = true;
        Ok(None)
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

    fn descend_to_leftmost_leaf(
        &mut self,
        file: &mut File,
        store: &mut PageStore,
        page_id: PageId,
    ) -> Result<(), EngineError> {
        let page = store.read_page(file, page_id)?;
        let parsed = IndexPageView::from_page(&page)?;

        if parsed.level == 0 {
            self.pending_entries = parsed.entries;
            self.current_index = 0;
            return Ok(());
        }

        let first_child = parsed
            .entries
            .first()
            .map(|e| e.page_no)
            .ok_or_else(|| EngineError::Format("内部索引页无子节点".into()))?;

        self.leaf_stack.push((page_id, 0));
        self.descend_to_leftmost_leaf(
            file,
            store,
            PageId {
                ext_no: page_id.ext_no,
                page_no: first_child,
            },
        )
    }

    fn find_next_sibling(
        &mut self,
        file: &mut File,
        store: &mut PageStore,
        parent_page_id: PageId,
        current_child_idx: usize,
    ) -> Result<Option<PageId>, EngineError> {
        let page = store.read_page(file, parent_page_id)?;
        let parsed = IndexPageView::from_page(&page)?;

        let next_idx = current_child_idx + 1;
        if next_idx < parsed.entries.len() {
            let next_entry = &parsed.entries[next_idx];
            self.leaf_stack.push((parent_page_id, next_idx));
            return Ok(Some(PageId {
                ext_no: parent_page_id.ext_no,
                page_no: next_entry.page_no,
            }));
        }

        Ok(None)
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
