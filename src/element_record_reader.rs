use std::fs::File;

use anyhow::{Result, anyhow};

use crate::page_manager::PageManager;
use crate::paged_reader::PagedReader;

pub struct ElementRecordReader;

impl ElementRecordReader {
    const PADDING_ZERO: [u8; 4] = [0x00, 0x00, 0x00, 0x00];
    const PADDING_SEVEN: [u8; 4] = [0x00, 0x00, 0x00, 0x07];

    pub fn read(
        file: &mut File,
        page_cache: &mut PageManager,
        ext_no: u32,
        page_size: usize,
        start_offset: u64,
    ) -> Result<Vec<u8>> {
        const INITIAL: usize = 16 * 1024;
        const MAX: usize = 64 * 1024;

        let file_len = file.metadata().map(|m| m.len()).unwrap_or(u64::MAX);

        let initial_available = file_len.saturating_sub(start_offset) as usize;
        if initial_available == 0 {
            return Err(anyhow!(
                "element record start_offset beyond EOF (start_offset={:#X}, file_len={:#X})",
                start_offset,
                file_len
            ));
        }

        let mut target = INITIAL.min(initial_available);
        let mut data =
            PagedReader::read(file, page_cache, ext_no, page_size, start_offset, target)?;

        loop {
            if let Some(end) = Self::find_record_end(&data)? {
                data.truncate(end);
                return Ok(data);
            }

            if target >= MAX {
                return Ok(data);
            }

            target = (target * 2).min(MAX);
            let need = target.saturating_sub(data.len());
            if need == 0 {
                return Ok(data);
            }

            // 仅追加读取“新增”部分，避免每轮扩容都从起点重读。
            let already = data.len() as u64;
            let available = file_len.saturating_sub(start_offset + already) as usize;
            if available == 0 {
                return Ok(data);
            }
            let to_read = need.min(available);
            let more = PagedReader::read(
                file,
                page_cache,
                ext_no,
                page_size,
                start_offset + already,
                to_read,
            )?;
            data.extend_from_slice(&more);
        }
    }

    fn find_record_end(data: &[u8]) -> Result<Option<usize>> {
        let prefix = Self::skip_padding_len(data);
        if prefix + 4 > data.len() {
            return Ok(None);
        }

        let impl_len_words = i32::from_be_bytes(data[prefix..prefix + 4].try_into().unwrap());
        if impl_len_words <= 0 {
            return Err(anyhow!("impl_len 非法: {}", impl_len_words));
        }

        let declared_impl_len = impl_len_words as usize * 4;
        if prefix + declared_impl_len > data.len() {
            return Ok(None);
        }

        let actual_impl_len = Self::extend_impl_len(declared_impl_len, &data[prefix..]);
        let mut pos = prefix + actual_impl_len;
        let mut saw_explicit_block = false;

        while pos + 4 <= data.len() {
            // 优先识别“明确”的结束标记：00 00 00 00 + 00 00 00 07
            if pos + 8 <= data.len()
                && &data[pos..pos + 4] == &Self::PADDING_ZERO
                && &data[pos + 4..pos + 8] == &Self::PADDING_SEVEN
            {
                return Ok(Some(pos + 8));
            }

            // 单独的 0x00000007：可能是 padding，也可能是 0x07 追加段的起始。
            // 只有在它“看起来不像追加段”时，才当作结束标记。
            if &data[pos..pos + 4] == &Self::PADDING_SEVEN {
                let looks_like_segment = pos + 6 <= data.len()
                    && data[pos + 4] == 0x00
                    && (data[pos + 5] == 0x01 || data[pos + 5] == 0x02);
                if !looks_like_segment {
                    return Ok(Some(pos + 4));
                }
                // 追加段/填充：向后推进一个 word 再继续判定
                pos += 4;
                continue;
            }

            // 0x00000000/0x00000007 的 padding
            if &data[pos..pos + 4] == &Self::PADDING_ZERO {
                pos += 4;
                continue;
            }

            if saw_explicit_block && Self::looks_like_element_record_start(data, pos) {
                return Ok(Some(pos));
            }

            let flag = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let len_words = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;

            if flag == 0x0001 || flag == 0x0002 {
                if len_words == 0 {
                    pos += 4;
                    continue;
                }
                let block_len = len_words * 4;
                if pos + block_len > data.len() {
                    return Ok(None);
                }
                pos += block_len;
                if flag == 0x0001 {
                    saw_explicit_block = true;
                }

                pos = match Self::advance_over_segments(data, pos, flag as u8) {
                    Some(p) => p,
                    None => return Ok(None),
                };
                continue;
            }

            // 有些 E3D 记录在 implicit/member 区和后续显式块之间夹着当前解析器
            // 尚不认识的 word。不能在第一个未知 word 就截断，否则会丢掉后面的
            // explicit block（例如 PHEI 可能在十几 KB 之后）。
            pos += 4;
        }

        Ok(None)
    }

