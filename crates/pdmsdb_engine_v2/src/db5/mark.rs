use std::collections::BTreeSet;

use crate::core::{EngineError, PageId, RefNo, SessionSnapshot};

#[derive(Debug, Clone)]
pub struct TransactionMark {
    pub mark_id: u32,
    pub session_at_mark: SessionSnapshot,
    pub index_root_at_mark: PageId,
    pub ce_refno_at_mark: Option<RefNo>,
    pub inserted_refnos_at_mark: BTreeSet<RefNo>,
}

pub struct TransactionManager {
    marks: Vec<TransactionMark>,
    next_mark_id: u32,
}

impl TransactionManager {
    pub fn new() -> Self {
        Self {
            marks: Vec::new(),
            next_mark_id: 1,
        }
    }

    pub fn set_mark(
        &mut self,
        session: SessionSnapshot,
        index_root: PageId,
    ) -> u32 {
        self.set_mark_full(session, index_root, None, BTreeSet::new())
    }

    pub fn set_mark_full(
        &mut self,
        session: SessionSnapshot,
        index_root: PageId,
        ce_refno: Option<RefNo>,
        inserted_refnos: BTreeSet<RefNo>,
    ) -> u32 {
        let mark_id = self.next_mark_id;
        self.next_mark_id += 1;
        self.marks.push(TransactionMark {
            mark_id,
            session_at_mark: session,
            index_root_at_mark: index_root,
            ce_refno_at_mark: ce_refno,
            inserted_refnos_at_mark: inserted_refnos,
        });
        mark_id
    }

    pub fn undo_to_mark(&mut self, mark_id: u32) -> Result<TransactionMark, EngineError> {
        let idx = self
            .marks
            .iter()
            .position(|m| m.mark_id == mark_id)
            .ok_or_else(|| EngineError::NotFound(format!("mark_id {} 不存在", mark_id)))?;

        let mark = self.marks[idx].clone();
        self.marks.truncate(idx);
        Ok(mark)
    }

    /// Undo the most recent mark (shorthand for undo_to_mark with latest).
    pub fn undo_latest(&mut self) -> Result<TransactionMark, EngineError> {
        let mark = self
            .marks
            .last()
            .cloned()
            .ok_or_else(|| EngineError::InvalidState("无可 undo 的 mark".into()))?;
        self.marks.pop();
        Ok(mark)
    }

    pub fn latest_mark(&self) -> Option<&TransactionMark> {
        self.marks.last()
    }

    pub fn mark_count(&self) -> usize {
        self.marks.len()
    }

    pub fn has_marks(&self) -> bool {
        !self.marks.is_empty()
    }

    pub fn clear_marks(&mut self) {
        self.marks.clear();
    }

    pub fn all_marks(&self) -> &[TransactionMark] {
        &self.marks
    }
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::new()
    }
}
