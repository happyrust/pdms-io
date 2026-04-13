use crate::core::{EngineError, RecordLoc, RefNo};

#[derive(Debug, Clone)]
pub struct ElementHandle {
    pub refno: RefNo,
    pub loc: RecordLoc,
    pub raw_data: Vec<u8>,
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

    pub fn require_current(&self) -> Result<&ElementHandle, EngineError> {
        self.current()
            .ok_or_else(|| EngineError::InvalidState("CE 未设置，无当前元素".into()))
    }
}

impl Default for CurrentElement {
    fn default() -> Self {
        Self::new()
    }
}
