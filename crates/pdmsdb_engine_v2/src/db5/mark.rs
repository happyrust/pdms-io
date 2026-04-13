use crate::core::{EngineError, PageId, SessionSnapshot};

#[derive(Debug, Clone)]
pub struct TransactionMark {
    pub mark_id: u32,
    pub session_at_mark: SessionSnapshot,
    pub index_root_at_mark: PageId,
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
        let mark_id = self.next_mark_id;
        self.next_mark_id += 1;
        self.marks.push(TransactionMark {
            mark_id,
            session_at_mark: session,
            index_root_at_mark: index_root,
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

    pub fn latest_mark(&self) -> Option<&TransactionMark> {
        self.marks.last()
    }

    pub fn has_marks(&self) -> bool {
        !self.marks.is_empty()
    }

    pub fn clear_marks(&mut self) {
        self.marks.clear();
    }
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::new()
    }
}
