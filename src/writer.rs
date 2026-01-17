//! 数据库写入模块
//!
//! 基于 IDA Pro 逆向分析实现的页面写入和元素序列化功能。
//!
//! # 设计参考
//! - db1_write_page: 写入单个页面到磁盘
//! - db5_save_work: 批量刷新所有脏页
//! - FHDBWN: Fortran 风格的底层写入封装

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};

use crate::defines::*;
use crate::page_manager::{PageManager, PageKey};

/// 写入操作错误类型
#[derive(Debug)]
pub enum WriteError {
    /// I/O 错误
    Io(std::io::Error),
    /// 页面大小不匹配
    PageSizeMismatch { expected: usize, actual: usize },
    /// 索引未找到
    IndexNotFound(u64),
    /// 序列化错误
    SerializationError(String),
}

impl From<std::io::Error> for WriteError {
    fn from(err: std::io::Error) -> Self {
        WriteError::Io(err)
    }
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::Io(e) => write!(f, "I/O 错误: {}", e),
            WriteError::PageSizeMismatch { expected, actual } => {
                write!(f, "页面大小不匹配: 期望 {} 字节, 实际 {} 字节", expected, actual)
            }
            WriteError::IndexNotFound(refno) => write!(f, "索引未找到: refno={}", refno),
            WriteError::SerializationError(msg) => write!(f, "序列化错误: {}", msg),
        }
    }
}

impl std::error::Error for WriteError {}

/// 写入统计
#[derive(Debug, Default, Clone)]
pub struct WriteStats {
    /// 已写入页面数
    pub pages_written: u64,
    /// 已写入字节数
    pub bytes_written: u64,
    /// 索引更新次数
    pub index_updates: u64,
}

/// 元素写入器
/// 
/// 提供元素级别的写入操作，封装页面管理和索引更新逻辑。
pub struct ElementWriter {
    /// 页面管理器
    page_manager: PageManager,
    /// 页面大小
    page_size: usize,
    /// 写入统计
    stats: WriteStats,
}

impl ElementWriter {
    /// 创建新的元素写入器
    pub fn new(page_size: usize) -> Self {
        let page_manager = PageManager::new(256, page_size);
        Self {
            page_manager,
            page_size,
            stats: WriteStats::default(),
        }
    }
    
    /// 使用 512 字节页面大小创建
    pub fn new_512() -> Self {
        Self::new(PAGE_SIZE_512)
    }
    
    /// 使用 2K 字节页面大小创建
    pub fn new_2k() -> Self {
        Self::new(PAGE_SIZE_2K)
    }
    
    /// 获取写入统计
    pub fn stats(&self) -> &WriteStats {
        &self.stats
    }
    
    /// 获取页面管理器的可变引用
    pub fn page_manager_mut(&mut self) -> &mut PageManager {
        &mut self.page_manager
    }
    
    /// 获取页面管理器的引用
    pub fn page_manager(&self) -> &PageManager {
        &self.page_manager
    }
    
    /// 写入单个页面
    /// 
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `ext_no` - 扩展号
    /// * `page_no` - 页面号
    /// * `data` - 页面数据
    pub fn write_page(
        &mut self,
        file: &mut File,
        ext_no: u32,
        page_no: u32,
        data: &[u8],
    ) -> Result<(), WriteError> {
        if data.len() != self.page_size {
            return Err(WriteError::PageSizeMismatch {
                expected: self.page_size,
                actual: data.len(),
            });
        }
        
        self.page_manager.write_page(file, ext_no, page_no, data)?;
        self.stats.pages_written += 1;
        self.stats.bytes_written += data.len() as u64;
        
        Ok(())
    }
    
    /// 刷新所有脏页到磁盘
    pub fn flush(&mut self, file: &mut File) -> Result<usize, WriteError> {
        let written = self.page_manager.flush_dirty_pages(file)?;
        self.stats.pages_written += written as u64;
        self.stats.bytes_written += (written * self.page_size) as u64;
        Ok(written)
    }
    
