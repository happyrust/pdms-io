use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::engine_v2::db2::header::HeaderManager;
use crate::engine_v2::io_layer::{FileHandle, OpenMode};
use crate::engine_v2::types::*;

/// 数据库打开操作 (对齐 db5_open_read_db / db5_open_write_db)
pub struct DbOpen;

impl DbOpen {
    /// 只读打开 (对齐 db5_open_read_db, opcode 134, mode=7)
    pub fn open_read(path: impl AsRef<Path>) -> DbResult<(FileHandle, DbHeader)> {
        let path = path.as_ref();
        let mut handle = FileHandle::open(path, OpenMode::ReadOnly, PageSize::default())?;
        let header = HeaderManager::read(&mut handle)?;

        let actual_ps = Self::detect_page_size(path, &header)?;
        if actual_ps != PageSize::default() {
            handle = FileHandle::open(path, OpenMode::ReadOnly, actual_ps)?;
        }

        Ok((handle, header))
    }

    /// 读写打开 (对齐 db5_open_write_db, opcode 138, 独占锁)
    pub fn open_write(path: impl AsRef<Path>) -> DbResult<(FileHandle, DbHeader)> {
        let path = path.as_ref();
        let mut handle = FileHandle::open(path, OpenMode::ReadWrite, PageSize::default())?;
        let header = HeaderManager::read(&mut handle)?;

        let actual_ps = Self::detect_page_size(path, &header)?;
        if actual_ps != PageSize::default() {
            handle = FileHandle::open(path, OpenMode::ReadWrite, actual_ps)?;
        }

        Ok((handle, header))
    }

    /// 通过探测页面类型来识别真实 page_size
    ///
    /// 部分文件头 page_size 字段不可靠 (如 ams1112_0001 声明 512 实际 2048)。
    /// 用 session_page_no / latest_ses_pgno 探测：如果 pgno*page_size 处的
    /// page_type == 3 (Session)，则命中。
    fn detect_page_size(path: &Path, header: &DbHeader) -> DbResult<PageSize> {
        let mut file = std::fs::File::open(path)?;
        let file_len = file.metadata()?.len();

        let candidates = [PageSize::B2K, PageSize::B4K, PageSize::B512];
        let probe_pgnos: Vec<u32> = [header.session_page_no, header.latest_ses_pgno]
            .into_iter()
            .filter(|&p| p > 0)
            .collect();

        for ps in candidates {
            for &pgno in &probe_pgnos {
                let off = pgno as u64 * ps.bytes() as u64;
                if off + 4 > file_len {
                    continue;
                }
                file.seek(SeekFrom::Start(off))?;
                let mut buf = [0u8; 4];
                if file.read_exact(&mut buf).is_err() {
                    continue;
                }
                let page_type = i32::from_be_bytes(buf);
                if page_type == 3 {
                    return Ok(ps);
                }
            }
        }

        Ok(PageSize::default())
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
