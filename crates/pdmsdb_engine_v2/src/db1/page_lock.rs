use std::collections::HashMap;

use crate::core::PageId;

#[derive(Debug, Clone)]
struct LockEntry {
    lock_count: u32,
    referenced: bool,
}

pub struct PageLockManager {
    locks: HashMap<PageId, LockEntry>,
}

impl PageLockManager {
    pub fn new() -> Self {
        Self {
            locks: HashMap::new(),
        }
    }

    pub fn lock(&mut self, page_id: PageId) {
        let entry = self.locks.entry(page_id).or_insert(LockEntry {
            lock_count: 0,
            referenced: false,
        });
        entry.lock_count += 1;
        entry.referenced = true;
    }

    pub fn unlock(&mut self, page_id: PageId) -> bool {
        if let Some(entry) = self.locks.get_mut(&page_id) {
            if entry.lock_count > 0 {
                entry.lock_count -= 1;
            }
            if entry.lock_count == 0 {
                self.locks.remove(&page_id);
            }
            true
        } else {
            false
        }
    }

    pub fn is_locked(&self, page_id: PageId) -> bool {
        self.locks
            .get(&page_id)
            .is_some_and(|e| e.lock_count > 0)
    }

    pub fn lock_count(&self, page_id: PageId) -> u32 {
        self.locks
            .get(&page_id)
            .map(|e| e.lock_count)
            .unwrap_or(0)
    }

    pub fn is_referenced(&self, page_id: PageId) -> bool {
        self.locks
            .get(&page_id)
            .is_some_and(|e| e.referenced)
    }

    pub fn clear_referenced(&mut self, page_id: PageId) {
        if let Some(entry) = self.locks.get_mut(&page_id) {
            entry.referenced = false;
        }
    }

    pub fn locked_page_ids(&self) -> Vec<PageId> {
        self.locks
            .iter()
            .filter(|(_, e)| e.lock_count > 0)
            .map(|(k, _)| *k)
            .collect()
    }

    pub fn total_locked(&self) -> usize {
        self.locks.values().filter(|e| e.lock_count > 0).count()
    }

    pub fn clear_all(&mut self) {
        self.locks.clear();
    }
}

impl Default for PageLockManager {
    fn default() -> Self {
        Self::new()
    }
}