    /// 分配新页面
    /// 
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// 
    /// # 返回值
    /// 新分配的页面号
    pub fn allocate_page(&mut self, file: &mut File) -> Result<u32, WriteError> {
        // 获取文件末尾位置
        let file_size = file.seek(SeekFrom::End(0))?;
        
        // 计算新页面号
        let new_page_no = (file_size / self.page_size as u64) as u32;
        
        // 写入空页面
        let empty_page = vec![0u8; self.page_size];
        file.write_all(&empty_page)?;
        file.flush()?;
        
        self.stats.pages_written += 1;
        self.stats.bytes_written += self.page_size as u64;
        
        Ok(new_page_no)
    }
    
    /// 更新参考号索引位置
    /// 
    /// 更新 B+ 树索引中的参考号位置信息
    /// 
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `ext_no` - 扩展号
    /// * `index_page_no` - 索引页面号
    /// * `refno_loc` - 新的参考号位置信息
    pub fn update_index_entry(
        &mut self,
        file: &mut File,
        ext_no: u32,
        index_page_no: u32,
        refno_loc: &RefnoDataLoc,
    ) -> Result<(), WriteError> {
        // 读取索引页面
        let page_data = self.page_manager.get_page(file, ext_no, index_page_no)
            .map_err(WriteError::Io)?
            .to_vec();
        
        // 验证页面类型
        if page_data.len() < 8 {
            return Err(WriteError::SerializationError("页面数据太短".into()));
        }
        
        let page_type = u32::from_be_bytes([page_data[0], page_data[1], page_data[2], page_data[3]]);
        if page_type != 5 && page_type != 8 {
            return Err(WriteError::SerializationError(
                format!("非索引页面类型: {}", page_type)
            ));
        }
        
        // 序列化新的 RefnoDataLoc
        let target_refno_bytes = [
            (refno_loc.refno_0 >> 24) as u8, (refno_loc.refno_0 >> 16) as u8,
            (refno_loc.refno_0 >> 8) as u8, refno_loc.refno_0 as u8,
            (refno_loc.refno_1 >> 24) as u8, (refno_loc.refno_1 >> 16) as u8,
            (refno_loc.refno_1 >> 8) as u8, refno_loc.refno_1 as u8,
        ];
        
        // 在页面中查找目标参考号
        let mut found_offset = None;
        for offset in (0x24..page_data.len()).step_by(16) {
            if offset + 8 <= page_data.len() && &page_data[offset..offset+8] == &target_refno_bytes {
                found_offset = Some(offset);
                break;
            }
        }
        
        if let Some(offset) = found_offset {
            // 更新页面数据
            let mut new_page_data = page_data.clone();
            
            // 写入新的 pgno 和 offset
            let pgno_bytes = refno_loc.pgno.to_be_bytes();
            new_page_data[offset + 8..offset + 12].copy_from_slice(&pgno_bytes);
            
            // offset 和 flag 打包为 4 字节
            let packed = ((refno_loc.offset as u32) << 12) | (refno_loc.flag as u32);
            let packed_bytes = packed.to_be_bytes();
            new_page_data[offset + 12..offset + 16].copy_from_slice(&packed_bytes);
            
            // 写回页面
            self.page_manager.write_page(file, ext_no, index_page_no, &new_page_data)?;
            self.stats.index_updates += 1;
            
            Ok(())
        } else {
            Err(WriteError::IndexNotFound(
                ((refno_loc.refno_0 as u64) << 32) | (refno_loc.refno_1 as u64)
            ))
        }
    }

