use crate::engine_v2::types::*;

/// Extract 记录管理 (对齐 db2_insert_extract / db2_remove_extract)
///
/// 旧实现完全缺失。V2 实现 Extract 记录的插入和移除。
pub struct ExtractManager;

impl ExtractManager {
    /// 插入 Extract 记录
    pub fn insert(_data: &mut [u8], _refno: RefNo) -> DbResult<()> {
        // TODO: Phase 3 细化
        Ok(())
    }

    /// 移除 Extract 记录
    pub fn remove(_data: &mut [u8], _refno: RefNo) -> DbResult<()> {
        // TODO: Phase 3 细化
        Ok(())
    }
}
