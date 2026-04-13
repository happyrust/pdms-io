use std::fs::File;

use crate::core::{EngineError, PageId, RecordLoc};
use crate::db1::PageStore;

pub struct RecordReaderV2;

impl RecordReaderV2 {
    const INITIAL: usize = 16 * 1024;
    const MAX: usize = 1024 * 1024;
    const PADDING_ZERO: [u8; 4] = [0, 0, 0, 0];
    const PADDING_SEVEN: [u8; 4] = [0, 0, 0, 7];

    pub fn read_record(
        file: &mut File,
        store: &mut PageStore,
        loc: RecordLoc,
    ) -> Result<Vec<u8>, EngineError> {
        let page_size = store.page_size();
        let start_page = store.read_page(
            file,
            PageId {
                ext_no: loc.ext_no,
                page_no: loc.page_no,
            },
        )?;
        if start_page.len() >= 4 {
            let page_type = u32::from_be_bytes(start_page[0..4].try_into().unwrap());
            if page_type != 5 && page_type != 7 {
                return Err(EngineError::Format(format!(
                    "record 起始页 {} 类型非数据页: type={}",
                    loc.page_no, page_type
                )));
            }
        }

        let start_offset = loc.page_no as u64 * page_size as u64 + loc.byte_offset as u64;
        let file_len = file.metadata()?.len();
        let initial_available = file_len.saturating_sub(start_offset) as usize;
        if initial_available == 0 {
            return Err(EngineError::Format(format!(
                "record offset 超出文件尾: {:#X}",
                start_offset
            )));
        }

        let mut target = Self::INITIAL.min(initial_available);
        let mut data = Self::read_window(file, store, start_offset, target)?;

        loop {
            if let Some(end) = Self::find_record_end(&data)? {
                data.truncate(end);
                return Ok(data);
            }

            if target >= Self::MAX {
                return Err(EngineError::Format(format!(
                    "record 超出上限 {}B (start={:#X})",
                    Self::MAX,
                    start_offset
                )));
            }

            target = (target * 2).min(Self::MAX).min(initial_available);
            let need = target.saturating_sub(data.len());
            if need == 0 {
                return Ok(data);
            }

            let already = data.len() as u64;
            let available = file_len.saturating_sub(start_offset + already) as usize;
            if available == 0 {
                return Ok(data);
            }

            let more = Self::read_window(file, store, start_offset + already, need.min(available))?;
            data.extend_from_slice(&more);
        }
    }

    fn read_window(
        file: &mut File,
        store: &mut PageStore,
        start_offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, EngineError> {
        let mut result = Vec::with_capacity(length);
        let mut remaining = length;
        let mut current = start_offset;

        while remaining > 0 {
            let page_size = store.page_size();
            let page_no = (current / page_size as u64) as u32;
            let offset_in_page = (current % page_size as u64) as usize;
            let to_read = remaining.min(page_size - offset_in_page);
            let page = store.read_page(file, PageId { ext_no: 1, page_no })?;
            result.extend_from_slice(&page[offset_in_page..offset_in_page + to_read]);
            current += to_read as u64;
            remaining -= to_read;
        }

        Ok(result)
    }

    fn find_record_end(data: &[u8]) -> Result<Option<usize>, EngineError> {
        let prefix = Self::skip_padding_len(data);
        if prefix + 4 > data.len() {
            return Ok(None);
        }

        let impl_len_words = i32::from_be_bytes(data[prefix..prefix + 4].try_into().unwrap());
        if impl_len_words <= 0 {
            return Err(EngineError::Format(format!(
                "impl_len 非法: {}",
                impl_len_words
            )));
        }

        let declared_impl_len = impl_len_words as usize * 4;
        if prefix + declared_impl_len > data.len() {
            return Ok(None);
        }

        let actual_impl_len = Self::extend_impl_len(declared_impl_len, &data[prefix..]);
        let mut pos = prefix + actual_impl_len;

        while pos + 4 <= data.len() {
            if pos + 8 <= data.len()
                && data[pos..pos + 4] == Self::PADDING_ZERO
                && data[pos + 4..pos + 8] == Self::PADDING_SEVEN
            {
                return Ok(Some(pos + 8));
            }

            if data[pos..pos + 4] == Self::PADDING_SEVEN {
                let looks_like_segment = pos + 6 <= data.len()
                    && data[pos + 4] == 0x00
                    && (data[pos + 5] == 0x01 || data[pos + 5] == 0x02);
                if !looks_like_segment {
                    return Ok(Some(pos + 4));
                }
                pos += 4;
                continue;
            }

            if data[pos..pos + 4] == Self::PADDING_ZERO {
                pos += 4;
                continue;
            }

            let flag = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let len_words = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
            if flag == 0x0001 || flag == 0x0002 {
                if len_words == 0 {
                    return Err(EngineError::Format(format!("block len_words=0 at {}", pos)));
                }
                let block_len = len_words * 4;
                if pos + block_len > data.len() {
                    return Ok(None);
                }
                pos += block_len;
                pos = match Self::advance_over_segments(data, pos, flag as u8) {
                    Some(next) => next,
                    None => return Ok(None),
                };
                continue;
            }

            return Ok(Some(pos));
        }

        Ok(None)
    }

    fn skip_padding_len(input: &[u8]) -> usize {
        let mut pos = 0;
        while pos + 4 <= input.len() {
            let next = &input[pos..pos + 4];
            if next == Self::PADDING_ZERO || next == Self::PADDING_SEVEN {
                pos += 4;
            } else {
                break;
            }
        }
        pos
    }

    fn extend_impl_len(mut declared: usize, input: &[u8]) -> usize {
        while declared + 4 <= input.len() {
            let next = &input[declared..declared + 4];
            if next == Self::PADDING_ZERO || next == Self::PADDING_SEVEN {
                declared += 4;
            } else {
                break;
            }
        }
        declared
    }

    fn advance_over_segments(data: &[u8], mut pos: usize, flag: u8) -> Option<usize> {
        while pos + 8 <= data.len()
            && data[pos..pos + 4] == Self::PADDING_SEVEN
            && data[pos + 4] == 0x00
            && data[pos + 5] == flag
        {
            let seg_len_words = u16::from_be_bytes([data[pos + 6], data[pos + 7]]) as usize;
            if seg_len_words == 0 {
                return None;
            }
            let seg_total = seg_len_words * 4 + 4;
            if pos + seg_total > data.len() {
                return None;
            }
            pos += seg_total;
        }
        Some(pos)
    }
}

pub fn read_record_from_loc(
    file: &mut File,
    store: &mut PageStore,
    loc: RecordLoc,
) -> Result<Vec<u8>, EngineError> {
    RecordReaderV2::read_record(file, store, loc)
}