    /// 插入新的索引项到叶子页面
    ///
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `ext_no` - 扩展号
    /// * `index_page_no` - 索引叶子页号
    /// * `refno_loc` - 参考号位置信息
    ///
    /// # 返回值
    /// * `Ok(())` - 插入成功
    /// * `Err(WriteError)` - 页面已满或其他错误
    pub fn insert_index_entry(
        &mut self,
        file: &mut File,
        ext_no: u32,
        index_page_no: u32,
        refno_loc: &RefnoDataLoc,
    ) -> Result<(), WriteError> {
        // 读取索引页面
        let page_data = self.page_manager.get_page(file, ext_no, index_page_no)
            .map_err(WriteError::Io)?
            .to_vec();
        
        if page_data.len() < 0x24 {
            return Err(WriteError::SerializationError("索引页面数据太短".into()));
        }
        
        // 验证页面类型 (5 = 数据页, 8 = 索引页)
        let page_type = u32::from_be_bytes([page_data[0], page_data[1], page_data[2], page_data[3]]);
        if page_type != 5 && page_type != 8 {
            return Err(WriteError::SerializationError(
                format!("非索引页面类型: {}", page_type)
            ));
        }
        
        // 查找插入位置 (第一个全零的 16 字节槽位)
        let mut insert_offset = None;
        for offset in (0x24..page_data.len()).step_by(16) {
            if offset + 16 <= page_data.len() {
                // 检查是否为空槽位
                let is_empty = page_data[offset..offset + 16].iter().all(|&b| b == 0);
                if is_empty {
                    insert_offset = Some(offset);
                    break;
                }
            }
        }
        
        if let Some(offset) = insert_offset {
            let mut new_page_data = page_data.clone();
            
            // 写入 refno_0 (4B)
            new_page_data[offset..offset + 4].copy_from_slice(&refno_loc.refno_0.to_be_bytes());
            
            // 写入 refno_1 (4B)
            new_page_data[offset + 4..offset + 8].copy_from_slice(&refno_loc.refno_1.to_be_bytes());
            
            // 写入 pgno (4B)
            new_page_data[offset + 8..offset + 12].copy_from_slice(&refno_loc.pgno.to_be_bytes());
            
            // 写入 offset(20bit) + flag(12bit) 打包为 4B
            let packed = ((refno_loc.offset as u32) << 12) | (refno_loc.flag as u32);
            new_page_data[offset + 12..offset + 16].copy_from_slice(&packed.to_be_bytes());
            
            // 写回页面
            self.page_manager.write_page(file, ext_no, index_page_no, &new_page_data)?;
            self.stats.index_updates += 1;
            
            Ok(())
        } else {
            // 页面已满，需要分裂
            Err(WriteError::SerializationError("索引页面已满，需要分裂（当前不支持）".into()))
        }
    }

    /// 创建 RefnoDataLoc 结构
    ///
    /// # 参数
    /// * `refno` - 完整的 64 位参考号
    /// * `page_no` - 数据页号
    /// * `offset` - 页内偏移（以 2 字节为单位，最大 20 位）
    /// * `flag` - 标志位（最大 12 位，通常为 0）
    pub fn create_refno_loc(refno: u64, page_no: u32, offset: u32, flag: u16) -> RefnoDataLoc {
        RefnoDataLoc {
            refno_0: (refno >> 32) as u32,
            refno_1: refno as u32,
            pgno: page_no,
            offset,
            flag,
        }
    }
}

impl Default for ElementWriter {
    fn default() -> Self {
        Self::new_512()
    }
}

// ==================================================================================
// 数据页写入层 (Data Page Writer)
// ==================================================================================

/// 数据页类型常量
pub const DATA_PAGE_TYPE: u32 = 5;
/// 主数据页子类型
pub const MAIN_DATA_SUBTYPE: u32 = 0x00743F49;

/// 数据页位置信息
#[derive(Debug, Clone, Default)]
pub struct DataPageLocation {
    /// 扩展号
    pub ext_no: u32,
    /// 页号
    pub page_no: u32,
    /// 页内偏移（以字节为单位）
    pub offset: usize,
}

