use crate::core::{EngineError, RefNo};
use crate::db4::attrs::AttrValue;

const EXPLICIT_FLAG: u16 = 0x0001;
const MEMBERS_FLAG: u16 = 0x0002;

#[derive(Debug, Clone)]
pub struct ExplicitBlock {
    pub flag: u8,
    pub hash: u32,
    pub self_ref: RefNo,
    pub payload: Vec<u8>,
}

pub fn parse_explicit_blocks(data: &[u8]) -> Result<Vec<ExplicitBlock>, EngineError> {
    let mut blocks = Vec::new();
    let mut pos = 0;

    while pos + 4 <= data.len() {
        if pos + 8 <= data.len()
            && data[pos..pos + 4] == [0, 0, 0, 0]
            && data[pos + 4..pos + 8] == [0, 0, 0, 7]
        {
            break;
        }

        if data[pos..pos + 4] == [0, 0, 0, 0] || data[pos..pos + 4] == [0, 0, 0, 7] {
            pos += 4;
            continue;
        }

        let flag = u16::from_be_bytes([data[pos], data[pos + 1]]);
        if flag != EXPLICIT_FLAG && flag != MEMBERS_FLAG {
            break;
        }

        let len_words = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        if len_words == 0 {
            break;
        }

        let block_bytes = len_words * 4;
        if pos + block_bytes > data.len() {
            break;
        }

        let block_data = &data[pos..pos + block_bytes];
        if block_data.len() < 16 {
            pos += block_bytes;
            continue;
        }

        let hash = u32::from_be_bytes(block_data[4..8].try_into().unwrap());
        let self_ref = RefNo::from_parts(
            u32::from_be_bytes(block_data[8..12].try_into().unwrap()),
            u32::from_be_bytes(block_data[12..16].try_into().unwrap()),
        );

        let mut payload = block_data[16..].to_vec();

        let mut next_pos = pos + block_bytes;
        while next_pos + 8 <= data.len()
            && data[next_pos..next_pos + 4] == [0, 0, 0, 7]
            && data[next_pos + 4] == 0x00
            && data[next_pos + 5] == flag as u8
        {
            let seg_words =
                u16::from_be_bytes([data[next_pos + 6], data[next_pos + 7]]) as usize;
            if seg_words == 0 {
                break;
            }
            let seg_total = seg_words * 4 + 4;
            if next_pos + seg_total > data.len() {
                break;
            }
            let seg_header_size = 8 + 12;
            if next_pos + seg_header_size < next_pos + seg_total {
                payload.extend_from_slice(&data[next_pos + seg_header_size..next_pos + seg_total]);
            }
            next_pos += seg_total;
        }

        blocks.push(ExplicitBlock {
            flag: flag as u8,
            hash,
            self_ref,
            payload,
        });

        pos = next_pos;
    }

    Ok(blocks)
}

pub fn read_explicit_integer(payload: &[u8], word_offset: usize) -> Option<i32> {
    let byte_offset = word_offset * 4;
    if byte_offset + 4 > payload.len() {
        return None;
    }
    Some(i32::from_be_bytes(
        payload[byte_offset..byte_offset + 4].try_into().unwrap(),
    ))
}

pub fn read_explicit_real_f64(payload: &[u8], word_offset: usize) -> Option<f64> {
    let byte_offset = word_offset * 4;
    if byte_offset + 8 > payload.len() {
        return None;
    }
    Some(f64::from_be_bytes(
        payload[byte_offset..byte_offset + 8].try_into().unwrap(),
    ))
}

pub fn read_explicit_string(payload: &[u8]) -> String {
    if payload.len() < 4 {
        return String::new();
    }
    let str_len_words = u32::from_be_bytes(payload[0..4].try_into().unwrap()) as usize;
    let str_bytes = str_len_words * 4;
    if str_bytes == 0 || 4 + str_bytes > payload.len() {
        return String::new();
    }
    let raw = &payload[4..4 + str_bytes];
    String::from_utf8_lossy(raw).trim_end_matches('\0').to_string()
}

pub fn read_explicit_reference(payload: &[u8], word_offset: usize) -> Option<RefNo> {
    let byte_offset = word_offset * 4;
    if byte_offset + 8 > payload.len() {
        return None;
    }
    let hi = u32::from_be_bytes(
        payload[byte_offset..byte_offset + 4].try_into().unwrap(),
    );
    let lo = u32::from_be_bytes(
        payload[byte_offset + 4..byte_offset + 8]
            .try_into()
            .unwrap(),
    );
    Some(RefNo::from_parts(hi, lo))
}

pub fn explicit_block_to_attr_value(
    block: &ExplicitBlock,
    is_string: bool,
) -> AttrValue {
    if is_string {
        AttrValue::String(read_explicit_string(&block.payload))
    } else if block.payload.len() >= 8 {
        if let Some(r) = read_explicit_reference(&block.payload, 0) {
            AttrValue::Reference(r)
        } else {
            AttrValue::Raw(block.payload.clone())
        }
    } else if let Some(v) = read_explicit_integer(&block.payload, 0) {
        AttrValue::Integer(v)
    } else {
        AttrValue::Raw(block.payload.clone())
    }
}
