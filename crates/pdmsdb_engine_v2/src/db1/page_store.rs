use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

use crate::core::{EngineError, PageId};
use crate::db2::HeaderView;

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
        if page_id.ext_no != 1 {
            return Err(EngineError::Unsupported(format!(
                "V2 只支持 ext_no=1，收到 {}",
                page_id.ext_no
            )));
        }

        let offset = page_id.page_no as u64 * page_size as u64;
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; page_size];
        file.read_exact(&mut buf)?;
        Ok(buf)
    }
}

#[derive(Default)]
pub struct PageCache {
    pages: HashMap<PageId, CachedPage>,
}

#[derive(Clone)]
struct CachedPage {
    data: Vec<u8>,
    dirty: bool,
}

pub struct PageGuard {
    pub page_id: PageId,
    pub data: Vec<u8>,
}

pub struct PageStore {
    page_size: usize,
    prefetch_pages: usize,
    cache: PageCache,
    io: StdPageIo,
}

impl PageStore {
    pub fn new(page_size: usize, prefetch_pages: usize) -> Self {
        Self {
            page_size,
            prefetch_pages,
            cache: PageCache::default(),
            io: StdPageIo,
        }
    }

    pub fn page_size(&self) -> usize {
        self.page_size
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
            for page_no in [header.session_page_no, header.latest_ses_pgno] {
                if page_no == 0 {
                    continue;
                }
                let offset = page_no as u64 * candidate as u64;
                if offset + 4 > file_len {
                    continue;
                }
                file.seek(SeekFrom::Start(offset))?;
                let mut buf = [0u8; 4];
                file.read_exact(&mut buf)?;
                if u32::from_be_bytes(buf) == 3 {
                    return Ok(candidate);
                }
            }
        }

        if matches!(header.page_size, 512 | 2048 | 4096) {
            return Ok(header.page_size as usize);
        }

        Ok(2048)
    }

    pub fn read_page(&mut self, file: &mut File, page_id: PageId) -> Result<Vec<u8>, EngineError> {
        if let Some(cached) = self.cache.pages.get(&page_id) {
            return Ok(cached.data.clone());
        }

        let page = self.io.read_page(file, page_id, self.page_size)?;
        self.cache.pages.insert(
            page_id,
            CachedPage {
                data: page.clone(),
                dirty: false,
            },
        );
        if self.prefetch_pages > 0 {
            let _ = self.prefetch_adjacent(file, page_id, self.prefetch_pages);
        }
        Ok(page)
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
            if self.cache.pages.contains_key(&next) {
                continue;
            }
            match self.io.read_page(file, next, self.page_size) {
                Ok(page) => {
                    self.cache.pages.insert(
                        next,
                        CachedPage {
                            data: page,
                            dirty: false,
                        },
                    );
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
        let page = self
            .cache
            .pages
            .get_mut(&page_id)
            .ok_or_else(|| EngineError::NotFound(format!("页面 {:?} 不在缓存中", page_id)))?;
        page.dirty = true;
        Ok(())
    }

    pub fn write_page(
        &mut self,
        _file: &mut File,
        page_id: PageId,
        data: &[u8],
    ) -> Result<(), EngineError> {
        if page_id.ext_no != 1 {
            return Err(EngineError::Unsupported(format!(
                "V2 只支持 ext_no=1，收到 {}",
                page_id.ext_no
            )));
        }
        if data.len() != self.page_size {
            return Err(EngineError::Format(format!(
                "页面大小不匹配: expected={}, actual={}",
                self.page_size,
                data.len()
            )));
        }

        self.cache.pages.insert(
            page_id,
            CachedPage {
                data: data.to_vec(),
                dirty: true,
            },
        );
        Ok(())
    }

    pub fn allocate_page(&mut self, file: &mut File, ext_no: u32) -> Result<PageId, EngineError> {
        if ext_no != 1 {
            return Err(EngineError::Unsupported(format!(
                "V2 只支持 ext_no=1，收到 {}",
                ext_no
            )));
        }

        let file_size = file.seek(SeekFrom::End(0))?;
        let disk_page_count = (file_size / self.page_size as u64) as u32;
        let next_cached_page = self
            .cache
            .pages
            .keys()
            .filter(|page| page.ext_no == ext_no)
            .map(|page| page.page_no.saturating_add(1))
            .max()
            .unwrap_or(disk_page_count);
        let page_id = PageId {
            ext_no,
            page_no: disk_page_count.max(next_cached_page),
        };

        self.cache.pages.insert(
            page_id,
            CachedPage {
                data: vec![0u8; self.page_size],
                dirty: true,
            },
        );
        Ok(page_id)
    }

    pub fn flush_dirty(&mut self, file: &mut File) -> Result<usize, EngineError> {
        let mut dirty_ids: Vec<PageId> = self
            .cache
            .pages
            .iter()
            .filter_map(|(page_id, page)| page.dirty.then_some(*page_id))
            .collect();
        dirty_ids.sort_by_key(|page_id| page_id.page_no);

        let mut written = 0usize;
        for page_id in dirty_ids {
            let page =
                self.cache.pages.get(&page_id).cloned().ok_or_else(|| {
                    EngineError::NotFound(format!("页面 {:?} 不在缓存中", page_id))
                })?;
            let offset = page_id.page_no as u64 * self.page_size as u64;
            file.seek(SeekFrom::Start(offset))?;
            file.write_all(&page.data)?;
            if let Some(entry) = self.cache.pages.get_mut(&page_id) {
                entry.dirty = false;
            }
            written += 1;
        }
        if written > 0 {
            file.flush()?;
        }
        Ok(written)
    }
}