impl DataPageLocation {
    /// 创建新的位置信息
    pub fn new(ext_no: u32, page_no: u32, offset: usize) -> Self {
        Self { ext_no, page_no, offset }
    }
}

/// 数据页写入器
///
/// 管理数据页的分配和元素写入
#[derive(Debug)]
pub struct DataPageWriter {
    /// 当前数据页缓冲区
    current_page: Vec<u8>,
    /// 当前页内偏移
    current_offset: usize,
    /// 当前页号
    current_page_no: u32,
    /// 扩展号
    ext_no: u32,
    /// 页面大小
    page_size: usize,
    /// 已写入的页面列表
    written_pages: Vec<(u32, Vec<u8>)>,
    /// 写入统计
    elements_written: u64,
    bytes_written: u64,
}

impl DataPageWriter {
    /// 创建新的数据页写入器
    ///
    /// # 参数
    /// * `page_size` - 页面大小
    /// * `start_page_no` - 起始页号
    /// * `ext_no` - 扩展号
    pub fn new(page_size: usize, start_page_no: u32, ext_no: u32) -> Self {
        let mut current_page = vec![0u8; page_size];
        
        // 初始化数据页头部
        // 页面类型 (4B) + 子类型 (4B) = 8B
        current_page[0..4].copy_from_slice(&DATA_PAGE_TYPE.to_be_bytes());
        current_page[4..8].copy_from_slice(&MAIN_DATA_SUBTYPE.to_be_bytes());
        
        Self {
            current_page,
            current_offset: 8, // 跳过页面头部
            current_page_no: start_page_no,
            ext_no,
            page_size,
            written_pages: Vec::new(),
            elements_written: 0,
            bytes_written: 0,
        }
    }

    /// 使用 512 字节页面创建
    pub fn new_512(start_page_no: u32) -> Self {
        Self::new(PAGE_SIZE_512, start_page_no, 1)
    }

    /// 使用 2K 字节页面创建
    pub fn new_2k(start_page_no: u32) -> Self {
        Self::new(PAGE_SIZE_2K, start_page_no, 1)
    }

    /// 获取当前页剩余空间
    pub fn remaining_space(&self) -> usize {
        self.page_size.saturating_sub(self.current_offset)
    }

    /// 获取当前位置信息
    pub fn current_location(&self) -> DataPageLocation {
        DataPageLocation::new(self.ext_no, self.current_page_no, self.current_offset)
    }

    /// 检查是否有足够空间写入指定大小的数据
    pub fn has_space_for(&self, size: usize) -> bool {
        self.remaining_space() >= size
    }

    /// 刷新当前页到已写入列表，并开始新页
    fn flush_current_page(&mut self) {
        // 保存当前页
        self.written_pages.push((self.current_page_no, self.current_page.clone()));
        
        // 创建新页
        self.current_page_no += 1;
        self.current_page = vec![0u8; self.page_size];
        
        // 初始化新页头部
        self.current_page[0..4].copy_from_slice(&DATA_PAGE_TYPE.to_be_bytes());
        self.current_page[4..8].copy_from_slice(&MAIN_DATA_SUBTYPE.to_be_bytes());
        self.current_offset = 8;
    }

    /// 写入原始数据到当前页
    ///
    /// 如果当前页空间不足，会自动刷新并创建新页
    ///
    /// # 返回值
    /// 返回写入位置信息
    pub fn write_data(&mut self, data: &[u8]) -> DataPageLocation {
        // 如果数据大小超过单页最大容量
        if data.len() > self.page_size - 8 {
            // TODO: 处理跨页元素
            panic!("元素数据超过单页容量，当前不支持跨页写入");
        }

        // 检查是否需要刷新页面
        if !self.has_space_for(data.len()) {
            self.flush_current_page();
        }

        // 记录写入位置
        let location = self.current_location();

        // 写入数据
        self.current_page[self.current_offset..self.current_offset + data.len()]
            .copy_from_slice(data);
        self.current_offset += data.len();
        self.bytes_written += data.len() as u64;

        location
    }

