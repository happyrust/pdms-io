use crate::engine_v2::db1::PageCache;
use crate::engine_v2::db2::header::HeaderManager;
use crate::engine_v2::io_layer::FileHandle;
use crate::engine_v2::types::*;

/// 数据库保存 (对齐 db5_save_work)
///
/// 保存流程: 脏页 flush → 索引更新 → 会话页写入 → 头部更新
pub struct DbSave;

impl DbSave {
    /// 完整保存 (对齐 db5_save_work)
    pub fn save_work(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        header: &mut DbHeader,
    ) -> DbResult<u32> {
        let dirty_count = cache.flush_all(handle)?;

        handle.refresh_len()?;
        header.stored_page_count = handle.total_pages();
        HeaderManager::write(handle, header)?;

        handle.sync()?;

        Ok(dirty_count)
    }
}
