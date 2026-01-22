//! 数值解析器
//!
//! 提供 PDMS 中各种数值类型的解析功能，包括：
//! - 标准数值类型 (i16, i32, u16, u32, f32, f64)
//! - PDMS 特殊格式数值（带标志位的显式数值）
//! - 单位换算数值

use aios_core::tool::float_tool::f64_round_3;
use nom::number::complete::{be_f32, be_f64, be_i16, be_i32, be_u16, be_u32};
use nom::IResult;

/// 解析大端序 f32
#[inline]
pub fn parse_f32_be(input: &[u8]) -> IResult<&[u8], f32> {
    be_f32(input)
}

/// 解析大端序 f64
#[inline]
pub fn parse_f64_be(input: &[u8]) -> IResult<&[u8], f64> {
    be_f64(input)
}

/// 解析大端序 i32
#[inline]
pub fn parse_i32_be(input: &[u8]) -> IResult<&[u8], i32> {
    be_i32(input)
}

/// 解析大端序 u32
#[inline]
pub fn parse_u32_be(input: &[u8]) -> IResult<&[u8], u32> {
    be_u32(input)
}

/// 解析大端序 i16
#[inline]
pub fn parse_i16_be(input: &[u8]) -> IResult<&[u8], i16> {
    be_i16(input)
}

/// 解析大端序 u16
#[inline]
pub fn parse_u16_be(input: &[u8]) -> IResult<&[u8], u16> {
    be_u16(input)
}

/// 解析显式数值（标志位 0x00 格式）
///
/// PDMS 显式属性中的特殊数值格式，12 字节：
/// - bytes[0..4]: 整数部分 a
/// - bytes[4..8]: 小数部分 b
/// - bytes[10..12]: 指数 times
///
/// 计算公式: ((a / 0x400 + b / 0x20000000) / 2^(5-times)) * 1000 / 1000
pub fn parse_explicit_num_00(data: &[u8]) -> IResult<&[u8], f64> {
    if data.len() < 12 {
        return Err(nom::Err::Incomplete(nom::Needed::new(12 - data.len())));
    }
    let (_, times) = be_i16(&data[10..12])?;
    let times = 2_f64.powf((5i16 - times) as f64);
    let (_, a) = be_i32(&data[..4])?;
    let (_, b) = be_i32(&data[4..8])?;
    let value = (((a as f64 / 0x400 as f64) + (b as f64 / 0x20000000 as f64)) / times * 1000.0).round()
        / 1000.0;
    let value = f64_round_3(value);
    Ok((data, value))
}

/// 解析显式数值（标志位 0x40 格式）
///
/// PDMS 显式属性中的 f64 格式，需要重排字节顺序：
/// - 12 字节输入
/// - 前 8 字节经过特殊重排后解析为 f64
/// - bytes[0] == 0x40 时结果取负
pub fn parse_explicit_f64_40(data: &[u8]) -> IResult<&[u8], f64> {
    if data.len() < 12 {
        return Err(nom::Err::Incomplete(nom::Needed::new(12 - data.len())));
    }
    let mut dst_data = data[..8].to_vec();
    let dst_first =
        (data[10] & 0xF).checked_shl(4).unwrap_or(0) + (data[11] & 0xF0).checked_shr(4).unwrap_or(0);
    dst_data[0] = dst_first;
    dst_data[1] = (data[11] & 0xF).checked_shl(4).unwrap_or(0) + (data[1] & 0xF);

    let value = if data[0] == 0x40 {
        let value = f64::from_be_bytes(dst_data.try_into().unwrap());
        -value
    } else {
        f64::from_be_bytes(dst_data.try_into().unwrap())
    };
    let value = f64_round_3(value);
    Ok((data, value))
}