    /// 写入元素数据并返回位置信息
    ///
    /// # 参数
    /// * `element_data` - 已序列化的元素数据
    pub fn write_element(&mut self, element_data: &[u8]) -> DataPageLocation {
        let location = self.write_data(element_data);
        self.elements_written += 1;
        location
    }

    /// 完成写入，返回所有已写入的页面
    ///
    /// 包括当前未满的页面
    pub fn finish(mut self) -> Vec<(u32, Vec<u8>)> {
        // 如果当前页有数据，也要包含
        if self.current_offset > 8 {
            self.written_pages.push((self.current_page_no, self.current_page));
        }
        self.written_pages
    }

    /// 获取统计信息
    pub fn stats(&self) -> (u64, u64) {
        (self.elements_written, self.bytes_written)
    }

    /// 获取当前页号
    pub fn current_page_no(&self) -> u32 {
        self.current_page_no
    }
}

// ==================================================================================
// 会话写入层 (Session Writer)
// ==================================================================================

/// 会话页面构建器
/// 
/// 用于创建新的会话页面数据，对应 db5_save_work 中的会话创建逻辑
#[derive(Debug, Clone)]
pub struct SessionBuilder {
    /// 会话号
    pub sesno: u32,
    /// 上一个会话页号
    pub last_ses_pageno: u32,
    /// 上一个会话扩展号
    pub last_ses_extno: u32,
    /// 会话结束页号
    pub end_pgno: u32,
    /// 会话结束扩展号
    pub end_extno: u32,
    /// 索引根页号
    pub index_root_pageno: u32,
    /// 索引根扩展号
    pub index_root_extno: u32,
    /// 声明页号
    pub claim_pageno: u32,
    /// 声明扩展号
    pub claim_extno: u32,
    /// 未知数据1
    pub unknown_1: i32,
    /// 未知数据2
    pub unknown_2: i32,
    /// 计算机名
    pub computer_name: String,
    /// 注释
    pub comments: String,
}

impl SessionBuilder {
    /// 创建新的会话构建器
    pub fn new(sesno: u32, last_ses_pageno: u32) -> Self {
        Self {
            sesno,
            last_ses_pageno,
            last_ses_extno: 1,
            end_pgno: 0,
            end_extno: 1,
            index_root_pageno: 0,
            index_root_extno: 1,
            claim_pageno: 0,
            claim_extno: 1,
            unknown_1: 0,
            unknown_2: 0,
            computer_name: String::new(),
            comments: String::new(),
        }
    }
    
    /// 设置结束页号
    pub fn end_pgno(mut self, pgno: u32) -> Self {
        self.end_pgno = pgno;
        self
    }
    
    /// 设置索引根页号
    pub fn index_root(mut self, pgno: u32) -> Self {
        self.index_root_pageno = pgno;
        self
    }

    /// 设置声明页号
    pub fn claim_root(mut self, pgno: u32) -> Self {
        self.claim_pageno = pgno;
        self
    }
    
    /// 设置计算机名
    pub fn computer_name(mut self, name: impl Into<String>) -> Self {
        self.computer_name = name.into();
        self
    }
    
    /// 设置注释
    pub fn comments(mut self, comments: impl Into<String>) -> Self {
        self.comments = comments.into();
        self
    }
    
