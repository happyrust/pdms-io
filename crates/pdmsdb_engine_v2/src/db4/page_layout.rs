use crate::core::{EngineError, RefNo};

pub const ELEMENT_PAGE_TYPE: u32 = 5;
pub const CONTINUATION_PAGE_TYPE: u32 = 7;
pub const ELEMENT_HEADER_SIZE: usize = 24;

#[derive(Debug, Clone)]
pub struct ElementRecordView {
    pub impl_len_words: i32,
    pub refno: RefNo,
    pub noun_hash: u32,
    pub owner: RefNo,
    pub implicit_data: Vec<u8>,
    pub members_data: Vec<u8>,
    pub explicit_data: Vec<u8>,
}

impl ElementRecordView {
    pub fn from_raw(raw: &[u8]) -> Result<Self, EngineError> {
        if raw.len() < ELEMENT_HEADER_SIZE {
            return Err(EngineError::Format(format!(
                "元素记录太短: {} < {}",
                raw.len(),
                ELEMENT_HEADER_SIZE
            )));
        }

        let prefix = skip_padding(raw);
        if prefix + 4 > raw.len() {
            return Err(EngineError::Format("无法读取 impl_len".into()));
        }

        let impl_len_words =
            i32::from_be_bytes(raw[prefix..prefix + 4].try_into().unwrap());
        if impl_len_words <= 0 {
            return Err(EngineError::Format(format!(
                "impl_len 非法: {}",
                impl_len_words
            )));
        }

        let declared_impl_len = impl_len_words as usize * 4;
        if prefix + declared_impl_len > raw.len() {
            return Err(EngineError::Format("impl_len 超出记录边界".into()));
        }

        let refno = RefNo::from_parts(
            u32::from_be_bytes(raw[prefix + 4..prefix + 8].try_into().unwrap()),
            u32::from_be_bytes(raw[prefix + 8..prefix + 12].try_into().unwrap()),
        );
        let noun_hash =
            u32::from_be_bytes(raw[prefix + 12..prefix + 16].try_into().unwrap());
        let owner = RefNo::from_parts(
            u32::from_be_bytes(raw[prefix + 16..prefix + 20].try_into().unwrap()),
            u32::from_be_bytes(raw[prefix + 20..prefix + 24].try_into().unwrap()),
        );

        let mut actual_impl_len = declared_impl_len;
        while actual_impl_len + 4 <= raw.len() - prefix {
            let w = i32::from_be_bytes(
                raw[prefix + actual_impl_len..prefix + actual_impl_len + 4]
                    .try_into()
                    .unwrap(),
            );
            if w != 0 && w != 7 {
                break;
            }
            actual_impl_len += 4;
        }

        let implicit_data = raw[prefix..prefix + actual_impl_len].to_vec();

        let membs_pos = prefix + actual_impl_len;
        let (members_data, members_consumed) = parse_members_region(&raw[membs_pos..], refno);

        let explicit_start = membs_pos + members_consumed;
        let explicit_data = if explicit_start < raw.len() {
            strip_terminal_padding(&raw[explicit_start..])
        } else {
            Vec::new()
        };

        Ok(Self {
            impl_len_words,
            refno,
            noun_hash,
            owner,
            implicit_data,
            members_data,
            explicit_data,
        })
    }
}

fn skip_padding(input: &[u8]) -> usize {
    let mut pos = 0;
    while pos + 4 <= input.len() {
        let w = &input[pos..pos + 4];
        if w == [0, 0, 0, 0] || w == [0, 0, 0, 7] {
            pos += 4;
        } else {
            break;
        }
    }
    pos
}

fn parse_members_region(data: &[u8], expected_refno: RefNo) -> (Vec<u8>, usize) {
    if data.len() < 14 {
        return (Vec::new(), 0);
    }

    let maybe_refno = RefNo::from_parts(
        u32::from_be_bytes(data[4..8].try_into().unwrap()),
        u32::from_be_bytes(data[8..12].try_into().unwrap()),
    );

    if maybe_refno != expected_refno {
        return (Vec::new(), 0);
    }

    if data[0] != 0x00 || data[1] != 0x02 {
        return (Vec::new(), 0);
    }

    let declared_words = u16::from_be_bytes([data[2], data[3]]) as usize;
    let declared_bytes = declared_words * 4;
    if declared_bytes > data.len() {
        return (data[..declared_bytes.min(data.len())].to_vec(), declared_bytes.min(data.len()));
    }

    let mut consumed = declared_bytes;
    let mut merged = data[..declared_bytes].to_vec();

    while consumed + 8 <= data.len()
        && data[consumed..consumed + 4] == [0, 0, 0, 7]
        && data[consumed + 4] == 0x00
        && data[consumed + 5] == 0x02
    {
        let seg_words = u16::from_be_bytes([data[consumed + 6], data[consumed + 7]]) as usize;
        if seg_words == 0 {
            break;
        }
        let seg_total = seg_words * 4 + 4;
        if consumed + seg_total > data.len() {
            break;
        }
        let payload_start = consumed + 8 + 12;
        let payload_end = consumed + seg_total;
        if payload_start < payload_end && payload_end <= data.len() {
            merged.extend_from_slice(&data[payload_start..payload_end]);
        }
        consumed += seg_total;
    }

    (merged, consumed)
}

fn strip_terminal_padding(data: &[u8]) -> Vec<u8> {
    let mut end = data.len();
    while end >= 4 {
        let w = &data[end - 4..end];
        if w == [0, 0, 0, 0] || w == [0, 0, 0, 7] {
            end -= 4;
        } else {
            break;
        }
    }
    data[..end].to_vec()
}