    fn looks_like_element_record_start(data: &[u8], pos: usize) -> bool {
        if pos + 16 > data.len() {
            return false;
        }

        let impl_len_words = i32::from_be_bytes(data[pos..pos + 4].try_into().unwrap());
        if impl_len_words <= 0 {
            return false;
        }

        let Some(impl_len) = (impl_len_words as usize).checked_mul(4) else {
            return false;
        };
        let Some(end) = pos.checked_add(impl_len) else {
            return false;
        };
        if impl_len < 16 || end > data.len() {
            return false;
        }

        let refno = &data[pos + 4..pos + 12];
        if refno.iter().all(|&b| b == 0) {
            return false;
        }

        let noun_hash = i32::from_be_bytes(data[pos + 12..pos + 16].try_into().unwrap());
        noun_hash != 0
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

    fn extend_impl_len(declared: usize, input: &[u8]) -> usize {
        let mut actual = declared;
        while actual + 4 <= input.len() {
            let next = &input[actual..actual + 4];
            if next == Self::PADDING_ZERO || next == Self::PADDING_SEVEN {
                actual += 4;
            } else {
                break;
            }
        }
        actual
    }

    #[allow(dead_code)]
    fn has_next_block_header(data: &[u8], pos: usize) -> bool {
        if pos + 2 > data.len() {
            return false;
        }
        let flag = u16::from_be_bytes([data[pos], data[pos + 1]]);
        flag == 0x0001 || flag == 0x0002
    }

    fn advance_over_segments(data: &[u8], mut pos: usize, flag: u8) -> Option<usize> {
        while pos + 8 <= data.len()
            && &data[pos..pos + 4] == &Self::PADDING_SEVEN
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page_manager::PageManager;
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom, Write};

    fn write_at(buf: &mut [u8], offset: usize, data: &[u8]) {
        buf[offset..offset + data.len()].copy_from_slice(data);
    }

    #[test]
    fn test_paged_reader_cross_page() {
        let page_size = 0x800usize;
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_test_paged_reader.bin");
        let _ = std::fs::remove_file(&temp_file);

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&temp_file)
            .unwrap();

        let mut content = vec![0u8; page_size * 4];
        for i in 0..page_size {
            content[i] = b'A';
        }
        for i in page_size..page_size * 2 {
            content[i] = b'B';
        }
        file.write_all(&content).unwrap();
        file.flush().unwrap();

        file.seek(SeekFrom::Start(0)).unwrap();
        let mut pm = PageManager::new(16, page_size);
        let out = PagedReader::read(
            &mut file,
            &mut pm,
            0,
            page_size,
            (page_size - 10) as u64,
            20,
        )
        .unwrap();

        assert_eq!(&out[..10], vec![b'A'; 10]);
        assert_eq!(&out[10..], vec![b'B'; 10]);

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_element_record_reader_members_with_segment() {
        let page_size = 0x800usize;
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_test_element_record_segment.bin");
        let _ = std::fs::remove_file(&temp_file);

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&temp_file)
            .unwrap();

        let start_offset = (page_size - 64) as u64;
        let mut buf = vec![0u8; page_size * 32];

        let padding = [0x00, 0x00, 0x00, 0x07];
        let impl_len_words: i32 = 6; // 24 bytes
        let mut implicit = vec![0u8; impl_len_words as usize * 4];
        implicit[0..4].copy_from_slice(&impl_len_words.to_be_bytes());

        let members_len_words: u16 = 5; // 20 bytes
        let mut members = vec![0u8; members_len_words as usize * 4];
        members[0..2].copy_from_slice(&0x0002u16.to_be_bytes());
        members[2..4].copy_from_slice(&members_len_words.to_be_bytes());

        let seg_len_words: u16 = 6; // total = 6*4+4 = 28 bytes
        let mut seg = vec![0u8; seg_len_words as usize * 4 + 4];
        seg[0..4].copy_from_slice(&[0x00, 0x00, 0x00, 0x07]);
        seg[4] = 0x00;
        seg[5] = 0x02;
        seg[6..8].copy_from_slice(&seg_len_words.to_be_bytes());

        let end_marker = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07];

