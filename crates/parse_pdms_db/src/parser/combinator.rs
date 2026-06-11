//! 自定义组合子
//!
//! 提供 PDMS 解析中特有的组合子和辅助函数，包括：
//! - 填充扩展
//! - 标记检测
//! - 数据块提取

use nom::IResult;
use nom::Parser;
use nom::bytes::complete::take;
use nom::error::{ErrorKind, make_error};

/// 默认 members 主段 payload 起始偏移（flag+len+self_ref 共 12 字节）
// specs/005 T201 修正(契约 F1-I1 字节裁决):members/显式主段是标准 5 词链节点
// `[(which<<16)|len][refno0][refno1][next_pg][next_loc]`+payload——载荷自 **+20** 起;
// 旧值 12 把 w3/w4 链指针计入载荷:单节点时混入 (0,0) 伪成员,多节点链时混入
// `(next_pg, next_loc)` 伪成员(sam7200 (23584,5443) 实测暴露)。追加段 +24(=4+20)一直正确。
const MEMBERS_BASE_PAYLOAD_OFFSET: usize = 20;
/// 追加段（0x00000007）payload 起始偏移（标记+flag+len+self_ref+保留 16 字节 共 24 字节）
const SEGMENT_PAYLOAD_OFFSET: usize = 24;

/// PDMS 0/7 填充标记
pub const PADDING_ZERO: [u8; 4] = [0x00, 0x00, 0x00, 0x00];
pub const PADDING_SEVEN: [u8; 4] = [0x00, 0x00, 0x00, 0x07];

/// 扩展隐含区长度以包含 0/7 填充
///
/// PDMS 隐含区末尾可能跟随 0x00000000 或 0x00000007 填充，
/// 需要将这些填充计入实际长度。
///
/// # 参数
/// - `declared`: 声明的长度（字节）
/// - `input`: 完整输入数据
///
/// # 返回
/// 扩展后的实际长度
#[inline]
pub fn extend_impl_len(declared: usize, input: &[u8]) -> usize {
    let mut actual = declared;
    while actual + 4 <= input.len() {
        let next = &input[actual..actual + 4];
        if next == PADDING_ZERO || next == PADDING_SEVEN {
            actual += 4;
        } else {
            break;
        }
    }
    actual
}

/// 检查数据是否以 0/7 填充开始
#[inline]
pub fn starts_with_padding(input: &[u8]) -> bool {
    if input.len() < 4 {
        return false;
    }
    let marker = &input[..4];
    marker == PADDING_ZERO || marker == PADDING_SEVEN
}

/// 跳过所有 0/7 填充
///
/// # 返回
/// 跳过填充后的剩余数据
#[inline]
pub fn skip_padding(input: &[u8]) -> &[u8] {
    let mut pos = 0;
    while pos + 4 <= input.len() {
        let next = &input[pos..pos + 4];
        if next == PADDING_ZERO || next == PADDING_SEVEN {
            pos += 4;
        } else {
            break;
        }
    }
    &input[pos..]
}

/// 解析指定长度的数据块
///
/// # 参数
/// - `len`: 要提取的字节数
#[inline]
pub fn take_bytes(len: usize) -> impl Fn(&[u8]) -> IResult<&[u8], &[u8]> {
    move |input| take(len).parse(input)
}

/// 解析带扩展的隐含区数据块
///
/// 读取声明长度后，扩展到包含所有 0/7 填充
///
/// # 参数
/// - `input`: 输入数据（第一个 u32 是长度）
///
/// # 返回
/// - 剩余数据
/// - 隐含区数据（不含长度字段本身）
pub fn take_impl_block(input: &[u8]) -> IResult<&[u8], &[u8]> {
    if input.len() < 4 {
        return Err(nom::Err::Error(make_error(input, ErrorKind::Eof)));
    }
    let declared = u32::from_be_bytes(input[..4].try_into().unwrap()) as usize * 4;
    let actual = extend_impl_len(declared, input);

    if actual > input.len() {
        return Err(nom::Err::Error(make_error(input, ErrorKind::Eof)));
    }

    let (_, block) = take(actual).parse(input)?;
    let rest = &input[actual..];
    Ok((rest, block))
}

