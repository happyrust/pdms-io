//! 页面缓存管理器模块
//!
//! 基于 IDA Pro 逆向分析 db1 模块实现的页面缓存机制。
//! 提供 LRU 缓存策略，脏页写回保证，批量 flush 和预读优化。
//!
//! # 设计参考
//! - db1_get_page: 检查缓存 -> 缓存命中返回 / 缓存未命中从磁盘读取
//! - db1_lock_page: 锁定页面防止被换出
//! - db1_update_page: 标记页面为脏页
//! - db1 eviction: 脏页驱逐前必须写回磁盘（对齐 core.dll 行为）

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::defines::{PAGE_SIZE_2K, PAGE_SIZE_512};

const DEFAULT_PREFETCH_PAGES: usize = 4;
const DEFAULT_BATCH_FLUSH_SIZE: usize = 32;

/// 缓存统计信息
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub reads: u64,
    pub writes: u64,
    pub evictions: u64,
    pub dirty_evictions: u64,
    pub prefetch_reads: u64,
}

impl CacheStats {
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
    pub data: Vec<u8>,
    pub page_no: u32,
    pub ext_no: u32,
    pub lock_count: u32,
    pub is_dirty: bool,
    pub last_access: u64,
}

impl CachedPage {
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

    pub fn is_locked(&self) -> bool {
        self.lock_count > 0
    }
}

/// 页面缓存键 (扩展号, 页面号)
pub type PageKey = (u32, u32);

/// 页面管理器
///
/// 基于 IDA Pro 分析的 db1 模块实现，提供页面级缓存、脏页安全驱逐、
/// 批量写入和预读功能。
#[derive(Debug)]
pub struct PageManager {
    pub(crate) cache: HashMap<PageKey, CachedPage>,
    max_pages: usize,
    page_size: usize,
    access_counter: AtomicU64,
    stats: CacheStats,
    prefetch_pages: usize,
    batch_flush_size: usize,
}

