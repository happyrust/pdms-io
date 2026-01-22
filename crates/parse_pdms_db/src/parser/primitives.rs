//! 基础类型解析器
//!
//! 提供 PDMS 中常用基础类型的解析功能，包括：
//! - RefU64 (参考号)
//! - RefI32Tuple (参考号元组)
//! - Hash 值
//! - PDMS 字符串

use aios_core::pdms_types::RefI32Tuple;
use aios_core::types::RefU64;
use nom::combinator::map;
use nom::error::{ErrorKind, make_error};
use nom::multi::count;
use nom::number::complete::{be_i32, be_u16, be_u32, be_u64};
use nom::IResult;
use nom::Parser;

/// 解析 RefU64（两个 u32 组成的参考号）
///
/// # 格式
/// - 8 字节大端序
/// - 前 4 字节: 高位部分
/// - 后 4 字节: 低位部分
///
/// # 示例
/// ```ignore
/// let input = &[0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02];
/// let (_, refno) = parse_refno(input).unwrap();
/// ```
#[inline]
pub fn parse_refno(input: &[u8]) -> IResult<&[u8], RefU64> {
    map(
        (be_u32, be_u32),
        |(high, low)| RefU64::from_two_nums(high, low),
    )
    .parse(input)
}

/// 解析 RefI32Tuple（两个 i32 组成的参考号元组）
///
/// # 格式
/// - 8 字节大端序
/// - 前 4 字节: 第一个 i32
/// - 后 4 字节: 第二个 i32
#[inline]
pub fn parse_ref_tuple(input: &[u8]) -> IResult<&[u8], RefI32Tuple> {
    map(
        (be_i32, be_i32),
        |(a, b)| RefI32Tuple::new(a, b),
    )
    .parse(input)
}

/// 解析 4 字节哈希值
///
/// PDMS 使用哈希值标识属性类型和名称
#[inline]
pub fn parse_hash(input: &[u8]) -> IResult<&[u8], i32> {
    be_i32(input)
}

/// 解析无符号 4 字节哈希值
#[inline]
pub fn parse_hash_u32(input: &[u8]) -> IResult<&[u8], u32> {
    be_u32(input)
}

/// 解析隐含区声明长度（单位：字节）
///
/// PDMS 隐含区长度以 word (4字节) 为单位存储
#[inline]
pub fn parse_impl_len_bytes(input: &[u8]) -> IResult<&[u8], usize> {
    let (input, impl_words) = be_u32(input)?;
    Ok((input, impl_words as usize * 4))
}

/// 解析多个 RefU64
///
/// # 参数
/// - `cnt`: 要解析的 RefU64 数量
#[inline]
pub fn parse_refno_vec(input: &[u8], cnt: usize) -> IResult<&[u8], Vec<RefU64>> {
    count(parse_refno, cnt).parse(input)
}

/// 从字节数组解析 RefU64Vec（成员列表）
///
/// 输入长度必须是 8 的倍数
#[inline]
pub fn parse_members(input: &[u8]) -> IResult<&[u8], Vec<RefU64>> {
    if input.len() % 8 != 0 {
        return Err(nom::Err::Error(make_error(input, ErrorKind::LengthValue)));
    }
    let cnt = input.len() / 8;
    let (residual, vals) = count(be_u64, cnt).parse(input)?;
    let members: Vec<RefU64> = vals.into_iter().map(RefU64).collect();
    Ok((residual, members))
}

/// 解析 owner 属性（返回字符串形式的 refno）
#[inline]
pub fn parse_owner(input: &[u8]) -> IResult<&[u8], String> {
    let (_, (owner0, owner1)) = (be_i32, be_i32).parse(input)?;
    let owner: String = RefI32Tuple::new(owner0, owner1).into();
    Ok((input, owner))
}

/// 解析 2 字节标志位和长度
///
/// # 返回
/// - `(flag, length_in_words)`: 标志位和长度（word 数）
#[inline]
pub fn parse_flag_and_len(input: &[u8]) -> IResult<&[u8], (u16, u16)> {
    (be_u16, be_u16).parse(input)
}

/// 从固定偏移量解析 RefI32Tuple
///
/// # 参数
/// - `input`: 至少 12 字节的输入
/// - `offset`: 开始偏移量（通常为 4）
///
/// # 返回
/// 从 `input[offset..offset+8]` 解析的 RefI32Tuple
#[inline]
pub fn parse_ref_tuple_at(input: &[u8], offset: usize) -> Option<RefI32Tuple> {
    if input.len() < offset + 8 {
        return None;
    }
    Some((&input[offset..offset + 8]).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_refno() {
        let input = [0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02];
        let (rest, refno) = parse_refno(&input).unwrap();
        assert!(rest.is_empty());
        assert_eq!(refno.get_0(), 1);
        assert_eq!(refno.get_1(), 2);
    }

    #[test]
    fn test_parse_ref_tuple() {
        let input = [0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x01];
        let (rest, tuple) = parse_ref_tuple(&input).unwrap();
        assert!(rest.is_empty());
        assert_eq!(tuple.get_0(), -1);
        assert_eq!(tuple.get_1(), 1);
    }

    #[test]
    fn test_parse_hash() {
        let input = [0xFF, 0xF7, 0xE1, 0x77]; // ATT_PX hash
        let (rest, hash) = parse_hash(&input).unwrap();
        assert!(rest.is_empty());
        assert_eq!(hash, 0xFFF7E177u32 as i32);
    }

    #[test]
    fn test_parse_impl_len_bytes() {
        let input = [0x00, 0x00, 0x00, 0x05]; // 5 words = 20 bytes
        let (rest, len) = parse_impl_len_bytes(&input).unwrap();
        assert!(rest.is_empty());
        assert_eq!(len, 20);
    }

    #[test]
    fn test_parse_members() {
        let input = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
        ];
        let (rest, members) = parse_members(&input).unwrap();
        assert!(rest.is_empty());
        assert_eq!(members.len(), 2);
    }

    #[test]
    fn test_parse_members_invalid_len() {
        let input = [0x00, 0x00, 0x00, 0x01, 0x00]; // 5 bytes, not multiple of 8
        let result = parse_members(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_flag_and_len() {
        let input = [0x00, 0x02, 0x00, 0x10]; // flag=2, len=16
        let (rest, (flag, len)) = parse_flag_and_len(&input).unwrap();
        assert!(rest.is_empty());
        assert_eq!(flag, 2);
        assert_eq!(len, 16);
    }
}
