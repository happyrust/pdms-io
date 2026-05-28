use crate::engine_v2::types::RefNo;

/// Current Element 导航栈 (对齐 core.dll CE 指针管理)
///
/// 旧实现完全缺失。V2 实现完整的 CE 指针 + 导航栈。
///
/// CE 是 PDMS 的核心导航概念：
/// - go_to_element(refno) — 将 CE 移动到目标元素 (opcode 108)
/// - push — 保存当前 CE 到栈
/// - pop — 恢复栈顶 CE
/// - clear_stack — 清除导航栈 (opcode 38)
#[derive(Debug)]
pub struct CurrentElement {
    current: Option<CeState>,
    stack: Vec<CeState>,
}

#[derive(Debug, Clone)]
struct CeState {
    refno: RefNo,
    dbno: u32,
    page_no: u32,
    offset: u32,
}

impl CurrentElement {
    pub fn new() -> Self {
        Self {
            current: None,
            stack: Vec::with_capacity(32),
        }
    }

    /// 移动 CE 到指定元素 (opcode 108)
    pub fn go_to(&mut self, refno: RefNo, dbno: u32, page_no: u32, offset: u32) {
        self.current = Some(CeState {
            refno,
            dbno,
            page_no,
            offset,
        });
    }

    /// 获取当前 CE 的 RefNo
    pub fn current_refno(&self) -> Option<RefNo> {
        self.current.as_ref().map(|s| s.refno)
    }

    /// 获取当前 CE 的物理位置
    pub fn current_location(&self) -> Option<(u32, u32, u32)> {
        self.current.as_ref().map(|s| (s.dbno, s.page_no, s.offset))
    }

    /// 压栈
    pub fn push(&mut self) {
        if let Some(ref state) = self.current {
            self.stack.push(state.clone());
        }
    }

    /// 弹栈
    pub fn pop(&mut self) -> bool {
        match self.stack.pop() {
            Some(state) => {
                self.current = Some(state);
                true
            }
            None => false,
        }
    }

    /// 清除导航栈 (opcode 38)
    pub fn clear_stack(&mut self) {
        self.stack.clear();
    }

    pub fn stack_depth(&self) -> usize {
        self.stack.len()
    }

    pub fn is_valid(&self) -> bool {
        self.current.is_some()
    }
}

impl Default for CurrentElement {
    fn default() -> Self {
        Self::new()
    }
}
