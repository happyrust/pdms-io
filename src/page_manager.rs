//! 页面缓存管理器模块
//!
//! 基于 IDA Pro 逆向分析 db1 模块实现的页面缓存机制。
//! 提供 LRU 缓存策略，减少磁盘 I/O 操作。
//!
//! # 设计参考
//! - db1_get_page: 检查缓存 -> 缓存命中返回 / 缓存未命中从磁盘读取
//! - db1_lock_page: 锁定页面防止被换出
//! - db1_update_page: 标记页面为脏页

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::fs::File;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::defines::{PAGE_SIZE, PAGE_SIZE_2K};

/// 缓存统计信息
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    /// 缓存命中次数
    pub hits: u64,
    /// 缓存未命中次数
    pub misses: u64,
    /// 页面读取次数
    pub reads: u64,
    /// 页面写入次数
    pub writes: u64,
}

impl CacheStats {
    /// 计算命中率
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// 缓存页面条目
#[derive(Debug, Clone)]
pub struct CachedPage {
    /// 页面数据
    pub data: Vec<u8>,
    /// 页面号
    pub page_no: u32,
    /// 数据库/扩展号 (用于多数据库支持)
    pub ext_no: u32,
    /// 锁计数 (>0 时页面不能被换出)
    pub lock_count: u32,
    /// 是否为脏页 (已修改但未写入磁盘)
    pub is_dirty: bool,
    /// 最后访问时间 (用于 LRU)
    pub last_access: u64,
}

impl CachedPage {
    /// 创建新的缓存页面
    pub fn new(page_no: u32, ext_no: u32, data: Vec<u8>) -> Self {
        Self {
            data,
            page_no,
            ext_no,
            lock_count: 0,
            is_dirty: false,
            last_access: 0,
        }
    }
    