/// 合并带 0x00000007 追加段的块数据
///
/// 一些 members/显式属性块在声明长度后，可能跟随一个或多个
/// `00 00 00 07 00 <flag>` 开头的追加段。该函数在保留主段内容的
/// 同时，将追加段的 payload 片段串联起来，返回组合后的 payload。
///
/// # 参数
/// - `input`: 从块起始处开始的完整数据（主段 + 后续可能的 0x07 段）
/// - `declared_len_bytes`: 主段声明的长度（字节）
/// - `flag`: 目标段的标志位（members=0x02/显式属性=0x01）
pub fn collect_segmented_payload(
    input: &[u8],
    declared_len_bytes: usize,
    flag: u8,
) -> IResult<&[u8], Vec<u8>> {
    if declared_len_bytes < MEMBERS_BASE_PAYLOAD_OFFSET {
        return Err(nom::Err::Error(make_error(input, ErrorKind::Eof)));
    }
    if input.len() < declared_len_bytes {
        return Err(nom::Err::Incomplete(nom::Needed::new(
            declared_len_bytes - input.len(),
        )));
    }

    // 主段 payload（去掉 flag/len/self_ref）
    let mut payload = input[MEMBERS_BASE_PAYLOAD_OFFSET..declared_len_bytes].to_vec();
    let mut cursor = declared_len_bytes;

    while cursor + 6 <= input.len()
        && &input[cursor..cursor + 4] == &[0x00, 0x00, 0x00, 0x07]
        && input[cursor + 4] == 0x00
        && input[cursor + 5] == flag
    {
        if cursor + 8 > input.len() {
            return Err(nom::Err::Error(make_error(input, ErrorKind::Eof)));
        }
        let seg_len_words =
            u16::from_be_bytes(input[cursor + 6..cursor + 8].try_into().unwrap()) as usize;
        let seg_len_bytes = seg_len_words * 4;
        if seg_len_bytes == 0 {
            return Err(nom::Err::Error(make_error(input, ErrorKind::LengthValue)));
        }

        // 与旧逻辑保持一致：总长 = len_bytes + 4（含前导长度字段）
        let seg_end = cursor + seg_len_bytes + 4;
        if seg_end > input.len() {
            return Err(nom::Err::Error(make_error(input, ErrorKind::Eof)));
        }

        let seg_payload_start = cursor + SEGMENT_PAYLOAD_OFFSET;
        if seg_payload_start > seg_end {
            return Err(nom::Err::Error(make_error(input, ErrorKind::LengthValue)));
        }

        payload.extend_from_slice(&input[seg_payload_start..seg_end]);
        cursor = seg_end;
    }

    Ok((&input[cursor..], payload))
}

/// 验证标志位
///
/// # 参数
/// - `expected`: 期望的标志值
#[inline]
pub fn verify_flag(input: &[u8], expected: u16) -> IResult<&[u8], u16> {
    if input.len() < 2 {
        return Err(nom::Err::Error(make_error(input, ErrorKind::Eof)));
    }
    let flag = u16::from_be_bytes(input[..2].try_into().unwrap());
    if flag != expected {
        return Err(nom::Err::Error(make_error(input, ErrorKind::Tag)));
    }
    Ok((&input[2..], flag))
}

/// 检查是否为特定的 4 字节标记
#[inline]
pub fn is_marker(input: &[u8], marker: &[u8; 4]) -> bool {
    input.len() >= 4 && &input[..4] == marker
}

/// 查找下一个非填充数据的位置
///
/// # 返回
/// 第一个非 0/7 填充的偏移量
#[inline]
pub fn find_non_padding(input: &[u8]) -> usize {
    let mut pos = 0;
    while pos + 4 <= input.len() {
        let next = &input[pos..pos + 4];
        if next != PADDING_ZERO && next != PADDING_SEVEN {
            break;
        }
        pos += 4;
    }
    pos
}

/// 按 word (4字节) 对齐长度
#[inline]
pub fn align_to_word(len: usize) -> usize {
    (len + 3) & !3
}

/// 按 8 字节对齐长度
#[inline]
pub fn align_to_8(len: usize) -> usize {
    (len + 7) & !7
}

/// 安全地提取指定范围的切片
///
/// # 返回
/// `Some(slice)` 如果范围有效，否则 `None`
#[inline]
pub fn safe_slice(input: &[u8], start: usize, end: usize) -> Option<&[u8]> {
    if start <= end && end <= input.len() {
        Some(&input[start..end])
    } else {
        None
    }
}

