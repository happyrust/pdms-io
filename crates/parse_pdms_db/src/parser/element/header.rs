//! 元素头部解析器
//!
//! 提供 PDMS 元素头部的解析功能，包括：
//! - 隐含区长度
//! - 参考号 (RefU64)
//! - 类型哈希
//! - Owner 引用

use aios_core::pdms_types::RefI32Tuple;
use aios_core::tool::db_tool::db1_dehash;
use aios_core::types::RefU64;
use nom::number::complete::be_i32;
use nom::IResult;

use crate::parser::combinator::extend_impl_len;

/// 元素头部结构
///
/// PDMS 元素的固定头部信息（前 24 字节）
#[derive(Debug, Clone)]
pub struct ElementHeader {
    /// 隐含区长度（word 数）
    pub impl_len_words: i32,
    /// 元素参考号
    pub refno: RefU64,
    /// 类型哈希值
    pub type_hash: i32,
    /// Owner 参考号
    pub owner: RefU64,
}

impl ElementHeader {
    /// 隐含区长度（字节）
    #[inline]
    pub fn impl_len_bytes(&self) -> usize {
        (self.impl_len_words as usize) * 4
    }

    /// 获取类型名称
    #[inline]
    pub fn type_name(&self) -> String {
        db1_dehash(self.type_hash as u32)
    }

    /// 获取类型哈希的无符号值
    #[inline]
    pub fn type_hash_u32(&self) -> u32 {
        self.type_hash as u32
    }
}

/// 解析元素头部
///
/// # 格式（24 字节）
/// - bytes[0..4]: 隐含区长度 (word 数)
/// - bytes[4..12]: 元素参考号 (RefU64)
/// - bytes[12..16]: 类型哈希
/// - bytes[16..24]: Owner 参考号
///
/// # 示例
/// ```ignore
/// let (rest, header) = parse_element_header(input)?;
/// println!("Element {} of type {}", header.refno, header.type_name());
/// ```
pub fn parse_element_header(input: &[u8]) -> IResult<&[u8], ElementHeader> {
    if input.len() < 24 {
        return Err(nom::Err::Incomplete(nom::Needed::new(24 - input.len())));
    }

    let (_, impl_len_words) = be_i32(&input[0..4])?;
    let refno = RefU64::from(&input[4..12]);
    let (_, type_hash) = be_i32(&input[12..16])?;
    let owner = RefU64::from(&input[16..24]);

    Ok((
        &input[24..],
        ElementHeader {
            impl_len_words,
            refno,
            type_hash,
            owner,
        },
    ))
}

/// 解析元素头部并计算实际隐含区长度
///
/// 考虑 0/7 填充扩展
pub fn parse_element_header_with_actual_len(
    input: &[u8],
) -> IResult<&[u8], (ElementHeader, usize)> {
    let (rest, header) = parse_element_header(input)?;
    let declared = header.impl_len_bytes();
    let actual = extend_impl_len(declared, input);
    Ok((rest, (header, actual)))
}

/// 快速提取元素参考号
///
/// 不进行完整解析，仅读取 refno 字段
#[inline]
pub fn extract_refno(input: &[u8]) -> Option<RefU64> {
    if input.len() < 12 {
        return None;
    }
    Some(RefU64::from(&input[4..12]))
}

/// 快速提取元素类型哈希
#[inline]
pub fn extract_type_hash(input: &[u8]) -> Option<i32> {
    if input.len() < 16 {
        return None;
    }
    Some(i32::from_be_bytes(input[12..16].try_into().unwrap()))
}

/// 快速提取元素类型名称
#[inline]
pub fn extract_type_name(input: &[u8]) -> Option<String> {
    extract_type_hash(input).map(|h| db1_dehash(h as u32))
}

/// 快速提取 owner 参考号
#[inline]
pub fn extract_owner(input: &[u8]) -> Option<RefU64> {
    if input.len() < 24 {
        return None;
    }
    Some(RefU64::from(&input[16..24]))
}

/// 从 RefI32Tuple 转换为 RefU64
#[inline]
pub fn ref_tuple_to_u64(tuple: RefI32Tuple) -> RefU64 {
    tuple.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_header() -> [u8; 24] {
        let mut data = [0u8; 24];
        // impl_len = 6 words
        data[0..4].copy_from_slice(&6i32.to_be_bytes());
        // refno = (1, 2)
        data[4..8].copy_from_slice(&1i32.to_be_bytes());
        data[8..12].copy_from_slice(&2i32.to_be_bytes());
        // type_hash = 0x12345678
        data[12..16].copy_from_slice(&0x12345678i32.to_be_bytes());
        // owner = (3, 4)
        data[16..20].copy_from_slice(&3i32.to_be_bytes());
        data[20..24].copy_from_slice(&4i32.to_be_bytes());
        data
    }

    #[test]
    fn test_parse_element_header() {
        let input = make_test_header();
        let (rest, header) = parse_element_header(&input).unwrap();

        assert!(rest.is_empty());
        assert_eq!(header.impl_len_words, 6);
        assert_eq!(header.impl_len_bytes(), 24);
        assert_eq!(header.refno.get_0(), 1);
        assert_eq!(header.refno.get_1(), 2);
        assert_eq!(header.type_hash, 0x12345678);
        assert_eq!(header.owner.get_0(), 3);
        assert_eq!(header.owner.get_1(), 4);
    }

    #[test]
    fn test_extract_refno() {
        let input = make_test_header();
        let refno = extract_refno(&input).unwrap();
        assert_eq!(refno.get_0(), 1);
        assert_eq!(refno.get_1(), 2);
    }

    #[test]
    fn test_extract_type_hash() {
        let input = make_test_header();
        let hash = extract_type_hash(&input).unwrap();
        assert_eq!(hash, 0x12345678);
    }

    #[test]
    fn test_extract_owner() {
        let input = make_test_header();
        let owner = extract_owner(&input).unwrap();
        assert_eq!(owner.get_0(), 3);
        assert_eq!(owner.get_1(), 4);
    }

    #[test]
    fn test_insufficient_data() {
        let short = [0u8; 10];
        assert!(parse_element_header(&short).is_err());
        assert!(extract_refno(&short).is_none());
        assert!(extract_type_hash(&short).is_none());
        assert!(extract_owner(&short).is_none());
    }
}
