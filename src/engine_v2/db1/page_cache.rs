use std::collections::HashMap;

use crate::engine_v2::io_layer::FileHandle;
use crate::engine_v2::types::*;
use super::page_io::PageIO;

const DEFAULT_POOL_SIZE: usize = 256;
const DEFAULT_PREFETCH: u32 = 4;

/// 页面缓存统计
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub dirty_writebacks: u64,
    pub prefetch_reads: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

/// pfno 池化页面缓存
///
/// 对齐 core.dll 的 db1 页面管理机制：
/// - 固定大小描述符池 (非 HashMap 动态扩展)
/// - 三元组 (dbno, page_id, extent) 匹配
/// - LRU 驱逐 + lock_count 保护 + referenced bit
/// - 脏页安全驱逐 (写回后再释放)
/// - 批量预读
pub struct PageCache {
    /// 描述符池：pfno → PageDescriptor
    pool: Vec<Option<PageDescriptor>>,
    /// 快速查找：PageId → pfno
    lookup: HashMap<PageId, usize>,
    /// 池大小上限
    capacity: usize,
    /// 单调递增的访问计数器
    tick: u64,
    /// 预读页数
    prefetch_count: u32,
    /// 页面 I/O
    page_io: PageIO,
    /// 统计信息
    pub stats: CacheStats,
}

impl PageCache {
    pub fn new(capacity: usize) -> Self {
        let cap = if capacity == 0 { DEFAULT_POOL_SIZE } else { capacity };
        Self {
            pool: (0..cap).map(|_| None).collect(),
            lookup: HashMap::with_capacity(cap),
            capacity: cap,
            tick: 0,
            prefetch_count: DEFAULT_PREFETCH,
            page_io: PageIO::new(),
            stats: CacheStats::default(),
        }
    }

    pub fn set_prefetch(&mut self, count: u32) {
        self.prefetch_count = count;
    }

    /// 检查页面是否在缓存中 (对齐 db1_is_page_incore)
    ///
    /// 返回 pfno (缓存池槽位索引)，None 表示未命中
    pub fn is_page_incore(&self, id: &PageId) -> Option<usize> {
        self.lookup.get(id).copied()
    }

    /// 获取页面 (对齐 db1_get_page)
    ///
    /// 缓存命中：增加 lock_count + 设置 referenced bit + 返回数据引用
    /// 缓存未命中：LRU 分配槽位 → 磁盘读取 → 写入缓存
    pub fn get_page(
        &mut self,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        page_no: u32,
    ) -> DbResult<&[u8]> {
        let id = PageId::new(dbno, page_no, extent);

        if let Some(pfno) = self.lookup.get(&id).copied() {
            self.tick += 1;
            let desc = self.pool[pfno].as_mut().unwrap();
            desc.lock();
            desc.access_tick = self.tick;
            self.stats.hits += 1;
            return Ok(&self.pool[pfno].as_ref().unwrap().data);
        }

        self.stats.misses += 1;
        let pfno = self.allocate_slot(handle)?;

        if self.prefetch_count > 1 {
            match self.page_io.prefetch_pages(handle, dbno, extent, page_no, self.prefetch_count) {
                Ok(pages) => {
                    self.stats.prefetch_reads += pages.len() as u64;
                    for (i, mut desc) in pages.into_iter().enumerate() {
                        self.tick += 1;
                        desc.access_tick = self.tick;
                        if i == 0 {
                            desc.lock();
                            let pid = desc.id;
                            self.pool[pfno] = Some(desc);
                            self.lookup.insert(pid, pfno);
                        } else {
                            if self.lookup.contains_key(&desc.id) { continue; }
                            if let Ok(slot) = self.find_free_slot() {
                                let pid = desc.id;
                                self.pool[slot] = Some(desc);
                                self.lookup.insert(pid, slot);
                            }
                        }
                    }
                }
                Err(_) => {
                    let mut desc = self.page_io.read_page(handle, dbno, extent, page_no)?;
                    self.tick += 1;
                    desc.access_tick = self.tick;
                    desc.lock();
                    self.pool[pfno] = Some(desc);
                    self.lookup.insert(id, pfno);
                }
            }
        } else {
            let mut desc = self.page_io.read_page(handle, dbno, extent, page_no)?;
            self.tick += 1;
            desc.access_tick = self.tick;
            desc.lock();
            self.pool[pfno] = Some(desc);
            self.lookup.insert(id, pfno);
        }

        if self.pool[pfno].is_none() {
            let mut desc = self.page_io.read_page(handle, dbno, extent, page_no)?;
            self.tick += 1;
            desc.access_tick = self.tick;
            desc.lock();
            self.pool[pfno] = Some(desc);
            self.lookup.insert(id, pfno);
        }

        Ok(&self.pool[pfno].as_ref().unwrap().data)
    }

