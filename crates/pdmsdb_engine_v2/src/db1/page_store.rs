use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

use crate::core::{EngineError, PageId};
use crate::db2::{HeaderView, SessionPageView};

/// Read-only I/O counters for one [`PageStore`].
///
/// `index_pages_read` and `record_pages_read` are physical-read deltas assigned
/// by the higher-level operations. `physical_pages_read` also includes pages
/// brought in by prefetch.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PageReadStats {
    pub physical_pages_read: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub prefetched_pages: u64,
    pub bytes_read: u64,
    pub index_pages_read: u64,
    pub record_pages_read: u64,
}

pub trait PageIo {
    fn read_page(
        &mut self,
        file: &mut File,
        page_id: PageId,
        page_size: usize,
    ) -> Result<Vec<u8>, EngineError>;
}

#[derive(Default)]
pub struct StdPageIo;

impl PageIo for StdPageIo {
    fn read_page(
        &mut self,
        file: &mut File,
        page_id: PageId,
        page_size: usize,
    ) -> Result<Vec<u8>, EngineError> {
        let offset = page_id.page_no as u64 * page_size as u64;
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; page_size];
        file.read_exact(&mut buf)?;
        Ok(buf)
    }
}

#[derive(Clone)]
struct PageFrame {
    #[allow(dead_code)]
    pfno: u32,
    page_id: PageId,
    data: Vec<u8>,
    dirty: bool,
    lock_count: u32,
    referenced: bool,
    access_seq: u64,
    cow_original: Option<Vec<u8>>,
}

pub struct PageCache {
    frames: Vec<PageFrame>,
    lookup: HashMap<PageId, usize>,
    max_frames: usize,
    next_seq: u64,
}

impl Default for PageCache {
    fn default() -> Self {
        Self::new(512)
    }
}

impl PageCache {
    pub fn new(max_frames: usize) -> Self {
        Self {
            frames: Vec::with_capacity(max_frames),
            lookup: HashMap::with_capacity(max_frames),
            max_frames,
            next_seq: 0,
        }
    }

    fn next_pfno(&self) -> u32 {
        self.frames.len() as u32 + 1
    }

    fn touch(&mut self, idx: usize) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.frames[idx].access_seq = seq;
        self.frames[idx].referenced = true;
    }

    fn find_evictable(&self) -> Option<usize> {
        self.frames
            .iter()
            .enumerate()
            .filter(|(_, f)| f.lock_count == 0)
            .min_by_key(|(_, f)| f.access_seq)
            .map(|(i, _)| i)
    }

    pub fn snapshot_cow(&mut self) {
        for frame in &mut self.frames {
            if frame.dirty && frame.cow_original.is_none() {
                frame.cow_original = Some(frame.data.clone());
            }
        }
    }

    pub fn rollback_cow(&mut self) {
        let mut restored = Vec::new();
        for (idx, frame) in self.frames.iter_mut().enumerate() {
            if let Some(original) = frame.cow_original.take() {
                frame.data = original;
                frame.dirty = false;
                restored.push(idx);
            }
        }
    }

    pub fn discard_cow(&mut self) {
        for frame in &mut self.frames {
            frame.cow_original = None;
        }
    }
}

pub struct PageGuard {
    pub page_id: PageId,
    pub data: Vec<u8>,
}

pub struct PageStore {
    page_size: usize,
    prefetch_pages: usize,
    pub(crate) cache: PageCache,
    io: StdPageIo,
    stats: PageReadStats,
}

impl PageStore {
    pub fn new(page_size: usize, prefetch_pages: usize) -> Self {
        Self {
            page_size,
            prefetch_pages,
            cache: PageCache::default(),
            io: StdPageIo,
            stats: PageReadStats::default(),
        }
    }