    /// 构建会话页面二进制数据
    /// 
    /// # 参数
    /// * `page_size` - 页面大小
    /// 
    /// # 返回值
    /// * 会话页面的二进制数据
    pub fn build(&self, page_size: usize) -> Vec<u8> {
        let mut data = vec![0u8; page_size];
        
        // 写入页面类型 (0x00000003 = 会话页)
        data[0..4].copy_from_slice(&3i32.to_be_bytes());
        
        // 写入上一个会话页号
        data[4..8].copy_from_slice(&(self.last_ses_pageno as i32).to_be_bytes());
        
        // 写入上一个会话扩展号 (通常为 1)
        data[8..12].copy_from_slice(&(self.last_ses_extno as i32).to_be_bytes());
        
        // 写入会话号
        data[12..16].copy_from_slice(&(self.sesno as i32).to_be_bytes());
        
        // 写入 unknown_0 (0xFFFFFFFF)
        data[16..20].copy_from_slice(&(-1i32).to_be_bytes());
        
        // 写入结束页号
        data[20..24].copy_from_slice(&self.end_pgno.to_be_bytes());
        
        // 写入结束扩展号 (通常为 1)
        data[24..28].copy_from_slice(&self.end_extno.to_be_bytes());
        
        // 写入索引根页号
        data[28..32].copy_from_slice(&self.index_root_pageno.to_be_bytes());
        
        // 写入索引根扩展号 (通常为 1)
        data[32..36].copy_from_slice(&self.index_root_extno.to_be_bytes());

        // 写入声明页号与扩展号
        data[0x24..0x28].copy_from_slice(&self.claim_pageno.to_be_bytes());
        data[0x28..0x2C].copy_from_slice(&self.claim_extno.to_be_bytes());

        // 写入未知字段
        data[0x2C..0x30].copy_from_slice(&self.unknown_1.to_be_bytes());
        data[0x30..0x34].copy_from_slice(&self.unknown_2.to_be_bytes());
        
        // 写入时间戳 (年、月、小时、秒)
        let now = chrono::Local::now();
        let year = now.year() as u32;
        let month = now.month() as u32;
        let hours = now.day() * 24 + now.hour();
        let seconds = now.minute() * 60 + now.second();
        
        data[0x34..0x38].copy_from_slice(&year.to_be_bytes());
        data[0x38..0x3C].copy_from_slice(&month.to_be_bytes());
        data[0x3C..0x40].copy_from_slice(&hours.to_be_bytes());
        data[0x40..0x44].copy_from_slice(&seconds.to_be_bytes());
        
        // 写入计算机名长度和内容
        let name_bytes = self.computer_name.as_bytes();
        let name_words = ((name_bytes.len() + 3) / 4).min(9); // 以 4 字节为单位，最多 9 words
        data[0x78..0x7C].copy_from_slice(&(name_words as u32).to_be_bytes());

        // 写入计算机名
        let name_start = 0x7C;
        let name_len = std::cmp::min(name_bytes.len(), name_words * 4);
        if name_len > 0 {
            data[name_start..name_start + name_len].copy_from_slice(&name_bytes[..name_len]);
        }
        
        // 写入注释长度和内容
        let comments_start = 0x7C + 36; // 名称固定 36 字节
        let comments_bytes = self.comments.as_bytes();
        let comments_words = ((comments_bytes.len() + 3) / 4).min(1024);
        if comments_start + 4 <= page_size {
            data[comments_start..comments_start + 4].copy_from_slice(&(comments_words as u32).to_be_bytes());
            let max_payload = page_size - comments_start - 4;
            let comments_len = std::cmp::min(comments_words * 4, max_payload);
            let copy_len = std::cmp::min(comments_bytes.len(), comments_len);
            if copy_len > 0 {
                data[comments_start + 4..comments_start + 4 + copy_len]
                    .copy_from_slice(&comments_bytes[..copy_len]);
            }
        }
        
        data
    }
}

/// 数据库头更新器
/// 
/// 用于更新 PDMS 数据库头部信息
pub struct HeaderUpdater;

impl HeaderUpdater {
    /// 更新数据库头的最新会话页号
    /// 
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `new_ses_pgno` - 新的会话页号
    pub fn update_latest_ses_pgno(file: &mut File, new_ses_pgno: u32) -> std::io::Result<()> {
        // 会话页号位于 offset 0x28 (40 字节)
        file.seek(SeekFrom::Start(0x28))?;
        file.write_all(&new_ses_pgno.to_be_bytes())?;
        file.flush()?;
        Ok(())
    }
    