    /// 获取页面可变引用 (用于 COW 写入)
    pub fn get_page_mut(
        &mut self,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        page_no: u32,
    ) -> DbResult<&mut [u8]> {
        let id = PageId::new(dbno, page_no, extent);

        if self.lookup.get(&id).is_none() {
            self.get_page(handle, dbno, extent, page_no)?;
        }

        let pfno = self.lookup[&id];
        let desc = self.pool[pfno].as_mut().unwrap();
        desc.mark_dirty();
        Ok(&mut desc.data)
    }

    /// 解锁页面 (减少 lock_count)
    pub fn unlock_page(&mut self, id: &PageId) {
        if let Some(&pfno) = self.lookup.get(id) {
            if let Some(desc) = self.pool[pfno].as_mut() {
                desc.unlock();
            }
        }
    }

    /// 标记页面为脏页
    pub fn mark_dirty(&mut self, id: &PageId) {
        if let Some(&pfno) = self.lookup.get(id) {
            if let Some(desc) = self.pool[pfno].as_mut() {
                desc.mark_dirty();
            }
        }
    }

    /// 将所有脏页写回磁盘
    pub fn flush_all(&mut self, handle: &mut FileHandle) -> DbResult<u32> {
        let mut count = 0u32;
        for slot in 0..self.capacity {
            if let Some(desc) = &self.pool[slot] {
                if desc.dirty {
                    self.page_io.write_page(handle, desc)?;
                    self.pool[slot].as_mut().unwrap().dirty = false;
                    self.stats.dirty_writebacks += 1;
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// 清空缓存 (所有脏页先写回)
    pub fn invalidate_all(&mut self, handle: &mut FileHandle) -> DbResult<()> {
        self.flush_all(handle)?;
        for slot in 0..self.capacity {
            self.pool[slot] = None;
        }
        self.lookup.clear();
        Ok(())
    }

    /// LRU 分配槽位 (对齐 db1_plu_locate_entry)
    fn allocate_slot(&mut self, handle: &mut FileHandle) -> DbResult<usize> {
        if let Ok(slot) = self.find_free_slot() {
            return Ok(slot);
        }
        self.evict_one(handle)
    }

    /// 查找空闲槽位
    fn find_free_slot(&self) -> Result<usize, ()> {
        for i in 0..self.capacity {
            if self.pool[i].is_none() {
                return Ok(i);
            }
        }
        Err(())
    }

    /// LRU 驱逐一个未锁定页面
    fn evict_one(&mut self, handle: &mut FileHandle) -> DbResult<usize> {
        let mut best_pfno = None;
        let mut best_tick = u64::MAX;

        for i in 0..self.capacity {
            if let Some(desc) = &self.pool[i] {
                if desc.is_locked() { continue; }
                if desc.referenced {
                    self.pool[i].as_mut().unwrap().referenced = false;
                    continue;
                }
                if desc.access_tick < best_tick {
                    best_tick = desc.access_tick;
                    best_pfno = Some(i);
                }
            }
        }

        let pfno = best_pfno.ok_or(DbError::CacheExhausted)?;

        let desc = self.pool[pfno].as_ref().unwrap();
        if desc.dirty {
            self.page_io.write_page(handle, desc)?;
            self.stats.dirty_writebacks += 1;
        }
        let old_id = desc.id;
        self.lookup.remove(&old_id);
        self.pool[pfno] = None;
        self.stats.evictions += 1;

        Ok(pfno)
    }
}