impl PageManager {
    pub fn new(max_pages: usize, page_size: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_pages),
            max_pages,
            page_size,
            access_counter: AtomicU64::new(0),
            stats: CacheStats::default(),
            prefetch_pages: DEFAULT_PREFETCH_PAGES,
            batch_flush_size: DEFAULT_BATCH_FLUSH_SIZE,
        }
    }

    pub fn default_512() -> Self {
        Self::new(256, PAGE_SIZE_512)
    }

    pub fn default_2k() -> Self {
        Self::new(256, PAGE_SIZE_2K)
    }

    pub fn set_prefetch_pages(&mut self, n: usize) {
        self.prefetch_pages = n;
    }

    pub fn set_batch_flush_size(&mut self, n: usize) {
        self.batch_flush_size = n.max(1);
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    pub fn cached_count(&self) -> usize {
        self.cache.len()
    }

    pub fn is_page_cached(&self, ext_no: u32, page_no: u32) -> bool {
        self.cache.contains_key(&(ext_no, page_no))
    }

    pub fn insert_page(&mut self, page: CachedPage) {
        let key = (page.ext_no, page.page_no);
        self.cache.insert(key, page);
    }

    /// 获取页面（缓存优先）
    ///
    /// 对应 db1_get_page：缓存命中则返回，未命中则从磁盘读取并加入缓存。
    /// 当缓存满时触发 LRU 驱逐，脏页驱逐前保证写回磁盘。
    /// 未命中时会预读后续 `prefetch_pages` 个相邻页。
    pub fn get_page(
        &mut self,
        file: &mut File,
        ext_no: u32,
        page_no: u32,
    ) -> std::io::Result<&[u8]> {
        let key = (ext_no, page_no);

        if self.cache.contains_key(&key) {
            self.stats.hits += 1;
            let access_time = self.access_counter.fetch_add(1, Ordering::Relaxed);
            if let Some(page) = self.cache.get_mut(&key) {
                page.last_access = access_time;
            }
            return Ok(&self.cache.get(&key).unwrap().data);
        }

        self.stats.misses += 1;

        self.ensure_capacity(file, 1 + self.prefetch_pages)?;

        let data = self.read_page_from_file(file, ext_no, page_no)?;
        self.stats.reads += 1;

        let access_time = self.access_counter.fetch_add(1, Ordering::Relaxed);
        let mut page = CachedPage::new(page_no, ext_no, data);
        page.last_access = access_time;
        self.cache.insert(key, page);

        self.prefetch(file, ext_no, page_no)?;

        Ok(&self.cache.get(&key).unwrap().data)
    }

    /// 获取页面的可变引用
    ///
    /// 返回后调用者可直接修改 `data`，之后应调用 `mark_dirty` 标记脏页。
    pub fn get_page_mut(
        &mut self,
        file: &mut File,
        ext_no: u32,
        page_no: u32,
    ) -> std::io::Result<&mut [u8]> {
        let key = (ext_no, page_no);

        if !self.cache.contains_key(&key) {
            self.stats.misses += 1;
            self.ensure_capacity(file, 1)?;

            let data = self.read_page_from_file(file, ext_no, page_no)?;
            self.stats.reads += 1;

            let access_time = self.access_counter.fetch_add(1, Ordering::Relaxed);
            let mut page = CachedPage::new(page_no, ext_no, data);
            page.last_access = access_time;
            self.cache.insert(key, page);
        } else {
            self.stats.hits += 1;
        }

        let access_time = self.access_counter.fetch_add(1, Ordering::Relaxed);
        let page = self.cache.get_mut(&key).unwrap();
        page.last_access = access_time;
        Ok(&mut page.data)
    }

    pub fn lock_page(&mut self, ext_no: u32, page_no: u32) -> bool {
        let key = (ext_no, page_no);
        if let Some(page) = self.cache.get_mut(&key) {
            page.lock_count += 1;
            true
        } else {
            false
        }
    }

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

    pub fn mark_dirty(&mut self, ext_no: u32, page_no: u32) -> bool {
        let key = (ext_no, page_no);
        if let Some(page) = self.cache.get_mut(&key) {
            page.is_dirty = true;
            true
        } else {
            false
        }
    }

    pub fn invalidate(&mut self, ext_no: u32, page_no: u32) -> Option<CachedPage> {
        let key = (ext_no, page_no);
        self.cache.remove(&key)
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.stats = CacheStats::default();
    }

    fn validate_ext_no(ext_no: u32) -> io::Result<()> {
        match ext_no {
            0 | 1 => Ok(()),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("暂不支持 ext_no={} 的多扩展物理路由", ext_no),
            )),
        }
    }

    fn page_offset(&self, ext_no: u32, page_no: u32) -> std::io::Result<u64> {
        Self::validate_ext_no(ext_no)?;
        Ok(page_no as u64 * self.page_size as u64)
    }

    fn read_page_from_file(
        &self,
        file: &mut File,
        ext_no: u32,
        page_no: u32,
    ) -> std::io::Result<Vec<u8>> {
        let offset = self.page_offset(ext_no, page_no)?;
        file.seek(SeekFrom::Start(offset))?;

        let mut data = vec![0u8; self.page_size];
        file.read_exact(&mut data)?;

        Ok(data)
    }

    /// 预读 page_no 之后的 prefetch_pages 个页面。
    /// 只读取尚未在缓存中的页面，读取失败（如超过文件尾）静默忽略。
    fn prefetch(&mut self, file: &mut File, ext_no: u32, base_page_no: u32) -> std::io::Result<()> {
        if self.prefetch_pages == 0 {
            return Ok(());
        }

        let access_time = self.access_counter.fetch_add(1, Ordering::Relaxed);

        for i in 1..=self.prefetch_pages as u32 {
            let pg = base_page_no.wrapping_add(i);
            let key = (ext_no, pg);

            if self.cache.contains_key(&key) || self.cache.len() >= self.max_pages {
                continue;
            }

            match self.read_page_from_file(file, ext_no, pg) {
                Ok(data) => {
                    self.stats.prefetch_reads += 1;
                    self.stats.reads += 1;
                    let mut page = CachedPage::new(pg, ext_no, data);
                    page.last_access = access_time;
                    self.cache.insert(key, page);
                }
                Err(_) => break,
            }
        }

        Ok(())
    }

    /// 确保缓存有足够空间容纳 `needed` 个新页面。
    /// 驱逐时优先选择干净且未锁定的页面；如果只剩脏页则写回磁盘后再驱逐。
    fn ensure_capacity(&mut self, file: &mut File, needed: usize) -> std::io::Result<()> {
        while self.cache.len() + needed > self.max_pages {
            self.evict_one(file)?;
        }
        Ok(())
    }

    /// LRU 驱逐一个页面。
    ///
    /// 优先驱逐干净的未锁定页面。如果所有可驱逐页面都是脏页，
    /// 则写回磁盘后再驱逐（保证数据不丢失）。
    fn evict_one(&mut self, file: &mut File) -> std::io::Result<()> {
        // Phase 1: 尝试找干净的未锁定页面
        let mut best_clean: Option<(PageKey, u64)> = None;
        let mut best_dirty: Option<(PageKey, u64)> = None;

        for (key, page) in &self.cache {
            if page.is_locked() {
                continue;
            }
            let target = if page.is_dirty {
                &mut best_dirty
            } else {
                &mut best_clean
            };
            match target {
                Some((_, t)) if page.last_access < *t => {
                    *target = Some((*key, page.last_access));
                }
                None => {
                    *target = Some((*key, page.last_access));
                }
                _ => {}
            }
        }

        if let Some((key, _)) = best_clean {
            self.cache.remove(&key);
            self.stats.evictions += 1;
        } else if let Some((key, _)) = best_dirty {
            if let Some(page) = self.cache.get(&key) {
                let ext_no = page.ext_no;
                let page_no = page.page_no;
                let data = page.data.clone();
                self.write_page_to_file(file, ext_no, page_no, &data)?;
                self.stats.writes += 1;
                self.stats.dirty_evictions += 1;
            }
            self.cache.remove(&key);
            self.stats.evictions += 1;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "缓存已满且所有页面均被锁定，无法驱逐",
            ));
        }

        Ok(())
    }

    // ==================================================================================
    // 写入功能
    // ==================================================================================

    /// 写入页面到文件并更新缓存（对应 db1_write_page_basic）
    pub fn write_page(
        &mut self,
        file: &mut File,
        ext_no: u32,
        page_no: u32,
        data: &[u8],
    ) -> std::io::Result<()> {
        assert_eq!(data.len(), self.page_size, "页面数据大小必须等于 page_size");

        self.write_page_to_file(file, ext_no, page_no, data)?;
        self.stats.writes += 1;

        let key = (ext_no, page_no);
        if let Some(page) = self.cache.get_mut(&key) {
            page.data.copy_from_slice(data);
            page.is_dirty = false;
            page.last_access = self.access_counter.fetch_add(1, Ordering::Relaxed);
        }

        Ok(())
    }

    pub fn allocate_page(&mut self, file: &mut File, ext_no: u32) -> std::io::Result<u32> {
        Self::validate_ext_no(ext_no)?;
        let file_size = file.seek(SeekFrom::End(0))?;
        let disk_page_count = (file_size / self.page_size as u64) as u32;
        let next_cached_page = self
            .cache
            .values()
            .filter(|page| page.ext_no == ext_no)
            .map(|page| page.page_no.saturating_add(1))
            .max()
            .unwrap_or(disk_page_count);
        let new_page_no = disk_page_count.max(next_cached_page);

        self.ensure_capacity(file, 1)?;

        let access_time = self.access_counter.fetch_add(1, Ordering::Relaxed);
        let mut page = CachedPage::new(new_page_no, ext_no, vec![0u8; self.page_size]);
        page.is_dirty = true;
        page.last_access = access_time;
        self.cache.insert((ext_no, new_page_no), page);

        Ok(new_page_no)
    }

    fn write_page_to_file(
        &self,
        file: &mut File,
        ext_no: u32,
        page_no: u32,
        data: &[u8],
    ) -> std::io::Result<()> {
        let offset = self.page_offset(ext_no, page_no)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)?;
        Ok(())
    }

    /// 批量刷新所有脏页到磁盘（对齐 db5_save_work）。
    ///
    /// 按 page_no 排序后顺序写入以减少磁盘寻道，分 batch 写入后统一 flush。
    pub fn flush_dirty_pages(&mut self, file: &mut File) -> std::io::Result<usize> {
        let mut dirty_keys: Vec<PageKey> = self.get_dirty_pages();
        if dirty_keys.is_empty() {
            return Ok(0);
        }

        dirty_keys.sort_by_key(|&(_, pgno)| pgno);

        let mut written_count = 0;

        for chunk in dirty_keys.chunks(self.batch_flush_size) {
            for &key in chunk {
                if let Some(page) = self.cache.get(&key) {
                    let ext_no = page.ext_no;
                    let page_no = page.page_no;
                    let data = page.data.clone();
                    self.write_page_to_file(file, ext_no, page_no, &data)?;
                    self.stats.writes += 1;
                    written_count += 1;
                }
            }
            file.flush()?;

            for &key in chunk {
                if let Some(page) = self.cache.get_mut(&key) {
                    page.is_dirty = false;
                }
            }
        }

        Ok(written_count)
    }

    pub fn get_dirty_pages(&self) -> Vec<PageKey> {
        self.cache
            .iter()
            .filter(|(_, page)| page.is_dirty)
            .map(|(key, _)| *key)
            .collect()
    }

    pub fn has_dirty_pages(&self) -> bool {
        self.cache.values().any(|page| page.is_dirty)
    }

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
            evictions: 0,
            dirty_evictions: 0,
            prefetch_reads: 0,
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

        assert!(!pm.has_dirty_pages());
        assert_eq!(pm.dirty_count(), 0);

        let page = CachedPage::new(1, 0, vec![0u8; 512]);
        pm.cache.insert((0, 1), page);
        pm.mark_dirty(0, 1);

        assert!(pm.has_dirty_pages());
        assert_eq!(pm.dirty_count(), 1);
        assert_eq!(pm.get_dirty_pages(), vec![(0, 1)]);
    }

    #[test]
    fn test_write_page_basic() {
        use std::fs::OpenOptions;
        use std::io::Read;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_test_write.bin");
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

        {
            let mut file = std::fs::File::open(&temp_file).expect("无法打开临时文件");
            file.seek(SeekFrom::Start(512 * 2)).expect("无法定位");

            let mut read_data = vec![0u8; 512];
            file.read_exact(&mut read_data).expect("无法读取");

            assert_eq!(read_data, test_data);
        }

        assert_eq!(pm.stats().writes, 1);
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_flush_dirty_pages() {
        use std::fs::OpenOptions;

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

        for i in 0..3 {
            let mut page = CachedPage::new(i, 0, vec![i as u8; 512]);
            page.is_dirty = true;
            pm.cache.insert((0, i), page);
        }

        assert_eq!(pm.dirty_count(), 3);

        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_file)
                .expect("无法打开临时文件");

            let written = pm.flush_dirty_pages(&mut file).expect("刷新失败");
            assert_eq!(written, 3);
        }

        assert!(!pm.has_dirty_pages());
        assert_eq!(pm.dirty_count(), 0);

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_dirty_eviction_writes_back() {
        use std::fs::OpenOptions;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_test_evict.bin");
        let _ = std::fs::remove_file(&temp_file);

        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");
            let empty_data = vec![0u8; 512 * 8];
            file.write_all(&empty_data).expect("无法初始化文件");
        }

        // max_pages=2, prefetch=0 so eviction is forced on third page
        let mut pm = PageManager::new(2, 512);
        pm.set_prefetch_pages(0);

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&temp_file)
            .expect("无法打开临时文件");

        pm.get_page(&mut file, 0, 0).expect("读取页面0");
        pm.cache.get_mut(&(0, 0)).unwrap().data = vec![0xAA; 512];
        pm.mark_dirty(0, 0);

        pm.get_page(&mut file, 0, 1).expect("读取页面1");

        // Reading page 2 should evict page 0 (dirty), which must be written back
        pm.get_page(&mut file, 0, 2).expect("读取页面2");

        assert!(pm.stats.dirty_evictions >= 1, "脏页应被写回后驱逐");
        assert!(!pm.cache.contains_key(&(0, 0)), "页面0应已被驱逐");

        // Verify the dirty data was written to disk
        file.seek(SeekFrom::Start(0)).expect("seek");
        let mut read_data = vec![0u8; 512];
        file.read_exact(&mut read_data).expect("read");
        assert_eq!(read_data, vec![0xAA; 512], "脏页数据应已写回磁盘");

        drop(file);
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_get_page_fails_when_all_cached_pages_are_locked() {
        use std::fs::OpenOptions;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_test_locked_capacity.bin");
        let _ = std::fs::remove_file(&temp_file);

        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");
            let empty_data = vec![0u8; 512 * 2];
            file.write_all(&empty_data).expect("无法初始化文件");
        }

        let mut pm = PageManager::new(1, 512);
        pm.set_prefetch_pages(0);

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&temp_file)
            .expect("无法打开临时文件");

        pm.get_page(&mut file, 1, 0).expect("读取页面0");
        assert!(pm.lock_page(1, 0));

        let err = pm
            .get_page(&mut file, 1, 1)
            .expect_err("锁满缓存时应返回错误");
        assert_eq!(err.kind(), io::ErrorKind::WouldBlock);

        drop(file);
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_unsupported_ext_no_returns_error() {
        use std::fs::OpenOptions;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_test_extno.bin");
        let _ = std::fs::remove_file(&temp_file);

        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_file)
                .expect("无法创建临时文件");
            let empty_data = vec![0u8; 512 * 2];
            file.write_all(&empty_data).expect("无法初始化文件");
        }

        let mut pm = PageManager::new(4, 512);
        pm.set_prefetch_pages(0);

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&temp_file)
            .expect("无法打开临时文件");

        let read_err = pm
            .get_page(&mut file, 2, 0)
            .expect_err("未实现扩展号应报错");
        assert_eq!(read_err.kind(), io::ErrorKind::Unsupported);

        let write_err = pm
            .write_page(&mut file, 2, 0, &vec![0u8; 512])
            .expect_err("未实现扩展号应报错");
        assert_eq!(write_err.kind(), io::ErrorKind::Unsupported);

        drop(file);
        let _ = std::fs::remove_file(&temp_file);
    }
}
