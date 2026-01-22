use std::fs::File;

use anyhow::Result;

use crate::page_manager::PageManager;

pub struct PagedReader;

impl PagedReader {
    pub fn read(
        file: &mut File,
        page_cache: &mut PageManager,
        ext_no: u32,
        page_size: usize,
        start_offset: u64,
        length: usize,
    ) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(length);
        let mut remaining = length;
        let mut current_offset = start_offset;

        while remaining > 0 {
            let pgno = (current_offset / page_size as u64) as u32;
            let offset_in_page = (current_offset % page_size as u64) as usize;
            let available_in_page = page_size - offset_in_page;
            let to_read = std::cmp::min(available_in_page, remaining);

            let data = page_cache.get_page(file, ext_no, pgno)?;
            result.extend_from_slice(&data[offset_in_page..offset_in_page + to_read]);

            current_offset += to_read as u64;
            remaining -= to_read;
        }

        Ok(result)
    }
}
