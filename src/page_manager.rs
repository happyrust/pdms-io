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
use std::io::{Read, Seek, SeekFrom, Write};
use std::fs::File;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::defines::{PAGE_SIZE_2K, PAGE_SIZE_512};

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
    pub(crate) cache: HashMap<PageKey, CachedPage>,
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
    /// * `page_size` - 页面大小
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
        Self::new(256, PAGE_SIZE_512)
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
    
    /// 直接插入页面到缓存（用于测试或预加载）
    /// 
    /// # 参数
    /// * `page` - 要插入的缓存页面
    pub fn insert_page(&mut self, page: CachedPage) {
        let key = (page.ext_no, page.page_no);
        self.cache.insert(key, page);
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
    /// 
    /// 注意：如果页面是脏页，调用者应该先调用 `flush_dirty_pages` 确保数据已写入
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
    
    // ==================================================================================
    // 写入功能 (基于 IDA Pro 逆向分析 db1_write_page 实现)
    // ==================================================================================
    
    /// 写入页面到文件 (对应 db1_write_page_basic)
    /// 
    /// # 参数
    /// * `file` - 数据库文件句柄（需可写）
    /// * `ext_no` - 扩展号
    /// * `page_no` - 页面号
    /// * `data` - 页面数据（必须等于 page_size）
    /// 
    /// # 返回值
    /// * `Ok(())` - 写入成功
    /// * `Err` - I/O 错误
    pub fn write_page(
        &mut self,
        file: &mut File,
        ext_no: u32,
        page_no: u32,
        data: &[u8],
    ) -> std::io::Result<()> {
        // 验证数据大小
        assert_eq!(data.len(), self.page_size, "页面数据大小必须等于 page_size");
        
        // 写入到文件
        self.write_page_to_file(file, page_no, data)?;
        self.stats.writes += 1;
        
        // 更新缓存（如果存在）
        let key = (ext_no, page_no);
        if let Some(page) = self.cache.get_mut(&key) {
            page.data = data.to_vec();
            page.is_dirty = false;
            page.last_access = self.access_counter.fetch_add(1, Ordering::SeqCst);
        }
        
        Ok(())
    }
    
    /// 写入页面数据到文件
    fn write_page_to_file(&self, file: &mut File, page_no: u32, data: &[u8]) -> std::io::Result<()> {
        let offset = page_no as u64 * self.page_size as u64;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)?;
        file.flush()?;
        Ok(())
    }
    
    /// 刷新所有脏页到磁盘 (对应 db5_save_work 核心逻辑)
    /// 
    /// # 参数
    /// * `file` - 数据库文件句柄（需可写）
    /// 
    /// # 返回值
    /// * `Ok(usize)` - 成功写入的脏页数量
    /// * `Err` - I/O 错误
    pub fn flush_dirty_pages(&mut self, file: &mut File) -> std::io::Result<usize> {
        let dirty_keys: Vec<PageKey> = self.get_dirty_pages();
        let mut written_count = 0;
        
        for key in dirty_keys {
            if let Some(page) = self.cache.get(&key) {
                let page_no = page.page_no;
                let data = page.data.clone();
                
                // 写入到文件
                self.write_page_to_file(file, page_no, &data)?;
                self.stats.writes += 1;
                written_count += 1;
            }
            
            // 清除脏标记
            if let Some(page) = self.cache.get_mut(&key) {
                page.is_dirty = false;
            }
        }
        
        Ok(written_count)
    }
    
    /// 获取所有脏页的键列表
    pub fn get_dirty_pages(&self) -> Vec<PageKey> {
        self.cache
            .iter()
            .filter(|(_, page)| page.is_dirty)
            .map(|(key, _)| *key)
            .collect()
    }
    
    /// 检查是否有脏页
    pub fn has_dirty_pages(&self) -> bool {
        self.cache.values().any(|page| page.is_dirty)
    }
    
    /// 获取脏页数量
    pub fn dirty_count(&self) -> usize {
        self.cache.values().filter(|page| page.is_dirty).count()
    }
}

impl Default for PageManager {
    fn default() -> Self {
        Self::default_2k()
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
    
    #[test]
    fn test_dirty_page_tracking() {
        let mut pm = PageManager::new(16, 512);
        
        // 初始状态：无脏页
        assert!(!pm.has_dirty_pages());
        assert_eq!(pm.dirty_count(), 0);
        
        // 手动添加一个缓存页面并标记为脏
        let page = CachedPage::new(1, 0, vec![0u8; 512]);
        pm.cache.insert((0, 1), page);
        pm.mark_dirty(0, 1);
        
        // 验证脏页状态
        assert!(pm.has_dirty_pages());
        assert_eq!(pm.dirty_count(), 1);
        assert_eq!(pm.get_dirty_pages(), vec![(0, 1)]);
    }
    
    #[test]
    fn test_write_page_basic() {
        use std::io::Read;
        use std::fs::OpenOptions;
        
        // 创建临时文件
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_test_write.bin");
        
        // 清理可能存在的旧文件
        let _ = std::fs::remove_file(&temp_file);
        
        // 创建并初始化文件
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");
            
            // 写入 4 个空页面 (2KB)
            let empty_data = vec![0u8; 512 * 4];
            file.write_all(&empty_data).expect("无法初始化文件");
        }
        
        // 测试写入
        let mut pm = PageManager::new(16, 512);
        let test_data: Vec<u8> = (0..512).map(|i| (i % 256) as u8).collect();
        
        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");
            
            pm.write_page(&mut file, 0, 2, &test_data)
                .expect("写入页面失败");
        }
        
        // 验证写入
        {
            let mut file = std::fs::File::open(&temp_file).expect("无法打开临时文件");
            file.seek(SeekFrom::Start(512 * 2)).expect("无法定位");
            
            let mut read_data = vec![0u8; 512];
            file.read_exact(&mut read_data).expect("无法读取");
            
            assert_eq!(read_data, test_data);
        }
        
        // 验证统计
        assert_eq!(pm.stats().writes, 1);
        
        // 清理
        let _ = std::fs::remove_file(&temp_file);
    }
    
    #[test]
    fn test_flush_dirty_pages() {
        use std::fs::OpenOptions;
        
        // 创建临时文件
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_test_flush.bin");
        let _ = std::fs::remove_file(&temp_file);
        
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
        
        let mut pm = PageManager::new(16, 512);
        
        // 添加多个脏页
        for i in 0..3 {
            let mut page = CachedPage::new(i, 0, vec![(i as u8); 512]);
            page.is_dirty = true;
            pm.cache.insert((0, i), page);
        }
        
        assert_eq!(pm.dirty_count(), 3);
        
        // 刷新脏页
        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");
            
            let written = pm.flush_dirty_pages(&mut file).expect("刷新失败");
            assert_eq!(written, 3);
        }
        
        // 验证脏页已清除
        assert!(!pm.has_dirty_pages());
        assert_eq!(pm.dirty_count(), 0);
        
        // 清理
        let _ = std::fs::remove_file(&temp_file);
    }
}
