//! 子元素解析器
//!
//! 提供 PDMS 元素子元素列表的解析功能

use aios_core::pdms_types::RefI32Tuple;
use aios_core::types::{RefU64, RefU64Vec};
use nom::IResult;
use nom::Parser;
use nom::error::{ErrorKind, make_error};
use nom::multi::count;
use nom::number::complete::{be_u16, be_u64};

use crate::parser::combinator::{collect_segmented_payload, extend_impl_len};
use crate::parser::primitives::parse_impl_len_bytes;

/// Members 块标志位
pub const MEMBERS_FLAG: u16 = 0x0002;

/// 解析 members 块
///
/// # 格式(specs/005 T101 字节裁决:标准 5 词链节点)
/// - bytes[0..2]: 标志位 (0x0002)
/// - bytes[2..4]: 总长度 (word 数,含 5 词节点头)
/// - bytes[4..12]: 自身 refno (用于校验)
/// - bytes[12..20]: 链指针 next_pg/next_loc(非载荷)
/// - bytes[20..]: 成员 RefU64 列表
///
/// # 参数
/// - `input`: 输入数据
/// - `expected_refno`: 期望的自身 refno（用于校验）
pub fn parse_members_block(input: &[u8], expected_refno: RefU64) -> IResult<&[u8], RefU64Vec> {
    if input.len() < 4 {
        return Err(nom::Err::Error(make_error(input, ErrorKind::Eof)));
    }

    // 解析 flag 和长度
    let (_, (flag, len_words)) = (be_u16, be_u16).parse(input)?;
    let memb_bytes_len = len_words as usize * 4;

    if flag != MEMBERS_FLAG {
        return Err(nom::Err::Error(make_error(input, ErrorKind::Tag)));
    }
    if memb_bytes_len > input.len() {
        return Err(nom::Err::Error(make_error(input, ErrorKind::Eof)));
    }

    // 校验自身 refno
    if memb_bytes_len < 12 {
        return Err(nom::Err::Error(make_error(input, ErrorKind::Eof)));
    }
    let block = &input[..memb_bytes_len];
    let self_refno: RefI32Tuple = (&block[4..12]).into();
    let expected = RefI32Tuple::new(expected_refno.get_0() as i32, expected_refno.get_1() as i32);

    if self_refno != expected {
        return Err(nom::Err::Error(make_error(input, ErrorKind::Verify)));
    }

    // 合并主段及可能存在的 0x00000007 追加段
    let (rest, members_data) =
        collect_segmented_payload(input, memb_bytes_len, MEMBERS_FLAG as u8)?;

    // 解析成员列表；错误时将错误绑定到原始 input 以避免悬垂引用
    let members = match parse_members_data(&members_data) {
        Ok(m) => m,
        Err(_) => return Err(nom::Err::Error(make_error(input, ErrorKind::Verify))),
    };
    Ok((rest, members))
}

/// 解析成员数据
///
/// 输入为纯 RefU64 列表（每个 8 字节）
fn parse_members_data(input: &[u8]) -> Result<RefU64Vec, nom::Err<nom::error::Error<&[u8]>>> {
    if input.len() % 8 != 0 {
        return Err(nom::Err::Error(make_error(input, ErrorKind::LengthValue)));
    }

    let cnt = input.len() / 8;
    let (_, vals) = count(be_u64, cnt).parse(input)?;
    let members: Vec<RefU64> = vals.into_iter().map(RefU64).collect();
    Ok(RefU64Vec(members))
}

/// 解析元素的子元素列表
///
/// 组合式解析，返回 (refno, children)
///
/// # 格式
/// - 隐含区（变长，含 0/7 填充）
/// - members 块
pub fn parse_element_children(input: &[u8]) -> IResult<&[u8], (RefU64, RefU64Vec)> {
    if input.len() < 24 {
        return Err(nom::Err::Error(make_error(input, ErrorKind::Eof)));
    }

    // 解析隐含区长度
    let (_, impl_len_bytes) = parse_impl_len_bytes(input)?;
    let refno: RefI32Tuple = (&input[4..12]).into();

    // 扩展隐含区长度（含 0/7 填充）
    let actual_impl_len = extend_impl_len(impl_len_bytes, input);
    if actual_impl_len > input.len() {
        return Err(nom::Err::Error(make_error(input, ErrorKind::Eof)));
    }

    // 解析 members 块
    let membs_data = &input[actual_impl_len..];
    let (rest, children) = parse_members_block(membs_data, refno.into())?;

    Ok((rest, (refno.into(), children)))
}

/// 快速提取元素的成员列表
///
/// 仅返回 RefU64 列表，不返回自身 refno
#[inline]
pub fn extract_members(input: &[u8]) -> Vec<RefU64> {
    parse_element_children(input)
        .map(|(_, (_, children))| children.0)
        .unwrap_or_default()
}

