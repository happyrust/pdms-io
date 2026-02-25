//! 数据库索引区解析器
//!
//! 提供 PDMS 数据库索引区的解析功能，用于快速定位元素

use aios_core::types::RefU64;
use dashmap::DashMap;
use nom::IResult;
use nom::number::complete::be_u32;

/// 元素索引条目
#[derive(Debug, Clone, Copy)]
pub struct ElementIndex {
    /// 元素参考号
    pub refno: RefU64,
    /// 数据偏移量
    pub offset: u32,
    /// 数据长度
    pub length: u32,
    /// 类型哈希
    pub type_hash: i32,
}

/// 解析索引区单条记录
///
/// # 格式（通常 16-20 字节）
/// - bytes[0..8]: RefU64
/// - bytes[8..12]: offset
/// - bytes[12..16]: length
/// - bytes[16..20]: type_hash (可选)
pub fn parse_index_entry(input: &[u8]) -> IResult<&[u8], ElementIndex> {
    if input.len() < 16 {
        return Err(nom::Err::Incomplete(nom::Needed::new(16 - input.len())));
    }

    let refno = RefU64::from(&input[0..8]);
    let (_, offset) = be_u32(&input[8..12])?;
    let (_, length) = be_u32(&input[12..16])?;

    let type_hash = if input.len() >= 20 {
        i32::from_be_bytes(input[16..20].try_into().unwrap_or([0; 4]))
    } else {
        0
    };

    Ok((
        &input[16..],
        ElementIndex {
            refno,
            offset,
            length,
            type_hash,
        },
    ))
}

/// 索引表类型
pub type IndexMap = DashMap<RefU64, ElementIndex>;

/// 从偏移量数组构建索引
///
/// # 参数
/// - `offsets`: 有序的偏移量数组
/// - `total_len`: 数据总长度
///
/// # 返回
/// 每个偏移量对应的数据长度
pub fn calculate_lengths_from_offsets(offsets: &[u32], total_len: u32) -> Vec<u32> {
    if offsets.is_empty() {
        return vec![];
    }

    let mut lengths = Vec::with_capacity(offsets.len());
    for i in 0..offsets.len() - 1 {
        lengths.push(offsets[i + 1] - offsets[i]);
    }
    // 最后一个元素的长度
    if let Some(&last_offset) = offsets.last() {
        lengths.push(total_len.saturating_sub(last_offset));
    }
    lengths
}

/// 根据偏移量获取数据切片
///
/// # 参数
/// - `data`: 完整数据
/// - `offset`: 起始偏移
/// - `length`: 数据长度
#[inline]
pub fn get_data_slice(data: &[u8], offset: u32, length: u32) -> Option<&[u8]> {
    let start = offset as usize;
    let end = start + length as usize;
    if end <= data.len() {
        Some(&data[start..end])
    } else {
        None
    }
}

/// 根据偏移量表查找元素数据长度
///
/// # 参数
/// - `offsets`: 有序的偏移量列表
/// - `target`: 目标偏移量
pub fn find_length_by_offset(offsets: &[u32], target: u32) -> usize {
    if let Some(index) = offsets.iter().position(|&o| o == target) {
        if index + 1 < offsets.len() {
            return (offsets[index + 1] - offsets[index]) as usize;
        }
    }
    0
}

/// 二分查找偏移量
#[inline]
pub fn binary_search_offset(offsets: &[u32], target: u32) -> Option<usize> {
    offsets.binary_search(&target).ok()
}

/// 查找包含给定偏移量的区间
///
/// 返回区间的起始偏移和长度
pub fn find_containing_range(offsets: &[u32], target: u32) -> Option<(u32, u32)> {
    for i in 0..offsets.len().saturating_sub(1) {
        if target >= offsets[i] && target < offsets[i + 1] {
            return Some((offsets[i], offsets[i + 1] - offsets[i]));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_index_entry() {
        let mut data = vec![0u8; 20];
        // refno = (1, 2)
        data[0..4].copy_from_slice(&1u32.to_be_bytes());
        data[4..8].copy_from_slice(&2u32.to_be_bytes());
        // offset = 1024
        data[8..12].copy_from_slice(&1024u32.to_be_bytes());
        // length = 256
        data[12..16].copy_from_slice(&256u32.to_be_bytes());
        // type_hash = 0x12345678
        data[16..20].copy_from_slice(&0x12345678i32.to_be_bytes());

        let (_, entry) = parse_index_entry(&data).unwrap();
        assert_eq!(entry.refno.get_0(), 1);
        assert_eq!(entry.refno.get_1(), 2);
        assert_eq!(entry.offset, 1024);
        assert_eq!(entry.length, 256);
        assert_eq!(entry.type_hash, 0x12345678);
    }

    #[test]
    fn test_calculate_lengths_from_offsets() {
        let offsets = vec![0, 100, 250, 400];
        let lengths = calculate_lengths_from_offsets(&offsets, 500);
        assert_eq!(lengths, vec![100, 150, 150, 100]);
    }

    #[test]
    fn test_calculate_lengths_empty() {
        let offsets: Vec<u32> = vec![];
        let lengths = calculate_lengths_from_offsets(&offsets, 100);
        assert!(lengths.is_empty());
    }

    #[test]
    fn test_get_data_slice() {
        let data = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        assert_eq!(get_data_slice(&data, 2, 3), Some(&[2, 3, 4][..]));
        assert_eq!(get_data_slice(&data, 8, 5), None); // 超出范围
        assert_eq!(get_data_slice(&data, 0, 10), Some(&data[..]));
    }

    #[test]
    fn test_find_length_by_offset() {
        let offsets = vec![0, 100, 250, 400];
        assert_eq!(find_length_by_offset(&offsets, 100), 150);
        assert_eq!(find_length_by_offset(&offsets, 250), 150);
        assert_eq!(find_length_by_offset(&offsets, 999), 0); // 不存在
    }

    #[test]
    fn test_binary_search_offset() {
        let offsets = vec![0, 100, 250, 400];
        assert_eq!(binary_search_offset(&offsets, 100), Some(1));
        assert_eq!(binary_search_offset(&offsets, 150), None);
    }

    #[test]
    fn test_find_containing_range() {
        let offsets = vec![0, 100, 250, 400];
        assert_eq!(find_containing_range(&offsets, 50), Some((0, 100)));
        assert_eq!(find_containing_range(&offsets, 150), Some((100, 150)));
        assert_eq!(find_containing_range(&offsets, 300), Some((250, 150)));
        assert_eq!(find_containing_range(&offsets, 400), None); // 边界
    }
}
