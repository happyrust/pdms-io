use std::fs::File;

use crate::core::{EngineError, PageId, RecordLoc, RecordWriteResult};
use crate::db1::PageStore;

pub const DATA_PAGE_TYPE: u32 = 5;
pub const MAIN_DATA_SUBTYPE: u32 = 0x0074_3F49;
pub const SPECIAL_PAGE_TYPE: u32 = 7;
pub const DATA_PAGE_HEADER_SIZE: usize = 24;
pub const SPECIAL_SEGMENT_HEADER_SIZE: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriterPageKind {
    Data,
    Special,
}

pub struct DataPageBuilderV2 {
    current_page: Vec<u8>,
    current_page_kind: WriterPageKind,
    current_offset: usize,
    current_page_no: u32,
    ext_no: u32,
    page_size: usize,
    written_pages: Vec<(PageId, Vec<u8>)>,
}

pub struct RecordWriterV2;

impl DataPageBuilderV2 {
    pub fn new(page_size: usize, start_page_no: u32, ext_no: u32) -> Self {
        Self {
            current_page: Self::init_data_page(page_size, start_page_no, ext_no),
            current_page_kind: WriterPageKind::Data,
            current_offset: DATA_PAGE_HEADER_SIZE,
            current_page_no: start_page_no,
            ext_no,
            page_size,
            written_pages: Vec::new(),
        }
    }

    fn init_data_page(page_size: usize, page_no: u32, ext_no: u32) -> Vec<u8> {
        let mut page = vec![0u8; page_size];
        page[0..4].copy_from_slice(&DATA_PAGE_TYPE.to_be_bytes());
        page[4..8].copy_from_slice(&MAIN_DATA_SUBTYPE.to_be_bytes());
        page[8..12].copy_from_slice(&ext_no.to_be_bytes());
        page[12..16].copy_from_slice(&page_no.to_be_bytes());
        page[16..20].copy_from_slice(&0u32.to_be_bytes());
        let bucket_id = (MAIN_DATA_SUBTYPE >> 13) & 0x1FFF;
        page[20..24].copy_from_slice(&bucket_id.to_be_bytes());
        page
    }

    fn init_special_page(page_size: usize) -> Vec<u8> {
        vec![0u8; page_size]
    }

    fn current_payload_start(&self) -> usize {
        match self.current_page_kind {
            WriterPageKind::Data => DATA_PAGE_HEADER_SIZE,
            WriterPageKind::Special => SPECIAL_SEGMENT_HEADER_SIZE,
        }
    }

    fn remaining_space(&self) -> usize {
        self.page_size.saturating_sub(self.current_offset)
    }

    fn current_page_has_payload(&self) -> bool {
        self.current_offset > self.current_payload_start()
    }

    fn current_loc(&self) -> RecordLoc {
        RecordLoc {
            ext_no: self.ext_no,
            page_no: self.current_page_no,
            byte_offset: self.current_offset as u32,
        }
    }

    fn push_current_page(&mut self) {
        if self.current_page_has_payload() {
            self.written_pages.push((
                PageId {
                    ext_no: self.ext_no,
                    page_no: self.current_page_no,
                },
                self.current_page.clone(),
            ));
        }
    }

    fn start_new_data_page(&mut self) {
        self.push_current_page();
        self.current_page_no += 1;
        self.current_page_kind = WriterPageKind::Data;
        self.current_page = Self::init_data_page(self.page_size, self.current_page_no, self.ext_no);
        self.current_offset = DATA_PAGE_HEADER_SIZE;
    }

    fn start_special_page(&mut self) {
        self.push_current_page();
        self.current_page_no += 1;
        self.current_page_kind = WriterPageKind::Special;
        self.current_page = Self::init_special_page(self.page_size);
        self.current_offset = 0;
    }

    fn ensure_data_page(&mut self) {
        if self.current_page_kind != WriterPageKind::Data {
            self.start_new_data_page();
        }
    }

    fn append_bytes(&mut self, bytes: &[u8]) {
        self.current_page[self.current_offset..self.current_offset + bytes.len()]
            .copy_from_slice(bytes);
        self.current_offset += bytes.len();
    }