    pub fn new_with_cache_size(page_size: usize, prefetch_pages: usize, max_frames: usize) -> Self {
        Self {
            page_size,
            prefetch_pages,
            cache: PageCache::new(max_frames),
            io: StdPageIo,
            stats: PageReadStats::default(),
        }
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    pub fn read_stats(&self) -> PageReadStats {
        self.stats
    }

    pub(crate) fn record_index_reads(&mut self, physical_pages: u64) {
        self.stats.index_pages_read = self.stats.index_pages_read.saturating_add(physical_pages);
    }

    pub(crate) fn record_record_reads(&mut self, physical_pages: u64) {
        self.stats.record_pages_read = self.stats.record_pages_read.saturating_add(physical_pages);
    }

    pub fn validate_page_size_from_header(
        file: &mut File,
        header: &HeaderView,
        hint: Option<usize>,
    ) -> Result<usize, EngineError> {
        let mut candidates = Vec::new();
        if let Some(hint) = hint {
            candidates.push(hint);
        }
        if matches!(header.page_size, 512 | 2048 | 4096) {
            candidates.push(header.page_size as usize);
        }
        candidates.extend([2048usize, 4096usize, 512usize]);
        candidates.dedup();

        let file_len = file.metadata()?.len();
        for candidate in candidates {
            let page_no = header.latest_ses_pgno;
            if page_no == 0 {
                continue;
            }
            let offset = page_no as u64 * candidate as u64;
            if offset + 0x34 > file_len {
                continue;
            }
            file.seek(SeekFrom::Start(offset))?;
            let mut buf = [0u8; 0x34];
            file.read_exact(&mut buf)?;
            let Ok(session) = SessionPageView::from_page(&buf) else {
                continue;
            };
            if session_page_bounds_are_coherent(header, &session) {
                return Ok(candidate);
            }
        }

        if matches!(header.page_size, 512 | 2048 | 4096) {
            return Ok(header.page_size as usize);
        }

        Ok(2048)
    }

    pub fn read_page(&mut self, file: &mut File, page_id: PageId) -> Result<Vec<u8>, EngineError> {
        if let Some(&idx) = self.cache.lookup.get(&page_id) {
            self.stats.cache_hits = self.stats.cache_hits.saturating_add(1);
            self.cache.touch(idx);
            return Ok(self.cache.frames[idx].data.clone());
        }

        self.stats.cache_misses = self.stats.cache_misses.saturating_add(1);
        self.ensure_capacity(file)?;
        let data = self.io.read_page(file, page_id, self.page_size)?;
        self.stats.physical_pages_read = self.stats.physical_pages_read.saturating_add(1);
        self.stats.bytes_read = self.stats.bytes_read.saturating_add(self.page_size as u64);
        self.insert_frame(page_id, data.clone(), false);

        if self.prefetch_pages > 0 {
            let _ = self.prefetch_adjacent(file, page_id, self.prefetch_pages);
        }
        Ok(data)
    }

    pub fn prefetch_adjacent(
        &mut self,
        file: &mut File,
        page_id: PageId,
        n: usize,
    ) -> Result<(), EngineError> {
        for step in 1..=n {
            let next = PageId {
                ext_no: page_id.ext_no,
                page_no: page_id.page_no.saturating_add(step as u32),
            };
            if self.cache.lookup.contains_key(&next) {
                continue;
            }
            if self.cache.frames.len() >= self.cache.max_frames {
                break;
            }
            match self.io.read_page(file, next, self.page_size) {
                Ok(data) => {
                    self.stats.physical_pages_read =
                        self.stats.physical_pages_read.saturating_add(1);
                    self.stats.prefetched_pages = self.stats.prefetched_pages.saturating_add(1);
                    self.stats.bytes_read =
                        self.stats.bytes_read.saturating_add(self.page_size as u64);
                    self.insert_frame(next, data, false);
                }
                Err(_) => break,
            }
        }
        Ok(())
    }

    pub fn read_guard(
        &mut self,
        file: &mut File,
        page_id: PageId,
    ) -> Result<PageGuard, EngineError> {
        Ok(PageGuard {
            page_id,
            data: self.read_page(file, page_id)?,
        })
    }

    pub fn mark_dirty(&mut self, page_id: PageId) -> Result<(), EngineError> {
        let &idx = self
            .cache
            .lookup
            .get(&page_id)
            .ok_or_else(|| EngineError::NotFound(format!("页面 {:?} 不在缓存中", page_id)))?;
        self.cache.frames[idx].dirty = true;
        Ok(())
    }

    pub fn update_page_cow(
        &mut self,
        file: &mut File,
        page_id: PageId,
    ) -> Result<&mut [u8], EngineError> {
        if !self.cache.lookup.contains_key(&page_id) {
            let _ = self.read_page(file, page_id)?;
        }
        let &idx = self.cache.lookup.get(&page_id).unwrap();
        let frame = &mut self.cache.frames[idx];
        if frame.cow_original.is_none() {
            frame.cow_original = Some(frame.data.clone());
        }
        frame.dirty = true;
        self.cache.touch(idx);
        let frame = &mut self.cache.frames[idx];
        Ok(&mut frame.data)
    }

    pub fn write_page(
        &mut self,
        _file: &mut File,
        page_id: PageId,
        data: &[u8],
    ) -> Result<(), EngineError> {
        if data.len() != self.page_size {
            return Err(EngineError::Format(format!(
                "页面大小不匹配: expected={}, actual={}",
                self.page_size,
                data.len()
            )));
        }

        if let Some(&idx) = self.cache.lookup.get(&page_id) {
            self.cache.frames[idx].data = data.to_vec();
            self.cache.frames[idx].dirty = true;
            self.cache.touch(idx);
        } else {
            self.insert_frame(page_id, data.to_vec(), true);
        }
        Ok(())
    }

    pub fn lock_page(&mut self, page_id: PageId) -> bool {
        if let Some(&idx) = self.cache.lookup.get(&page_id) {
            self.cache.frames[idx].lock_count += 1;
            self.cache.frames[idx].referenced = true;
            true
        } else {
            false
        }
    }

    pub fn unlock_page(&mut self, page_id: PageId) -> bool {
        if let Some(&idx) = self.cache.lookup.get(&page_id) {
            if self.cache.frames[idx].lock_count > 0 {
                self.cache.frames[idx].lock_count -= 1;
            }
            true
        } else {
            false
        }
    }

    pub fn allocate_page(&mut self, file: &mut File, ext_no: u32) -> Result<PageId, EngineError> {
        let file_size = file.seek(SeekFrom::End(0))?;
        let disk_page_count = (file_size / self.page_size as u64) as u32;
        let next_cached = self
            .cache
            .frames
            .iter()
            .filter(|f| f.page_id.ext_no == ext_no)
            .map(|f| f.page_id.page_no.saturating_add(1))
            .max()
            .unwrap_or(disk_page_count);
        let page_id = PageId {
            ext_no,
            page_no: disk_page_count.max(next_cached),
        };

        self.insert_frame(page_id, vec![0u8; self.page_size], true);
        Ok(page_id)
    }

    pub fn flush_dirty(&mut self, file: &mut File) -> Result<usize, EngineError> {
        let mut dirty_indices: Vec<usize> = self
            .cache
            .frames
            .iter()
            .enumerate()
            .filter(|(_, f)| f.dirty)
            .map(|(i, _)| i)
            .collect();
        dirty_indices.sort_by_key(|&i| self.cache.frames[i].page_id.page_no);

        let mut written = 0usize;
        for &idx in &dirty_indices {
            let frame = &self.cache.frames[idx];
            let offset = frame.page_id.page_no as u64 * self.page_size as u64;
            file.seek(SeekFrom::Start(offset))?;
            file.write_all(&frame.data)?;
            written += 1;
        }
        if written > 0 {
            file.flush()?;
        }
        for &idx in &dirty_indices {
            self.cache.frames[idx].dirty = false;
            self.cache.frames[idx].cow_original = None;
        }
        Ok(written)
    }

    fn insert_frame(&mut self, page_id: PageId, data: Vec<u8>, dirty: bool) {
        let pfno = self.cache.next_pfno();
        let seq = self.cache.next_seq;
        self.cache.next_seq += 1;
        let idx = self.cache.frames.len();
        self.cache.frames.push(PageFrame {
            pfno,
            page_id,
            data,
            dirty,
            lock_count: 0,
            referenced: true,
            access_seq: seq,
            cow_original: None,
        });
        self.cache.lookup.insert(page_id, idx);
    }

    pub fn snapshot_cow(&mut self) {
        self.cache.snapshot_cow();
    }

    pub fn rollback_cow(&mut self) {
        self.cache.rollback_cow();
    }

    pub fn discard_cow(&mut self) {
        self.cache.discard_cow();
    }

    fn ensure_capacity(&mut self, file: &mut File) -> Result<(), EngineError> {
        while self.cache.frames.len() >= self.cache.max_frames {
            let victim = self.cache.find_evictable().ok_or_else(|| {
                EngineError::InvalidState("缓存已满且所有页面均被锁定".into())
            })?;
            let frame = &self.cache.frames[victim];
            if frame.dirty {
                let offset = frame.page_id.page_no as u64 * self.page_size as u64;
                file.seek(SeekFrom::Start(offset))?;
                file.write_all(&frame.data)?;
                file.flush()?;
            }
            let removed_id = self.cache.frames[victim].page_id;
            self.cache.lookup.remove(&removed_id);
            self.cache.frames.swap_remove(victim);
            if victim < self.cache.frames.len() {
                let swapped_id = self.cache.frames[victim].page_id;
                self.cache.lookup.insert(swapped_id, victim);
            }
        }
        Ok(())
    }
}

fn session_page_bounds_are_coherent(header: &HeaderView, session: &SessionPageView) -> bool {
    let current = (header.ext_no.max(1), header.latest_ses_pgno);
    let end = (session.end_extno.max(1), session.end_pgno);
    let index = (session.index_root_extno.max(1), session.index_root_pageno);
    let previous = (
        session.last_ses_extno.max(1),
        session.last_ses_pageno.max(0) as u32,
    );

    session.end_pgno != 0
        && session.index_root_pageno != 0
        && current <= end
        && index <= end
        && (session.last_ses_pageno <= 0 || previous < current)
}

#[cfg(test)]
mod tests {
    use std::io::{Seek, SeekFrom, Write};