    /// 更新数据库头的存储页数
    /// 
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `page_count` - 新的页数
    pub fn update_stored_page_count(file: &mut File, page_count: u32) -> std::io::Result<()> {
        // 页数位于 offset 0x38 (56 字节)
        file.seek(SeekFrom::Start(0x38))?;
        file.write_all(&page_count.to_be_bytes())?;
        file.flush()?;
        Ok(())
    }
    
    /// 批量更新数据库头
    /// 
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `ses_pgno` - 新的会话页号
    /// * `page_count` - 新的页数
    pub fn update_header(
        file: &mut File,
        ses_pgno: u32,
        page_count: u32,
    ) -> std::io::Result<()> {
        Self::update_latest_ses_pgno(file, ses_pgno)?;
        Self::update_stored_page_count(file, page_count)?;
        Ok(())
    }
}

use chrono::{Datelike, Timelike};

/// 完整的数据库写入器
/// 
/// 组合 ElementWriter 和会话管理功能，提供高层 API
pub struct DatabaseWriter {
    /// 元素写入器
    pub element_writer: ElementWriter,
    /// 当前会话号
    pub current_sesno: Option<u32>,
    /// 页面大小
    pub page_size: usize,
}

impl DatabaseWriter {
    /// 创建新的数据库写入器
    pub fn new(page_size: usize) -> Self {
        Self {
            element_writer: ElementWriter::new(page_size),
            current_sesno: None,
            page_size,
        }
    }
    
    /// 使用 512 字节页面创建
    pub fn new_512() -> Self {
        Self::new(PAGE_SIZE_512)
    }
    
    /// 使用 2K 字节页面创建
    pub fn new_2k() -> Self {
        Self::new(PAGE_SIZE_2K)
    }
    
    /// 开始新会话
    /// 
    /// # 参数
    /// * `sesno` - 会话号
    pub fn begin_session(&mut self, sesno: u32) {
        self.current_sesno = Some(sesno);
    }
    
    /// 提交当前会话
    /// 
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `last_ses_pgno` - 上一个会话页号
    /// * `computer_name` - 计算机名（可选）
    /// * `comments` - 注释（可选）
    /// 
    /// # 返回值
    /// * 新会话页的页号
    pub fn commit_session(
        &mut self,
        file: &mut File,
        last_ses_pgno: u32,
        computer_name: Option<&str>,
        comments: Option<&str>,
    ) -> Result<u32, WriteError> {
        let sesno = self.current_sesno
            .ok_or_else(|| WriteError::SerializationError("未开始会话".into()))?;
        
        // 1. 刷新所有脏页
        let written = self.element_writer.flush(file)?;
        
        // 2. 分配会话页面
        let new_ses_pgno = self.element_writer.allocate_page(file)?;
        
        // 3. 构建会话页面数据
        let session = SessionBuilder::new(sesno, last_ses_pgno)
            .end_pgno(new_ses_pgno)
            .index_root(0) // TODO: 实际应该从索引中获取
            .computer_name(computer_name.unwrap_or("PDMS-IO"))
            .comments(comments.unwrap_or(""))
            .build(self.page_size);
        
        // 4. 写入会话页面
        self.element_writer.write_page(file, 0, new_ses_pgno, &session)?;
        
        // 5. 更新数据库头
        let file_size = file.seek(SeekFrom::End(0))?;
        let page_count = (file_size / self.page_size as u64) as u32;
        HeaderUpdater::update_header(file, new_ses_pgno, page_count)?;
        
        // 6. 清除会话状态
        self.current_sesno = None;
        
        Ok(new_ses_pgno)
    }
    
    /// 是否有活跃会话
    pub fn has_active_session(&self) -> bool {
        self.current_sesno.is_some()
    }
    