/// 计算剩余可用字节数
#[inline]
pub fn remaining_bytes(input: &[u8], consumed: usize) -> usize {
    input.len().saturating_sub(consumed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::element::children::MEMBERS_FLAG;

    #[test]
    fn test_extend_impl_len() {
        // 声明 4 字节，后面有两个填充
        let input = [
            0x00, 0x00, 0x00, 0x01, // 1 word = 4 bytes 数据
            0x00, 0x00, 0x00, 0x00, // 填充
            0x00, 0x00, 0x00, 0x07, // 填充
            0x00, 0x00, 0x00, 0x02, // 非填充数据
        ];
        // 声明长度 4，实际应该扩展到 12 (4 + 4 + 4)
        let actual = extend_impl_len(4, &input);
        assert_eq!(actual, 12);
    }

    #[test]
    fn test_skip_padding() {
        let input = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x01,
        ];
        let rest = skip_padding(&input);
        assert_eq!(rest, &[0x00, 0x00, 0x00, 0x01]);
    }

    #[test]
    fn test_starts_with_padding() {
        assert!(starts_with_padding(&PADDING_ZERO));
        assert!(starts_with_padding(&PADDING_SEVEN));
        assert!(!starts_with_padding(&[0x00, 0x00, 0x00, 0x01]));
        assert!(!starts_with_padding(&[0x00, 0x00])); // 太短
    }

    #[test]
    fn test_verify_flag() {
        let input = [0x00, 0x02, 0x00, 0x10];
        let (rest, flag) = verify_flag(&input, 0x0002).unwrap();
        assert_eq!(flag, 2);
        assert_eq!(rest, &[0x00, 0x10]);

        // 错误的标志位
        let result = verify_flag(&input, 0x0003);
        assert!(result.is_err());
    }

    #[test]
    fn test_align_to_word() {
        assert_eq!(align_to_word(0), 0);
        assert_eq!(align_to_word(1), 4);
        assert_eq!(align_to_word(4), 4);
        assert_eq!(align_to_word(5), 8);
    }

    #[test]
    fn test_align_to_8() {
        assert_eq!(align_to_8(0), 0);
        assert_eq!(align_to_8(1), 8);
        assert_eq!(align_to_8(8), 8);
        assert_eq!(align_to_8(9), 16);
    }

    #[test]
    fn test_safe_slice() {
        let input = [1, 2, 3, 4, 5];
        assert_eq!(safe_slice(&input, 0, 3), Some(&[1, 2, 3][..]));
        assert_eq!(safe_slice(&input, 2, 5), Some(&[3, 4, 5][..]));
        assert_eq!(safe_slice(&input, 0, 6), None); // 超出范围
        assert_eq!(safe_slice(&input, 3, 2), None); // start > end
    }

    #[test]
    fn test_find_non_padding() {
        let input = [
            0x00, 0x00, 0x00, 0x00, // padding
            0x00, 0x00, 0x00, 0x07, // padding
            0x00, 0x00, 0x00, 0x01, // data
        ];
        assert_eq!(find_non_padding(&input), 8);

        let input2 = [0x00, 0x00, 0x00, 0x01]; // no padding
        assert_eq!(find_non_padding(&input2), 0);
    }

    #[test]
    fn test_collect_segmented_payload() {
        // 主段 = 标准 5 词节点 + payload(specs/005 T101 字节裁决):
        // flag|len(4) + refno(8) + 链指针(8) + payload(8) = 7 words(28 bytes)
        let mut data = Vec::new();
        data.extend_from_slice(&MEMBERS_FLAG.to_be_bytes());
        data.extend_from_slice(&(7u16).to_be_bytes()); // 28 bytes
        data.extend_from_slice(&1u32.to_be_bytes()); // refno high
        data.extend_from_slice(&2u32.to_be_bytes()); // refno low
        data.extend_from_slice(&0u32.to_be_bytes()); // next_pg(链指针,非载荷)
        data.extend_from_slice(&0u32.to_be_bytes()); // next_loc
        data.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]); // payload 1
        data.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]); // payload 2

        // 追加段：0x00000007 00 02，len=7 words（28 bytes），payload=8 bytes
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x07, 0x00, MEMBERS_FLAG as u8]);
        data.extend_from_slice(&(7u16).to_be_bytes());
        data.extend_from_slice(&1u32.to_be_bytes()); // refno high
        data.extend_from_slice(&2u32.to_be_bytes()); // refno low
        data.extend_from_slice(&0u32.to_be_bytes()); // reserved
        data.extend_from_slice(&0u32.to_be_bytes()); // reserved
        data.extend_from_slice(&[0x55, 0x66, 0x77, 0x88]); // payload 3
        data.extend_from_slice(&[0x99, 0xAA, 0xBB, 0xCC]); // payload 4

        let (rest, payload) = collect_segmented_payload(&data, 28, MEMBERS_FLAG as u8).unwrap();
        assert!(rest.is_empty());
        assert_eq!(
            payload,
            vec![
                0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA,
                0xBB, 0xCC
            ]
        );
    }
}
