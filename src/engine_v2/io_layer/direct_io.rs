use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use crate::engine_v2::types::{DbResult, PageSize};

/// 页面对齐直接读取器
///
/// 提供按页面大小对齐的连续/随机读取能力。
/// 替代 core.dll 的 DirectAccessToken vtable 读写接口。
pub struct DirectReader {
    page_size: usize,
}

impl DirectReader {
    pub fn new(page_size: PageSize) -> Self {
        Self {
            page_size: page_size.bytes(),
        }
    }

    /// 从文件指定偏移读取 n 字节
    pub fn read_at(file: &mut File, offset: u64, buf: &mut [u8]) -> DbResult<()> {
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(buf)?;
        Ok(())
    }

    /// 读取单页到新 Vec
    pub fn read_page_vec(&self, file: &mut File, page_no: u32) -> DbResult<Vec<u8>> {
        let mut buf = vec![0u8; self.page_size];
        let offset = page_no as u64 * self.page_size as u64;
        Self::read_at(file, offset, &mut buf)?;
        Ok(buf)
    }

    /// 读取跨页连续数据 (用于元素续页拼接)
    pub fn read_span(&self, file: &mut File, start_offset: u64, len: usize) -> DbResult<Vec<u8>> {
        let mut buf = vec![0u8; len];
        Self::read_at(file, start_offset, &mut buf)?;
        Ok(buf)
    }
}