    /// 获取写入统计
    pub fn stats(&self) -> &WriteStats {
        self.element_writer.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::Read;
    
    #[test]
    fn test_element_writer_creation() {
        let writer = ElementWriter::new_512();
        assert_eq!(writer.page_size, 512);
        assert_eq!(writer.stats.pages_written, 0);
    }
    
    #[test]
    fn test_write_page() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_writer_test.bin");
        let _ = std::fs::remove_file(&temp_file);
        
        // 初始化文件
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");
            
            let empty_data = vec![0u8; 512 * 4];
            file.write_all(&empty_data).expect("无法初始化文件");
        }
        
        let mut writer = ElementWriter::new_512();
        let test_data: Vec<u8> = (0..512).map(|i| (i % 256) as u8).collect();
        
        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");
            
            writer.write_page(&mut file, 0, 1, &test_data).expect("写入失败");
        }
        
        // 验证
        {
            let mut file = std::fs::File::open(&temp_file).expect("无法打开文件");
            file.seek(SeekFrom::Start(512)).expect("无法定位");
            
            let mut read_data = vec![0u8; 512];
            file.read_exact(&mut read_data).expect("无法读取");
            
            assert_eq!(read_data, test_data);
        }
        
        assert_eq!(writer.stats.pages_written, 1);
        assert_eq!(writer.stats.bytes_written, 512);
        
        let _ = std::fs::remove_file(&temp_file);
    }
    
    #[test]
    fn test_allocate_page() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_alloc_test.bin");
        let _ = std::fs::remove_file(&temp_file);
        
        // 创建初始文件 (2 页)
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");
            
            let data = vec![0u8; 512 * 2];
            file.write_all(&data).expect("无法初始化文件");
        }
        
        let mut writer = ElementWriter::new_512();
        
        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");
            
            let new_page = writer.allocate_page(&mut file).expect("分配失败");
            assert_eq!(new_page, 2); // 第三个页面 (0, 1, 2)
        }
        
        // 验证文件大小增加
        let file_size = std::fs::metadata(&temp_file).expect("无法获取元数据").len();
        assert_eq!(file_size, 512 * 3);
        
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_data_page_writer_creation() {
        let writer = DataPageWriter::new_512(10);
        assert_eq!(writer.current_page_no(), 10);
        assert_eq!(writer.remaining_space(), 512 - 8); // 页面大小减去头部
    }

    #[test]
    fn test_data_page_writer_write_element() {
        let mut writer = DataPageWriter::new_512(10);
        
        // 写入一个 100 字节的元素
        let element_data = vec![0xABu8; 100];
        let location = writer.write_element(&element_data);
        
        assert_eq!(location.page_no, 10);
        assert_eq!(location.offset, 8); // 头部后的第一个位置
        
        let (elements, bytes) = writer.stats();
        assert_eq!(elements, 1);
        assert_eq!(bytes, 100);
    }

    #[test]
    fn test_data_page_writer_auto_flush() {
        let mut writer = DataPageWriter::new_512(10);
        
        // 写入多个元素直到需要刷新页面
        // 每个元素 100 字节，页面可用空间 504 字节，可容纳 5 个
        for i in 0..6 {
            let element_data = vec![i as u8; 100];
            let location = writer.write_element(&element_data);
            
            if i < 5 {
                assert_eq!(location.page_no, 10);
            } else {
                // 第 6 个元素应该在新页面
                assert_eq!(location.page_no, 11);
            }
        }
    }

    #[test]
    fn test_data_page_writer_finish() {
        let mut writer = DataPageWriter::new_512(10);
        
        // 写入一个元素
        let element_data = vec![0xABu8; 50];
        writer.write_element(&element_data);
        
        // 完成
        let pages = writer.finish();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].0, 10); // 页号
        assert_eq!(pages[0].1.len(), 512); // 完整页面大小
    }
}