    fn append_segment_header(&mut self, flag: u8, len_words: u16, self_ref: &[u8]) {
        self.append_bytes(&SPECIAL_PAGE_TYPE.to_be_bytes());
        self.current_page[self.current_offset] = 0x00;
        self.current_page[self.current_offset + 1] = flag;
        self.current_page[self.current_offset + 2..self.current_offset + 4]
            .copy_from_slice(&len_words.to_be_bytes());
        self.current_offset += 4;
        self.append_bytes(self_ref);
        self.append_bytes(&0u32.to_be_bytes());
        self.append_bytes(&0u32.to_be_bytes());
    }

    fn skip_padding_len(input: &[u8]) -> usize {
        let mut pos = 0;
        while pos + 4 <= input.len() {
            let next = &input[pos..pos + 4];
            if next == [0, 0, 0, 0] || next == [0, 0, 0, 7] {
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
            if next == [0, 0, 0, 0] || next == [0, 0, 0, 7] {
                actual += 4;
            } else {
                break;
            }
        }
        actual
    }

    fn parse_block_len(record: &[u8], pos: usize) -> Result<Option<(u8, usize)>, EngineError> {
        if pos + 4 > record.len() {
            return Ok(None);
        }
        let flag = u16::from_be_bytes([record[pos], record[pos + 1]]);
        if flag != 0x0001 && flag != 0x0002 {
            return Ok(None);
        }
        let len_words = u16::from_be_bytes([record[pos + 2], record[pos + 3]]) as usize;
        if len_words == 0 {
            return Err(EngineError::Format(format!("block len_words=0 at {}", pos)));
        }
        let block_len = len_words * 4;
        if pos + block_len > record.len() {
            return Err(EngineError::Format(format!(
                "block overruns record boundary at {}",
                pos
            )));
        }
        Ok(Some((flag as u8, block_len)))
    }

    fn write_block_with_segments(
        &mut self,
        flag: u8,
        block: &[u8],
        trailing_reserve: usize,
    ) -> Result<(), EngineError> {
        let self_ref = &block[4..12];
        self.ensure_data_page();

        let mut available = self.remaining_space();
        if available <= trailing_reserve + 12 {
            self.start_new_data_page();
            available = self.remaining_space();
        }

        let must_reserve_for_first_segment =
            if block.len() > available.saturating_sub(trailing_reserve) {
                SPECIAL_SEGMENT_HEADER_SIZE + 4
            } else {
                0
            };
        let effective_available = available
            .saturating_sub(trailing_reserve)
            .saturating_sub(must_reserve_for_first_segment);
        if effective_available < 12 {
            return Err(EngineError::Format(
                "当前数据页空间不足，无法写入 block 首段".into(),
            ));
        }

        let first_chunk_len = block.len().min(effective_available & !0x3);
        if first_chunk_len < 12 {
            return Err(EngineError::Format(
                "block 首段长度不足，无法安全切分".into(),
            ));
        }

        let first_chunk = &block[..first_chunk_len];
        let first_words = (first_chunk_len / 4) as u16;
        let mut first_header = first_chunk[..4].to_vec();
        first_header[2..4].copy_from_slice(&first_words.to_be_bytes());
        self.append_bytes(&first_header);
        self.append_bytes(&first_chunk[4..]);

        let mut remaining = &block[first_chunk_len..];
        while !remaining.is_empty() {
            if self.remaining_space() < SPECIAL_SEGMENT_HEADER_SIZE + 4 {
                self.start_special_page();
            }

            let max_payload = self
                .remaining_space()
                .saturating_sub(SPECIAL_SEGMENT_HEADER_SIZE)
                .saturating_sub(
                    if remaining.len()
                        <= self
                            .remaining_space()
                            .saturating_sub(SPECIAL_SEGMENT_HEADER_SIZE)
                    {
                        trailing_reserve
                    } else {
                        0
                    },
                );
            let payload_len = remaining.len().min(max_payload & !0x3);
            if payload_len == 0 {
                self.start_special_page();
                continue;
            }

            let seg_words = ((payload_len + 20) / 4) as u16;
            self.append_segment_header(flag, seg_words, self_ref);
            self.append_bytes(&remaining[..payload_len]);
            remaining = &remaining[payload_len..];
        }

        Ok(())
    }

    pub fn write_record(&mut self, record: &[u8]) -> Result<RecordWriteResult, EngineError> {
        self.ensure_data_page();

        let prefix = Self::skip_padding_len(record);
        if prefix + 4 > record.len() {
            return Err(EngineError::Format("元素记录缺少 impl_len".into()));
        }

        let impl_len_words = i32::from_be_bytes(record[prefix..prefix + 4].try_into().unwrap());
        if impl_len_words <= 0 {
            return Err(EngineError::Format(format!(
                "impl_len 非法: {}",
                impl_len_words
            )));
        }

        let declared_impl_len = impl_len_words as usize * 4;
        if prefix + declared_impl_len > record.len() {
            return Err(EngineError::Format("隐含区长度超出元素记录边界".into()));
        }
        let implicit_len = prefix + Self::extend_impl_len(declared_impl_len, &record[prefix..]);
        let minimum_tail = 8usize;
        if self.remaining_space() < implicit_len + minimum_tail {
            self.start_new_data_page();
        }
        if self.remaining_space() < implicit_len {
            return Err(EngineError::Format("单个元素隐含区超过数据页容量".into()));
        }

        let start = self.current_loc();
        self.append_bytes(&record[..implicit_len]);

        let mut pos = implicit_len;
        while pos < record.len() {
            if pos + 8 <= record.len()
                && &record[pos..pos + 4] == [0, 0, 0, 0]
                && &record[pos + 4..pos + 8] == [0, 0, 0, 7]
            {
                if self.remaining_space() < 8 {
                    return Err(EngineError::Format("元素结束标记无法落在当前页".into()));
                }
                self.append_bytes(&record[pos..pos + 8]);
                break;
            }

            if let Some((flag, block_len)) = Self::parse_block_len(record, pos)? {
                let trailing_reserve = if pos + block_len + 8 <= record.len()
                    && &record[pos + block_len..pos + block_len + 4] == [0, 0, 0, 0]
                    && &record[pos + block_len + 4..pos + block_len + 8] == [0, 0, 0, 7]
                {
                    8
                } else {
                    0
                };
                self.write_block_with_segments(
                    flag,
                    &record[pos..pos + block_len],
                    trailing_reserve,
                )?;
                pos += block_len;
                continue;
            }

            let remaining = record.len() - pos;
            if self.remaining_space() < remaining {
                return Err(EngineError::Format("元素尾部数据无法安全跨页写入".into()));
            }
            self.append_bytes(&record[pos..]);
            pos = record.len();
        }

        let pages_used = self
            .written_pages
            .len()
            .saturating_add(usize::from(self.current_page_has_payload()));
        let result = RecordWriteResult {
            start,
            total_len: record.len(),
            end_page: PageId {
                ext_no: self.ext_no,
                page_no: self.current_page_no,
            },
            pages_used,
        };

        if result.end_page.page_no != result.start.page_no
            && self.current_page_kind == WriterPageKind::Special
        {
            self.start_new_data_page();
        }

        Ok(result)
    }

    pub fn finish(mut self) -> Vec<(PageId, Vec<u8>)> {
        self.push_current_page();
        self.written_pages
    }
}

impl RecordWriterV2 {
    pub fn write_record(
        file: &mut File,
        store: &mut PageStore,
        ext_no: u32,
        record: &[u8],
    ) -> Result<RecordWriteResult, EngineError> {
        let start_page = store.allocate_page(file, ext_no)?;
        let mut builder = DataPageBuilderV2::new(store.page_size(), start_page.page_no, ext_no);
        let result = builder.write_record(record)?;
        for (page_id, page) in builder.finish() {
            store.write_page(file, page_id, &page)?;
        }
        store.flush_dirty(file)?;
        Ok(result)
    }
}

pub fn write_record(
    file: &mut File,
    store: &mut PageStore,
    ext_no: u32,
    record: &[u8],
) -> Result<RecordWriteResult, EngineError> {
    RecordWriterV2::write_record(file, store, ext_no, record)
}