/// 解析显式数值（标志位 0xFF 格式）
///
/// PDMS 显式属性中的特殊数值格式，12 字节：
/// - bytes[0..4]: 整数部分 a
/// - bytes[4..8]: 小数部分 b
/// - bytes[10..12]: 指数部分（0xFFFF - value）
pub fn parse_explicit_num_ff(data: &[u8]) -> IResult<&[u8], f64> {
    if data.len() < 12 {
        return Err(nom::Err::Incomplete(nom::Needed::new(12 - data.len())));
    }
    let a = i32::from_be_bytes(data[..4].try_into().unwrap());
    let b = i32::from_be_bytes(data[4..8].try_into().unwrap());
    let v = (a as f64 / 0x10024 as f64) + b as f64 / 0x40000000 as f64 * 0.5;
    let c = 0xFFFF_u32 - u16::from_be_bytes(data[10..12].try_into().unwrap()) as u32;
    let div_times = 2_i32.pow(c);
    let v = (v * 1000.0).round() / (div_times as f64) / 1000.0;
    let value = f64_round_3(v);
    Ok((data, value))
}

/// 根据标志位选择合适的数值解析器
///
/// # 参数
/// - `data`: 至少 12 字节的数据
///
/// # 返回
/// 解析后的 f64 值
pub fn parse_explicit_num(data: &[u8]) -> IResult<&[u8], f64> {
    if data.len() < 12 {
        return Err(nom::Err::Incomplete(nom::Needed::new(12 - data.len())));
    }
    match data[8] {
        0x00 if data[9] != 0x00 => parse_explicit_num_00(data),
        0x40 | 0x00 => parse_explicit_f64_40(data),
        0xFF => parse_explicit_num_ff(data),
        _ => parse_explicit_f64_40(data), // 默认使用 40 格式
    }
}

/// 乘以系数并保留两位小数
///
/// 用于 PDMS 中的数值转换
#[inline]
pub fn times_keep_f32_two_decimal(input: i32) -> f32 {
    let input = input as f32;
    let result = input / 40.0_f32 * 100.0;
    let b_seven = result as i32 % 10 == 7 && result < 100.0;
    let result = if b_seven {
        f32::trunc(result) / 100.0
    } else {
        result.round() / 100.0
    };
    result
}

/// 快速从字节解析 i32（不返回 IResult）
#[inline]
pub fn bytes_to_i32(input: &[u8]) -> i32 {
    i32::from_be_bytes(input[..4].try_into().unwrap_or([0; 4]))
}

/// 快速从字节解析 u16（不返回 IResult）
#[inline]
pub fn bytes_to_u16(input: &[u8]) -> u16 {
    u16::from_be_bytes(input[..2].try_into().unwrap_or([0; 2]))
}

/// 快速从字节解析 f32（不返回 IResult）
#[inline]
pub fn bytes_to_f32(input: &[u8]) -> f32 {
    f32::from_be_bytes(input[..4].try_into().unwrap_or([0; 4]))
}

/// 快速从字节解析 f64（不返回 IResult）
#[inline]
pub fn bytes_to_f64(input: &[u8]) -> f64 {
    f64::from_be_bytes(input[..8].try_into().unwrap_or([0; 8]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_f32_be() {
        // 1.0 in f32 big-endian
        let input = [0x3F, 0x80, 0x00, 0x00];
        let (_, value) = parse_f32_be(&input).unwrap();
        assert!((value - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_parse_f64_be() {
        // 1.0 in f64 big-endian
        let input = [0x3F, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let (_, value) = parse_f64_be(&input).unwrap();
        assert!((value - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_parse_explicit_f64_40() {
        // 测试数据: -5.0
        let data = [0x40, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x04, 0x03];
        let result = parse_explicit_f64_40(&data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_bytes_to_i32() {
        let input = [0x00, 0x00, 0x00, 0x64]; // 100
        assert_eq!(bytes_to_i32(&input), 100);

        let input = [0xFF, 0xFF, 0xFF, 0xFF]; // -1
        assert_eq!(bytes_to_i32(&input), -1);
    }

    #[test]
    fn test_bytes_to_u16() {
        let input = [0x00, 0x64]; // 100
        assert_eq!(bytes_to_u16(&input), 100);

        let input = [0xFF, 0xFF]; // 65535
        assert_eq!(bytes_to_u16(&input), 65535);
    }

    #[test]
    fn test_times_keep_f32_two_decimal() {
        let result = times_keep_f32_two_decimal(40);
        assert!((result - 1.0).abs() < 1e-6);
    }
}