    use tempfile::tempfile;

    use super::PageStore;
    use crate::db2::HeaderView;

    fn write_u32(page: &mut [u8], offset: usize, value: u32) {
        page[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn session_page(page_size: usize, sesno: u32, page_no: u32, index_root: u32) -> Vec<u8> {
        let mut page = vec![0u8; page_size];
        write_u32(&mut page, 0x00, 3);
        write_u32(&mut page, 0x04, 4);
        write_u32(&mut page, 0x08, 1);
        write_u32(&mut page, 0x0c, sesno);
        write_u32(&mut page, 0x14, page_no);
        write_u32(&mut page, 0x18, 1);
        write_u32(&mut page, 0x1c, index_root);
        write_u32(&mut page, 0x20, 1);
        page
    }

    #[test]
    fn rejects_a_header_size_false_positive_when_the_session_bounds_are_impossible() {
        let latest_page = 9u32;
        let mut file = tempfile().unwrap();
        file.set_len((latest_page as u64 + 1) * 2048).unwrap();

        let mut false_page = session_page(512, 74, 1, 6);
        write_u32(&mut false_page, 0x04, u32::MAX - 10);
        file.seek(SeekFrom::Start(latest_page as u64 * 512))
            .unwrap();
        file.write_all(&false_page).unwrap();

        let true_page = session_page(2048, 272, latest_page, latest_page - 1);
        file.seek(SeekFrom::Start(latest_page as u64 * 2048))
            .unwrap();
        file.write_all(&true_page).unwrap();

        let header = HeaderView {
            version: 2,
            db_num: 7000,
            latest_ses_pgno: latest_page,
            ext_no: 1,
            session_page_no: 2,
            page_size: 512,
            stored_page_count: 0,
        };

        let actual = PageStore::validate_page_size_from_header(&mut file, &header, None).unwrap();
        assert_eq!(actual, 2048);
    }
}