        let record = [
            padding.as_slice(),
            implicit.as_slice(),
            members.as_slice(),
            seg.as_slice(),
            end_marker.as_slice(),
        ]
        .concat();
        write_at(&mut buf, start_offset as usize, &record);

        file.write_all(&buf).unwrap();
        file.flush().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();

        let mut pm = PageManager::new(128, page_size);
        let out =
            ElementRecordReader::read(&mut file, &mut pm, 0, page_size, start_offset).unwrap();
        assert_eq!(out, record);

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_element_record_reader_grow_buffer_incrementally() {
        let page_size = 0x800usize;
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_test_element_record_grow.bin");
        let _ = std::fs::remove_file(&temp_file);

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&temp_file)
            .unwrap();

        // 构造一个超过 INITIAL(16KiB) 的 record，确保触发“增量追加读取”分支。
        let start_offset = 0u64;
        let mut buf = vec![0u8; page_size * 32];

        let padding = [0x00, 0x00, 0x00, 0x07];
        let impl_len_words: i32 = 6; // 24 bytes
        let mut implicit = vec![0u8; impl_len_words as usize * 4];
        implicit[0..4].copy_from_slice(&impl_len_words.to_be_bytes());

        // 一个很大的 members 块，使 record 长度 > 16KiB。
        let members_len_words: u16 = 5000; // 20000 bytes
        let mut members = vec![0u8; members_len_words as usize * 4];
        members[0..2].copy_from_slice(&0x0002u16.to_be_bytes());
        members[2..4].copy_from_slice(&members_len_words.to_be_bytes());

        let end_marker = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07];

        let record = [
            padding.as_slice(),
            implicit.as_slice(),
            members.as_slice(),
            end_marker.as_slice(),
        ]
        .concat();
        write_at(&mut buf, start_offset as usize, &record);

        file.write_all(&buf).unwrap();
        file.flush().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();

        let mut pm = PageManager::new(128, page_size);
        let out =
            ElementRecordReader::read(&mut file, &mut pm, 0, page_size, start_offset).unwrap();
        assert_eq!(out, record);

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_element_record_reader_stops_before_adjacent_record_without_padding() {
        let page_size = 0x800usize;
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_test_adjacent_records.bin");
        let _ = std::fs::remove_file(&temp_file);

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&temp_file)
            .unwrap();

        let start_offset = 0u64;
        let mut buf = vec![0u8; page_size * 2];

        let impl_len_words: i32 = 6; // 24 bytes
        let mut implicit = vec![0u8; impl_len_words as usize * 4];
        implicit[0..4].copy_from_slice(&impl_len_words.to_be_bytes());
        implicit[4..12].copy_from_slice(&0x0000_0001_0000_0001u64.to_be_bytes());

        let explicit_len_words: u16 = 5; // 20 bytes
        let mut explicit = vec![0u8; explicit_len_words as usize * 4];
        explicit[0..2].copy_from_slice(&0x0001u16.to_be_bytes());
        explicit[2..4].copy_from_slice(&explicit_len_words.to_be_bytes());
        explicit[4..12].copy_from_slice(&0x0000_0001_0000_0001u64.to_be_bytes());
        explicit[12..16].copy_from_slice(&0x00CC_6B3Fu32.to_be_bytes());
        explicit[16..20].copy_from_slice(&0x3800_0002u32.to_be_bytes());

        let first_record = [implicit.as_slice(), explicit.as_slice()].concat();

        let mut next_record = vec![0u8; impl_len_words as usize * 4];
        next_record[0..4].copy_from_slice(&impl_len_words.to_be_bytes());
        next_record[4..12].copy_from_slice(&0x0000_0001_0000_0002u64.to_be_bytes());
        next_record[12..16].copy_from_slice(&0x000C_2E93u32.to_be_bytes());
        let end_marker = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07];

        let file_bytes = [
            first_record.as_slice(),
            next_record.as_slice(),
            end_marker.as_slice(),
        ]
        .concat();
        write_at(&mut buf, start_offset as usize, &file_bytes);

        file.write_all(&buf).unwrap();
        file.flush().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();

        let mut pm = PageManager::new(128, page_size);
        let out =
            ElementRecordReader::read(&mut file, &mut pm, 0, page_size, start_offset).unwrap();
        assert_eq!(out, first_record);

        let _ = std::fs::remove_file(&temp_file);
    }
}