    /// 是否被锁定
    pub fn is_locked(&self) -> bool {
        self.lock_count > 0
    }
}

/// 页面缓存键 (扩展号, 页面号)
pub type PageKey = (u32, u32);

/// 页面管理器
/// 
/// 基于 IDA Pro 分析的 db1 模块实现，提供页面级缓存功能。
#[derive(Debug)]
pub struct PageManager {
    /// 缓存映射表: (ext_no, page_no) -> CachedPage
    cache: HashMap<PageKey, CachedPage>,
    /// 最大缓存页面数
    max_pages: usize,
    /// 页面大小
    page_size: usize,
    /// 访问计数器 (用于 LRU)
    access_counter: AtomicU64,
    /// 缓存统计
    stats: CacheStats,
}

impl PageManager {
    /// 创建新的页面管理器
    /// 
    /// # 参数
    /// * `max_pages` - 最大缓存页面数，默认 256 (约 512KB 或 512MB 取决于页面大小)
    /// * `page_size` - 页面大小，默认 512 字节
    pub fn new(max_pages: usize, page_size: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_pages),
            max_pages,
            page_size,
            access_counter: AtomicU64::new(0),
            stats: CacheStats::default(),
        }
    }
    
    /// 使用默认配置创建页面管理器
    pub fn default_512() -> Self {
        Self::new(256, PAGE_SIZE)
    }
    
    /// 使用 2K 页面大小创建页面管理器
    pub fn default_2k() -> Self {
        Self::new(256, PAGE_SIZE_2K)
    }
    
    /// 获取缓存统计信息
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }
    
    /// 获取当前缓存页面数
    pub fn cached_count(&self) -> usize {
        self.cache.len()
    }
    
    /// 检查页面是否在缓存中
    pub fn is_page_cached(&self, ext_no: u32, page_no: u32) -> bool {
        self.cache.contains_key(&(ext_no, page_no))
    }
    
    /// 获取页面 (缓存优先)
    /// 
    /// 对应 IDA 分析的 db1_get_page 函数:
    /// 1. 检查缓存是否命中
    /// 2. 命中则更新访问时间并返回
    /// 3. 未命中则从文件读取，加入缓存后返回
    /// 
    /// # 参数
    /// * `file` - 数据库文件句柄
    /// * `ext_no` - 扩展号
    /// * `page_no` - 页面号
    pub fn get_page(
        &mut self,
        file: &mut File,
        ext_no: u32,
        page_no: u32,
    ) -> std::io::Result<&[u8]> {
        let key = (ext_no, page_no);
        
        // 检查缓存
        if self.cache.contains_key(&key) {
            // 缓存命中
            self.stats.hits += 1;
            let access_time = self.access_counter.fetch_add(1, Ordering::SeqCst);
            if let Some(page) = self.cache.get_mut(&key) {
                page.last_access = access_time;
            }
            return Ok(&self.cache.get(&key).unwrap().data);
        }
        
        // 缓存未命中
        self.stats.misses += 1;
        
        // 检查是否需要换出页面
        if self.cache.len() >= self.max_pages {
            self.evict_one();
        }
        
        // 从文件读取页面
        let data = self.read_page_from_file(file, page_no)?;
        self.stats.reads += 1;
        
        // 加入缓存
        let access_time = self.access_counter.fetch_add(1, Ordering::SeqCst);
        let mut page = CachedPage::new(page_no, ext_no, data);
        page.last_access = access_time;
        self.cache.insert(key, page);
        
        Ok(&self.cache.get(&key).unwrap().data)
    }
    
    /// 锁定页面 (防止被换出)
    /// 
    /// 对应 IDA 分析的 db1_lock_page 函数
    pub fn lock_page(&mut self, ext_no: u32, page_no: u32) -> bool {
        let key = (ext_no, page_no);
        if let Some(page) = self.cache.get_mut(&key) {
            page.lock_count += 1;
            true
        } else {
            false
        }
    }
    
    /// 解锁页面
    pub fn unlock_page(&mut self, ext_no: u32, page_no: u32) -> bool {
        let key = (ext_no, page_no);
        if let Some(page) = self.cache.get_mut(&key) {
            if page.lock_count > 0 {
                page.lock_count -= 1;
            }
            true
        } else {
            false
        }
    }
    
    /// 标记页面为脏页
    /// 
    /// 对应 IDA 分析的 db1_update_page 函数
    pub fn mark_dirty(&mut self, ext_no: u32, page_no: u32) -> bool {
        let key = (ext_no, page_no);
        if let Some(page) = self.cache.get_mut(&key) {
            page.is_dirty = true;
            true
        } else {
            false
        }
    }
    
    /// 使缓存中的页面失效
    pub fn invalidate(&mut self, ext_no: u32, page_no: u32) -> Option<CachedPage> {
        let key = (ext_no, page_no);
        self.cache.remove(&key)
    }
    
    /// 清空所有缓存
    pub fn clear(&mut self) {
        self.cache.clear();
        self.stats = CacheStats::default();
    }
    
    /// 从文件读取页面
    fn read_page_from_file(&self, file: &mut File, page_no: u32) -> std::io::Result<Vec<u8>> {
        let offset = page_no as u64 * self.page_size as u64;
        file.seek(SeekFrom::Start(offset))?;
        
        let mut data = vec![0u8; self.page_size];
        file.read_exact(&mut data)?;
        
        Ok(data)
    }
    
    /// 使用 LRU 策略换出一个页面
    fn evict_one(&mut self) {
        // 找到最久未访问且未锁定的页面
        let mut oldest_key: Option<PageKey> = None;
        let mut oldest_time = u64::MAX;
        
        for (key, page) in &self.cache {
            if !page.is_locked() && page.last_access < oldest_time {
                oldest_time = page.last_access;
                oldest_key = Some(*key);
            }
        }
        
        // 移除找到的页面
        if let Some(key) = oldest_key {
            // 注意: 如果是脏页，实际应用中应该先写回磁盘
            // 这里简化处理，只读模式下不需要写回
            self.cache.remove(&key);
        }
    }
}

impl Default for PageManager {
    fn default() -> Self {
        Self::default_512()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_stats() {
        let stats = CacheStats {
            hits: 80,
            misses: 20,
            reads: 20,
            writes: 0,
        };
        assert!((stats.hit_rate() - 0.8).abs() < 0.001);
    }
    
    #[test]
    fn test_page_manager_creation() {
        let pm = PageManager::new(128, 512);
        assert_eq!(pm.cached_count(), 0);
        assert_eq!(pm.max_pages, 128);
        assert_eq!(pm.page_size, 512);
    }
}
