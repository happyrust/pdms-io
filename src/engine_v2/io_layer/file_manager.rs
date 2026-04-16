use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::engine_v2::types::{DbError, DbResult, PageSize};

/// 数据库文件打开模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    ReadOnly,
    ReadWrite,
}

/// 数据库文件句柄管理器
///
/// 替代 core.dll 的 FIOXST/FIONEW + DirectAccessToken 机制。
/// 使用 Rust std::fs::File，提供页面对齐的直接读写。
#[derive(Debug)]
pub struct FileHandle {
    file: File,
    path: PathBuf,
    mode: OpenMode,
    page_size: usize,
    file_len: u64,
}

impl FileHandle {
    /// 打开已有数据库文件 (替代 FIOXST)
    pub fn open(path: impl AsRef<Path>, mode: OpenMode, page_size: PageSize) -> DbResult<Self> {
        let path = path.as_ref().to_path_buf();
        let file = match mode {
            OpenMode::ReadOnly => File::open(&path)?,
            OpenMode::ReadWrite => OpenOptions::new().read(true).write(true).open(&path)?,
        };
        let file_len = file.metadata()?.len();
        Ok(Self {
            file,
            path,
            mode,
            page_size: page_size.bytes(),
            file_len,
        })
    }

    /// 创建新数据库文件 (替代 FIONEW)
    pub fn create(path: impl AsRef<Path>, page_size: PageSize) -> DbResult<Self> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true).write(true).create(true).truncate(true)
            .open(&path)?;
        Ok(Self {
            file,
            path,
            mode: OpenMode::ReadWrite,
            page_size: page_size.bytes(),
            file_len: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn mode(&self) -> OpenMode {
        self.mode
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    pub fn file_len(&self) -> u64 {
        self.file_len
    }

    pub fn total_pages(&self) -> u32 {
        (self.file_len / self.page_size as u64) as u32
    }

    /// 刷新文件长度缓存
    pub fn refresh_len(&mut self) -> DbResult<u64> {
        self.file_len = self.file.metadata()?.len();
        Ok(self.file_len)
    }

    /// 读取单个页面 (替代 FHDBRN → DirectAccessToken::read)
    pub fn read_page(&mut self, page_no: u32, buf: &mut [u8]) -> DbResult<()> {
        let offset = page_no as u64 * self.page_size as u64;
        if offset + self.page_size as u64 > self.file_len {
            return Err(DbError::PageOutOfRange {
                page_no,
                file_pages: self.total_pages(),
            });
        }
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(buf)?;
        Ok(())
    }

    /// 批量读取连续页面 (预读优化)
    pub fn read_pages(&mut self, start_page: u32, count: u32, buf: &mut [u8]) -> DbResult<u32> {
        let max_pages = self.total_pages();
        let actual = count.min(max_pages.saturating_sub(start_page));
        if actual == 0 {
            return Ok(0);
        }
        let offset = start_page as u64 * self.page_size as u64;
        let len = actual as usize * self.page_size;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut buf[..len])?;
        Ok(actual)
    }

    /// 写入单个页面 (替代 FHDBWN → DirectAccessToken::write)
    pub fn write_page(&mut self, page_no: u32, data: &[u8]) -> DbResult<()> {
        if self.mode == OpenMode::ReadOnly {
            return Err(DbError::Other("write on read-only handle".into()));
        }
        let offset = page_no as u64 * self.page_size as u64;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(data)?;
        let new_end = offset + data.len() as u64;
        if new_end > self.file_len {
            self.file_len = new_end;
        }
        Ok(())
    }

    /// 同步到磁盘
    pub fn sync(&self) -> DbResult<()> {
        self.file.sync_all()?;
        Ok(())
    }

    /// 重新打开文件 (用于重试机制，替代 FHSWIT 模式切换)
    pub fn reopen(&mut self) -> DbResult<()> {
        let new_file = match self.mode {
            OpenMode::ReadOnly => File::open(&self.path)?,
            OpenMode::ReadWrite => OpenOptions::new().read(true).write(true).open(&self.path)?,
        };
        self.file = new_file;
        self.refresh_len()?;
        Ok(())
    }

    pub fn into_inner(self) -> File {
        self.file
    }
}
