use std::collections::HashMap;

use crate::core::EngineError;

#[derive(Debug, Clone)]
pub struct DbBlockEntry {
    pub db_no: i32,
    pub file_path: String,
    pub open_mode: DbOpenMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbOpenMode {
    ReadOnly,
    ReadWrite,
    Shared,
    Closed,
}

pub struct DbLookupTable {
    entries: HashMap<i32, DbBlockEntry>,
    max_blocks: usize,
}

impl DbLookupTable {
    pub fn new(max_blocks: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(max_blocks),
            max_blocks,
        }
    }

    pub fn create_entry(&mut self, entry: DbBlockEntry) -> Result<(), EngineError> {
        if self.entries.len() >= self.max_blocks {
            return Err(EngineError::InvalidState(format!(
                "数据库块查找表已满 (max={})",
                self.max_blocks
            )));
        }
        self.entries.insert(entry.db_no, entry);
        Ok(())
    }

    pub fn find(&self, db_no: i32) -> Option<&DbBlockEntry> {
        self.entries.get(&db_no)
    }

    pub fn find_mut(&mut self, db_no: i32) -> Option<&mut DbBlockEntry> {
        self.entries.get_mut(&db_no)
    }

    pub fn remove(&mut self, db_no: i32) -> Option<DbBlockEntry> {
        self.entries.remove(&db_no)
    }

    pub fn find_empty_slot(&self) -> Option<i32> {
        for i in 1..=self.max_blocks as i32 {
            if !self.entries.contains_key(&i) {
                return Some(i);
            }
        }
        None
    }

    pub fn find_current(&self) -> Option<&DbBlockEntry> {
        self.entries
            .values()
            .find(|e| e.open_mode == DbOpenMode::ReadWrite)
    }

    pub fn has_aux_blocks(&self) -> bool {
        self.entries.len() > 1
    }

    pub fn active_count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.open_mode != DbOpenMode::Closed)
            .count()
    }

    pub fn list_all(&self) -> Vec<&DbBlockEntry> {
        self.entries.values().collect()
    }
}

impl Default for DbLookupTable {
    fn default() -> Self {
        Self::new(64)
    }
}
