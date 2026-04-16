use std::path::{Path, PathBuf};

use crate::engine_v2::db1::PageCache;
use crate::engine_v2::db2::header::HeaderManager;
use crate::engine_v2::io_layer::{FileHandle, OpenMode};
use crate::engine_v2::types::*;

/// 数据库打开操作 (对齐 db5_open_read_db / db5_open_write_db)
pub struct DbOpen;

impl DbOpen {
    /// 只读打开 (对齐 db5_open_read_db, opcode 134, mode=7)
    pub fn open_read(path: impl AsRef<Path>) -> DbResult<(FileHandle, DbHeader)> {
        let mut handle = FileHandle::open(path, OpenMode::ReadOnly, PageSize::default())?;
        let header = HeaderManager::read(&mut handle)?;

        let actual_ps = PageSize::from_raw(header.page_size);
        if actual_ps != PageSize::default() {
            handle = FileHandle::open(handle.path(), OpenMode::ReadOnly, actual_ps)?;
        }

        Ok((handle, header))
    }

    /// 读写打开 (对齐 db5_open_write_db, opcode 138, 独占锁)
    pub fn open_write(path: impl AsRef<Path>) -> DbResult<(FileHandle, DbHeader)> {
        let mut handle = FileHandle::open(path, OpenMode::ReadWrite, PageSize::default())?;
        let header = HeaderManager::read(&mut handle)?;

        let actual_ps = PageSize::from_raw(header.page_size);
        if actual_ps != PageSize::default() {
            handle = FileHandle::open(handle.path(), OpenMode::ReadWrite, actual_ps)?;
        }

        Ok((handle, header))
    }

    /// 构造数据库文件路径
    ///
    /// 格式: `<project_dir>/<project_name>NNN` (3位补零)
    /// 对齐 DB_DB::appendDBFileName
    pub fn make_db_path(project_dir: &Path, project_name: &str, dbno: u32) -> PathBuf {
        let filename = format!("{}{:03}", project_name, dbno);
        project_dir.join(filename)
    }
}
