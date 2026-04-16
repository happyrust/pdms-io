use crate::core::{EngineError, RecordLoc, RefNo};

#[derive(Debug, Clone)]
pub struct ElementHandle {
    pub refno: RefNo,
    pub loc: RecordLoc,
    pub raw_data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavDirection {
    Owner,
    FirstMember,
    LastMember,
    NextSibling,
    PrevSibling,
}

pub struct CurrentElement {
    stack: Vec<ElementHandle>,
}

impl CurrentElement {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    pub fn current(&self) -> Option<&ElementHandle> {
        self.stack.last()
    }

    pub fn current_mut(&mut self) -> Option<&mut ElementHandle> {
        self.stack.last_mut()
    }

    pub fn push(&mut self, handle: ElementHandle) {
        self.stack.push(handle);
    }

    pub fn pop(&mut self) -> Option<ElementHandle> {
        self.stack.pop()
    }

    pub fn clear(&mut self) {
        self.stack.clear();
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn set(&mut self, handle: ElementHandle) {
        self.stack.clear();
        self.stack.push(handle);
    }

    pub fn go_to(&mut self, handle: ElementHandle) {
        self.push(handle);
    }

    /// Pop back to previous element on the stack.
    /// Returns the popped (current) handle, leaving the previous one as current.
    pub fn back(&mut self) -> Result<ElementHandle, EngineError> {
        if self.stack.len() <= 1 {
            return Err(EngineError::InvalidState(
                "导航栈底部，无法 back".into(),
            ));
        }
        Ok(self.stack.pop().unwrap())
    }

    /// Peek at the element below current on the stack (the "caller").
    pub fn peek_previous(&self) -> Option<&ElementHandle> {
        if self.stack.len() >= 2 {
            Some(&self.stack[self.stack.len() - 2])
        } else {
            None
        }
    }

    pub fn require_current(&self) -> Result<&ElementHandle, EngineError> {
        self.current()
            .ok_or_else(|| EngineError::InvalidState("CE 未设置，无当前元素".into()))
    }

    pub fn stack_refnos(&self) -> Vec<RefNo> {
        self.stack.iter().map(|h| h.refno).collect()
    }
}

impl Default for CurrentElement {
    fn default() -> Self {
        Self::new()
    }
}