/// 获取成员数量（不完整解析）
///
/// 快速读取成员块的长度信息
pub fn count_members(input: &[u8]) -> Option<usize> {
    if input.len() < 24 {
        return None;
    }

    // 读取隐含区长度
    let impl_len_bytes = u32::from_be_bytes(input[0..4].try_into().ok()?) as usize * 4;
    let actual_impl_len = extend_impl_len(impl_len_bytes, input);

    if actual_impl_len >= input.len() {
        return None;
    }

    let membs_data = &input[actual_impl_len..];
    if membs_data.len() < 4 {
        return None;
    }

    // 读取 members 块长度
    let len_words = u16::from_be_bytes(membs_data[2..4].try_into().ok()?) as usize;
    let total_bytes = len_words * 4;

    // 减去 5 词节点头 (flag|len + refno×2 + 链指针×2 = 20 字节)，剩余为成员数据
    if total_bytes >= 20 {
        Some((total_bytes - 20) / 8)
    } else {
        Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_members_block(refno: RefU64, members: &[RefU64]) -> Vec<u8> {
        let mut data = Vec::new();

        // flag = 0x0002
        data.extend_from_slice(&MEMBERS_FLAG.to_be_bytes());

        // 5 词节点头 + 成员载荷(specs/005 T101 布局):len = (4 + 8 + 8 + n*8) / 4
        let total_bytes = 4 + 8 + 8 + members.len() * 8;
        let len_words = total_bytes / 4;
        data.extend_from_slice(&(len_words as u16).to_be_bytes());

        // self refno
        data.extend_from_slice(&(refno.get_0() as i32).to_be_bytes());
        data.extend_from_slice(&(refno.get_1() as i32).to_be_bytes());

        // 链指针 next_pg/next_loc(单节点 = 0)
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());

        // members
        for m in members {
            data.extend_from_slice(&m.0.to_be_bytes());
        }

        data
    }

    #[test]
    fn test_parse_members_block() {
        let refno = RefU64::from_two_nums(1, 2);
        let members = vec![RefU64(100), RefU64(200), RefU64(300)];
        let data = make_members_block(refno, &members);

        let (rest, result) = parse_members_block(&data, refno).unwrap();
        assert!(rest.is_empty());
        assert_eq!(result.0.len(), 3);
        assert_eq!(result.0[0].0, 100);
        assert_eq!(result.0[1].0, 200);
        assert_eq!(result.0[2].0, 300);
    }

    #[test]
    fn test_parse_members_block_with_segment() {
        let refno = RefU64::from_two_nums(1, 2);
        let base_members = vec![RefU64(10), RefU64(20)];
        let extra_member = RefU64(30);

        // 主段
        let mut data = make_members_block(refno, &base_members);

        // 追加段：00 00 00 07 00 02 00 07 ... payload(30)
        let seg_len_words: u16 = 7; // 28 bytes
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x07, 0x00, MEMBERS_FLAG as u8]);
        data.extend_from_slice(&seg_len_words.to_be_bytes());
        // self refno + reserved 8 bytes
        data.extend_from_slice(&(refno.get_0() as i32).to_be_bytes());
        data.extend_from_slice(&(refno.get_1() as i32).to_be_bytes());
        data.extend_from_slice(&0i32.to_be_bytes());
        data.extend_from_slice(&0i32.to_be_bytes());
        // payload
        data.extend_from_slice(&extra_member.0.to_be_bytes());

        let (rest, result) = parse_members_block(&data, refno).unwrap();
        assert!(rest.is_empty());
        assert_eq!(result.0.len(), 3);
        assert_eq!(result.0[2].0, extra_member.0);
    }

    #[test]
    fn test_parse_members_block_wrong_refno() {
        let refno = RefU64::from_two_nums(1, 2);
        let wrong_refno = RefU64::from_two_nums(3, 4);
        let members = vec![RefU64(100)];
        let data = make_members_block(refno, &members);

        let result = parse_members_block(&data, wrong_refno);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_members_block_wrong_flag() {
        let mut data = vec![0x00, 0x03, 0x00, 0x04]; // wrong flag
        data.extend_from_slice(&[0; 12]); // dummy data

        let result = parse_members_block(&data, RefU64::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_members_empty() {
        let empty: &[u8] = &[];
        assert!(extract_members(empty).is_empty());
    }

    #[test]
    fn test_count_members() {
        // 构造一个简单的元素数据
        let mut data = Vec::new();

        // impl_len = 6 words (24 bytes)
        data.extend_from_slice(&6u32.to_be_bytes());
        // refno
        data.extend_from_slice(&1i32.to_be_bytes());
        data.extend_from_slice(&2i32.to_be_bytes());
        // type_hash
        data.extend_from_slice(&0x12345678i32.to_be_bytes());
        // owner
        data.extend_from_slice(&3i32.to_be_bytes());
        data.extend_from_slice(&4i32.to_be_bytes());

        // members block: flag + len + refno + 2 members
        let refno = RefU64::from_two_nums(1, 2);
        let members = vec![RefU64(100), RefU64(200)];
        data.extend(make_members_block(refno, &members));

        let count = count_members(&data);
        assert_eq!(count, Some(2));
    }
}
