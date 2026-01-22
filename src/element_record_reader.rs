use std::fs::File;

use anyhow::{anyhow, Result};

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
        const MAX: usize = 1024 * 1024;

        let mut buf_size = INITIAL;
        loop {
            let data = PagedReader::read(file, page_cache, ext_no, page_size, start_offset, buf_size)?;
            if let Some(end) = Self::find_record_end(&data)? {
                let mut out = data;
                out.truncate(end);
                return Ok(out);
            }

            if buf_size >= MAX {
                return Ok(data);
            }
            buf_size = (buf_size * 2).min(MAX);
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

            let flag = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let len_words = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;

            if flag == 0x0001 || flag == 0x0002 {
                if len_words == 0 {
                    return Err(anyhow!("block len_words=0 at pos={}", pos));
                }
                let block_len = len_words * 4;
                if pos + block_len > data.len() {
                    return Ok(None);
                }
                pos += block_len;

                pos = match Self::advance_over_segments(data, pos, flag as u8) {
                    Some(p) => p,
                    None => return Ok(None),
                };
                continue;
            }

            // 到了这里既不是 padding / end marker，也不是 0x0001/0x0002 块头：
            // 在 PDMS 元素格式里，这通常意味着“下一条元素记录”已经开始（例如 impl_len_words）。
            // 相比继续盲扫（可能吞入后续记录/页面），这里直接认为当前元素结束更稳妥。
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
        let out = PagedReader::read(&mut file, &mut pm, 0, page_size, (page_size - 10) as u64, 20).unwrap();

        assert_eq!(&out[..10], vec![b'A'; 10]);
        assert_eq!(&out[10..], vec![b'B'; 10]);

        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_element_record_reader_end_marker() {
        let page_size = 0x800usize;
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("pdms_io_test_element_record.bin");
        let _ = std::fs::remove_file(&temp_file);

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&temp_file)
            .unwrap();

        let start_offset = (page_size - 32) as u64;
        let mut buf = vec![0u8; page_size * 16];

        let padding = [0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x07];
        let impl_len_words: i32 = 6; // 24 bytes
        let mut implicit = vec![0u8; impl_len_words as usize * 4];
        implicit[0..4].copy_from_slice(&impl_len_words.to_be_bytes());

        let end_marker = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07];

        let record = [padding.as_slice(), implicit.as_slice(), end_marker.as_slice()].concat();
        write_at(&mut buf, start_offset as usize, &record);

        file.write_all(&buf).unwrap();
        file.flush().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();

        let mut pm = PageManager::new(64, page_size);
        let out = ElementRecordReader::read(&mut file, &mut pm, 0, page_size, start_offset).unwrap();
        assert_eq!(out, record);

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

        let record = [padding.as_slice(), implicit.as_slice(), members.as_slice(), seg.as_slice(), end_marker.as_slice()].concat();
        write_at(&mut buf, start_offset as usize, &record);

        file.write_all(&buf).unwrap();
        file.flush().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();

        let mut pm = PageManager::new(128, page_size);
        let out = ElementRecordReader::read(&mut file, &mut pm, 0, page_size, start_offset).unwrap();
        assert_eq!(out, record);

        let _ = std::fs::remove_file(&temp_file);
    }
}
