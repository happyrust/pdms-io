use crate::engine_v2::db1::PageCache;
use crate::engine_v2::io_layer::FileHandle;
use crate::engine_v2::types::DbResult;

/// 数据库关闭 (对齐 db5_close_db)
pub struct DbClose;

impl DbClose {
    /// 关闭数据库: 同步缓存 + 释放句柄
    pub fn close(cache: &mut PageCache, handle: &mut FileHandle) -> DbResult<()> {
        cache.flush_all(handle)?;
        handle.sync()?;
        Ok(())
    }
}
