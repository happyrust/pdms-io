//! 显式属性解析器
//!
//! 提供 PDMS 显式属性的解析功能，包括：
//! - 属性类型判断
//! - 属性头部解析
//! - 属性值解析

use aios_core::pdms_types::DbAttributeType;
use nom::IResult;
use nom::Parser;
use nom::number::complete::{be_i32, be_u16};

/// 显式属性类型映射
///
/// 根据类型标识码返回对应的属性类型
#[inline]
pub fn get_explicit_attr_type(type_code: u16) -> Option<DbAttributeType> {
    use DbAttributeType::*;
    match type_code {
        0x3C00 | 0x2800 => Some(STRING),
        0x1800 => Some(DOUBLEVEC),
        0x1C00 => Some(INTVEC),
        0x4000 | 0x1000 => Some(ELEMENT),
        0x0C00 => Some(INTEGER),
        0x1400 => Some(BOOL),
        0x0800 => Some(DOUBLE),
        0x3800 => Some(TYPEX),
        0x2000 => Some(RefU64Vec),
        0x0000 => None,
        _ => None,
    }
}

/// 显式属性头部
#[derive(Debug, Clone)]
pub struct ExplicitAttrHeader {
    /// 属性哈希值
    pub hash: i32,
    /// 属性类型码
    pub type_code: u16,
    /// 属性长度（word 数）
    pub length: u16,
}

impl ExplicitAttrHeader {
    /// 获取数据长度（字节）
    pub fn data_len(&self) -> usize {
        self.length as usize * 4
    }

    /// 获取属性类型
    pub fn attr_type(&self) -> Option<DbAttributeType> {
        get_explicit_attr_type(self.type_code)
    }
}

/// 解析显式属性头部
///
/// # 格式（8 字节）
/// - bytes[0..4]: 属性哈希值 (i32)
/// - bytes[4..6]: 属性类型码 (u16)
/// - bytes[6..8]: 属性长度 (u16, word 数)
pub fn parse_explicit_header(input: &[u8]) -> IResult<&[u8], ExplicitAttrHeader> {
    let (input, (hash, type_code, length)) = (be_i32, be_u16, be_u16).parse(input)?;

    Ok((
        input,
        ExplicitAttrHeader {
            hash,
            type_code,
            length,
        },
    ))
}

/// 常用显式属性哈希值
pub mod hashes {
    pub const ATT_PX: i32 = 0xFFF7E177u32 as i32;
    pub const ATT_PY: i32 = 0xFFF7E15Cu32 as i32;
    pub const ATT_PZ: i32 = 0xFFF7E141u32 as i32;
    pub const ATT_PDIA: i32 = 0xFFF77D0Fu32 as i32;
    pub const ATT_PHEI: i32 = 0xFFF520EFu32 as i32;
    pub const ATT_PDIS: i32 = 0xFFF21519u32 as i32;
    pub const ATT_PCON: i32 = 0xFFF3848Du32 as i32;
    pub const ATT_PBOR: i32 = 0xFFF2511Cu32 as i32;
    pub const ATT_PPRO: i32 = 0xFFF32DC0u32 as i32;
    pub const ATT_DPRO: i32 = 0xFFF32DCCu32 as i32;
    pub const ATT_BTHK: i32 = 0xFFF47D68u32 as i32;
    pub const ATT_PTCDI: i32 = 0x95A34;
}

/// 检查是否为位置相关属性
#[inline]
pub fn is_position_attr(hash: i32) -> bool {
    use hashes::*;
    matches!(hash, ATT_PX | ATT_PY | ATT_PZ)
}

/// 检查是否为尺寸相关属性
#[inline]
pub fn is_dimension_attr(hash: i32) -> bool {
    use hashes::*;
    matches!(hash, ATT_PDIA | ATT_PHEI | ATT_PDIS)
}

/// 解析数值类型的显式属性值
///
/// # 参数
/// - `input`: 属性数据
/// - `header`: 已解析的属性头部
pub fn parse_explicit_number<'a>(
    input: &'a [u8],
    header: &ExplicitAttrHeader,
) -> IResult<&'a [u8], f64> {
    use crate::parser::numeric::{
        parse_explicit_f64_40, parse_explicit_num_00, parse_explicit_num_ff,
    };

    let data_len = header.data_len();
    if input.len() < data_len || data_len < 12 {
        return Err(nom::Err::Incomplete(nom::Needed::new(12)));
    }

    let data = &input[..data_len];

    // 根据标志位选择解析器
    let flag = if data.len() >= 10 {
        i16::from_be_bytes(data[8..10].try_into().unwrap_or([0; 2]))
    } else {
        0
    };

    let value = match flag {
        0 => parse_explicit_num_00(data).map(|(_, v)| v).unwrap_or(0.0),
        0x4000 => parse_explicit_f64_40(data).map(|(_, v)| v).unwrap_or(0.0),
        -1 => parse_explicit_num_ff(data).map(|(_, v)| v).unwrap_or(0.0),
        _ => 0.0,
    };

    Ok((&input[data_len..], value))
}

/// 解析字符串类型的显式属性值
pub fn parse_explicit_string<'a>(
    input: &'a [u8],
    header: &ExplicitAttrHeader,
) -> IResult<&'a [u8], String> {
    let data_len = header.data_len();
    if input.len() < data_len {
        return Err(nom::Err::Incomplete(nom::Needed::new(
            data_len - input.len(),
        )));
    }

    let data = &input[..data_len];

    // 字符串以每个字符占 4 字节存储
    let string: String = data
        .chunks(4)
        .filter_map(|chunk| {
            if chunk.len() == 4 {
                let val = i32::from_be_bytes(chunk.try_into().unwrap());
                if val > 0 && val < 128 {
                    Some(val as u8 as char)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    Ok((
        &input[data_len..],
        string.trim_end_matches('\0').to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_explicit_attr_type() {
        assert_eq!(
            get_explicit_attr_type(0x3C00),
            Some(DbAttributeType::STRING)
        );
        assert_eq!(
            get_explicit_attr_type(0x0800),
            Some(DbAttributeType::DOUBLE)
        );
        assert_eq!(
            get_explicit_attr_type(0x0C00),
            Some(DbAttributeType::INTEGER)
        );
        assert_eq!(get_explicit_attr_type(0x0000), None);
    }

    #[test]
    fn test_parse_explicit_header() {
        let input = [
            0xFF, 0xF7, 0xE1, 0x77, // hash (ATT_PX)
            0x08, 0x00, // type code (DOUBLE)
            0x00, 0x03, // length (3 words)
        ];
        let (rest, header) = parse_explicit_header(&input).unwrap();
        assert!(rest.is_empty());
        assert_eq!(header.hash, hashes::ATT_PX);
        assert_eq!(header.type_code, 0x0800);
        assert_eq!(header.length, 3);
        assert_eq!(header.data_len(), 12);
    }

    #[test]
    fn test_is_position_attr() {
        assert!(is_position_attr(hashes::ATT_PX));
        assert!(is_position_attr(hashes::ATT_PY));
        assert!(is_position_attr(hashes::ATT_PZ));
        assert!(!is_position_attr(hashes::ATT_PDIA));
    }

    #[test]
    fn test_is_dimension_attr() {
        assert!(is_dimension_attr(hashes::ATT_PDIA));
        assert!(is_dimension_attr(hashes::ATT_PHEI));
        assert!(!is_dimension_attr(hashes::ATT_PX));
    }
}
