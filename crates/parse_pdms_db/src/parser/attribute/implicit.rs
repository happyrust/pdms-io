//! 隐式属性解析器
//!
//! 提供 PDMS 隐式属性的解析功能，包括：
//! - 基于偏移量的属性定位
//! - 多种数据类型解析（整数、浮点、字符串、引用号等）
//! - 特殊属性处理（LEVEL、PTS、表达式等）
//! - f32/f64 混合模式支持
//! - 可配置的元素头部偏移

use aios_core::helper::{parse_to_f32, parse_to_f64};
use aios_core::pdms_types::DbAttributeType;
use aios_core::types::NamedAttrValue;
use glam::Vec3;
use nom::IResult;

/// 元素数据的标准头部大小（单位：word）
///
/// 标准元素数据头部布局：
/// - bytes[0..4]: impl_len (1 word)
/// - bytes[4..12]: refno (2 words)
/// - bytes[12..16]: type_hash (1 word)
/// - bytes[16..24]: owner (2 words)
///
/// 总计：6 words = 24 bytes
pub const STANDARD_ELEMENT_HEADER_WORDS: usize = 6;

/// 隐式属性偏移信息
#[derive(Debug, Clone)]
pub struct ImplicitAttrOffset {
    /// 属性名称
    pub name: String,
    /// 偏移量（低16位为word位置，高12位为特殊标志）
    pub offset: u32,
    /// 属性类型
    pub attr_type: DbAttributeType,
}

/// 解析隐式属性值
///
/// # 参数
/// - `input`: 隐式数据块（已分段合并）
/// - `attr_info`: 属性元数据信息
/// - `is_f32`: 是否使用 f32 模式
/// - `f32_neg_offset`: f32 模式的负偏移量（用于调整因 f32/f64 类型差异导致的偏移）
/// - `_step`: 当前解析步骤（用于调试）
///
/// # 偏移计算说明
///
/// Schema 中的 offset 是从元素开头计算的（以 word 为单位）。实际数据位置计算为：
/// ```text
/// actual_offset = (schema_offset - f32_neg_offset) * 4
/// ```
///
/// 其中 f32_neg_offset 的累计规则：
/// - 每个 DOUBLE 属性在 f32 模式下节省 1 word
/// - 每个 Vec3/DIRECTION/POSITION/ORIENTATION 属性在 f32 模式下节省 3 words
///
/// # 返回
/// `IResult<&[u8], NamedAttrValue>` - 解析后的属性值
pub fn parse_implicit_attr_value<'a>(
    input: &'a [u8],
    attr_info: &ImplicitAttrOffset,
    is_f32: bool,
    f32_neg_offset: usize,
    _step: usize,
) -> IResult<&'a [u8], NamedAttrValue> {
    // 计算实际偏移位置
    let offset_words = (attr_info.offset & 0xFFFF) as usize;
    let offset_bytes = offset_words * 4;

    // 应用 f32 模式偏移调整
    let actual_offset = if is_f32 {
        offset_bytes.saturating_sub(f32_neg_offset * 4)
    } else {
        offset_bytes
    };

    // 确保数据范围有效
    if actual_offset >= input.len() {
        return Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Eof,
        )));
    }

    let data = &input[actual_offset..];

    // 根据属性类型解析值
    match attr_info.attr_type {
        DbAttributeType::INTEGER => parse_integer(data),
        DbAttributeType::DOUBLE => parse_double(data, is_f32),
        DbAttributeType::BOOL => parse_bool(data, attr_info.offset),
        DbAttributeType::STRING => {
            let (rest, value) = parse_string(data)?;
            Ok((rest, normalize_empty_string_attr(&attr_info.name, value)))
        }
        DbAttributeType::ELEMENT => parse_refno(data),
        DbAttributeType::WORD => parse_word(data),
        DbAttributeType::Vec3Type => parse_vec3(data, is_f32),
        DbAttributeType::DOUBLEVEC => parse_double_array(data, is_f32),
        DbAttributeType::INTVEC => parse_int_array(data, &attr_info.name),
        _ => {
            // 未知类型，返回空值
            Ok((input, NamedAttrValue::InvalidType))
        }
    }
}

/// 检测元素数据的实际头部大小（单位：word）
///
/// PDMS 元素数据可能有不同的头部布局：
/// - 标准头部 (6 words = 24 bytes): impl_len + refno + type_hash + owner
/// - 扩展头部 (某些特殊元素可能有额外字段)
///
/// # 参数
/// - `input`: 元素数据的开头部分
///
/// # 返回
/// 检测到的头部大小（以 word 为单位）
///
/// # 说明
/// 当前实现返回标准头部大小。如果发现特定元素有不同的头部布局，
/// 可以根据 type_hash 或其他特征进行检测。
pub fn detect_header_size(input: &[u8]) -> usize {
    // 标准头部检测逻辑
    // bytes[0..4]: impl_len - 隐式数据长度
    // bytes[4..12]: refno - 参考号
    // bytes[12..16]: type_hash - 类型哈希
    // bytes[16..24]: owner - 所有者参考号

    if input.len() < 24 {
        return STANDARD_ELEMENT_HEADER_WORDS;
    }

    // 目前返回标准头部大小
    // TODO: 如果需要支持特殊元素的扩展头部，在这里添加检测逻辑
    // 例如：检查 type_hash 是否属于需要扩展头部的类型
    STANDARD_ELEMENT_HEADER_WORDS
}

/// 计算 f32 模式下的负偏移量
///
/// 当元素使用 f32 模式存储浮点数时，某些属性类型会比 f64 模式占用更少的空间，
/// 需要累计这个差值来调整后续属性的偏移量。
///
/// # 参数
/// - `attr_type`: 属性类型
///
/// # 返回
/// 该属性类型在 f32 模式下节省的 word 数量
pub fn get_f32_offset_adjustment(attr_type: DbAttributeType) -> usize {
    match attr_type {
        // DOUBLE: f64 (2 words) -> f32 (1 word)，节省 1 word
        DbAttributeType::DOUBLE => 1,
        // Vec3/方向/位置/姿态: 3 个 f64 (6 words) -> 3 个 f32 (3 words)，节省 3 words
        DbAttributeType::DIRECTION
        | DbAttributeType::POSITION
        | DbAttributeType::ORIENTATION
        | DbAttributeType::Vec3Type => 3,
        // 其他类型不受影响
        _ => 0,
    }
}

/// 解析整数类型
fn parse_integer(input: &[u8]) -> IResult<&[u8], NamedAttrValue> {
    if input.len() < 4 {
        return Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Eof,
        )));
    }
    let value = i32::from_be_bytes(input[..4].try_into().unwrap());
    Ok((input, NamedAttrValue::IntegerType(value)))
}

/// 解析浮点类型
fn parse_double(input: &[u8], is_f32: bool) -> IResult<&[u8], NamedAttrValue> {
    if is_f32 {
        if input.len() < 4 {
            return Err(nom::Err::Error(nom::error::make_error(
                input,
                nom::error::ErrorKind::Eof,
            )));
        }
        let value = parse_to_f32(&input[..4]);
        Ok((input, NamedAttrValue::F32Type(value)))
    } else {
        if input.len() < 8 {
            return Err(nom::Err::Error(nom::error::make_error(
                input,
                nom::error::ErrorKind::Eof,
            )));
        }
        let value = parse_to_f64(&input[..8]) as f32;
        Ok((input, NamedAttrValue::F32Type(value)))
    }
}

/// 解析布尔类型（从 offset 高位提取位信息）
fn parse_bool(input: &[u8], offset: u32) -> IResult<&[u8], NamedAttrValue> {
    if input.len() < 4 {
        return Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Eof,
        )));
    }

    // 高12位用于位操作
    let bit_position = (offset >> 20) as u32;
    let value = i32::from_be_bytes(input[..4].try_into().unwrap());
    let bool_value = (value >> bit_position) & 1 == 1;

    Ok((input, NamedAttrValue::BoolType(bool_value)))
}

/// 解析字符串类型
///
/// PDMS 隐式字符串有两种布局（len 均为字符数）:
/// - **packed**: 文本按 4 字节/word 打包（如 SDTE.SKEY: `[4]["VGBW"]`），
///   首文本字节即第一个字符（可打印,非 0）;
/// - **word-per-char**: 每 word 存一个字符（低字节有效），首文本 word 高 3 字节为 0。
///
/// 此前实现只按 word-per-char 读取,packed 字符串会被错读
/// （如 "VGBW" → "W\0\0\0"，金标准 =13246/243869 Skey VGBW）。
fn parse_string(input: &[u8]) -> IResult<&[u8], NamedAttrValue> {
    if input.len() < 4 {
        return Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Eof,
        )));
    }

    let len = i32::from_be_bytes(input[..4].try_into().unwrap()) as usize;
    let data_start = 4;

    // packed 判据：首文本字节非 0（packed 字符串的第一个字符是可打印 ASCII）。
    if len > 0 && input.len() > data_start && input[data_start] != 0 {
        let packed_end = data_start + len;
        if input.len() < packed_end {
            return Err(nom::Err::Error(nom::error::make_error(
                input,
                nom::error::ErrorKind::Eof,
            )));
        }
        let string: String = input[data_start..packed_end]
            .iter()
            .map(|&b| b as char)
            .collect();
        return Ok((input, NamedAttrValue::StringType(string)));
    }

    let data_end = data_start + len * 4;

    if input.len() < data_end {
        return Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Eof,
        )));
    }

    let string: String = input[data_start..data_end]
        .chunks(4)
        .filter_map(|chunk| {
            if chunk.len() == 4 {
                let val = i32::from_be_bytes(chunk.try_into().unwrap());
                Some(val as u8 as char)
            } else {
                None
            }
        })
        .collect();

    Ok((input, NamedAttrValue::StringType(string)))
}

fn normalize_empty_string_attr(attr_name: &str, value: NamedAttrValue) -> NamedAttrValue {
    match value {
        NamedAttrValue::StringType(value)
            if value.is_empty() && matches!(attr_name.trim(), "MTOL" | "MTOQ") =>
        {
            NamedAttrValue::StringType("0".to_string())
        }
        other => other,
    }
}

/// 解析 RefU64 类型
fn parse_refno(input: &[u8]) -> IResult<&[u8], NamedAttrValue> {
    use aios_core::types::RefU64;

    if input.len() < 8 {
        return Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Eof,
        )));
    }

    let refno = RefU64::from(&input[..8]);
    Ok((input, NamedAttrValue::RefU64Type(refno)))
}

/// 解析 Word 类型
fn parse_word(input: &[u8]) -> IResult<&[u8], NamedAttrValue> {
    if input.len() < 4 {
        return Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Eof,
        )));
    }
    use aios_core::tool::db_tool::db1_dehash;

    let value = i32::from_be_bytes(input[..4].try_into().unwrap());
    let word_str = db1_dehash(value.unsigned_abs());
    Ok((input, NamedAttrValue::WordType(word_str)))
}

/// 解析 Vec3 类型
fn parse_vec3(input: &[u8], is_f32: bool) -> IResult<&[u8], NamedAttrValue> {
    if is_f32 {
        if input.len() < 16 {
            return Err(nom::Err::Error(nom::error::make_error(
                input,
                nom::error::ErrorKind::Eof,
            )));
        }
        // 长度 + 3个 f32 值
        let _len = i32::from_be_bytes(input[..4].try_into().unwrap());
        let x = parse_to_f32(&input[4..8]);
        let y = parse_to_f32(&input[8..12]);
        let z = parse_to_f32(&input[12..16]);
        Ok((input, NamedAttrValue::Vec3Type(Vec3::new(x, y, z))))
    } else {
        if input.len() < 28 {
            return Err(nom::Err::Error(nom::error::make_error(
                input,
                nom::error::ErrorKind::Eof,
            )));
        }
        // 长度 + 3个 f64 值
        let _len = i32::from_be_bytes(input[..4].try_into().unwrap());
        let x = parse_to_f64(&input[4..12]) as f32;
        let y = parse_to_f64(&input[12..20]) as f32;
        let z = parse_to_f64(&input[20..28]) as f32;
        Ok((input, NamedAttrValue::Vec3Type(Vec3::new(x, y, z))))
    }
}

/// 解析浮点数组类型
fn parse_double_array(input: &[u8], is_f32: bool) -> IResult<&[u8], NamedAttrValue> {
    if input.len() < 4 {
        return Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Eof,
        )));
    }

    let count = i32::from_be_bytes(input[..4].try_into().unwrap()) as usize;
    let data_start = 4;

    if is_f32 {
        let data_end = data_start + count * 4;
        if input.len() < data_end {
            return Err(nom::Err::Error(nom::error::make_error(
                input,
                nom::error::ErrorKind::Eof,
            )));
        }

        let values: Vec<f32> = input[data_start..data_end]
            .chunks(4)
            .filter_map(|chunk| {
                if chunk.len() == 4 {
                    Some(parse_to_f32(chunk))
                } else {
                    None
                }
            })
            .collect();

        Ok((input, NamedAttrValue::F32VecType(values)))
    } else {
        let data_end = data_start + count * 8;
        if input.len() < data_end {
            return Err(nom::Err::Error(nom::error::make_error(
                input,
                nom::error::ErrorKind::Eof,
            )));
        }

        let values: Vec<f32> = input[data_start..data_end]
            .chunks(8)
            .filter_map(|chunk| {
                if chunk.len() == 8 {
                    Some(parse_to_f64(chunk) as f32)
                } else {
                    None
                }
            })
            .collect();

        Ok((input, NamedAttrValue::F32VecType(values)))
    }
}

/// 解析整数数组类型（特殊处理 LEVEL/PTS）
fn parse_int_array<'a>(input: &'a [u8], attr_name: &str) -> IResult<&'a [u8], NamedAttrValue> {
    if input.len() < 4 {
        return Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Eof,
        )));
    }

    // 带计数头的整型数组属性按数据头读取真实数量。
    // 历史白名单只写了 "LEVEL"，但字典规范名是 LEVE（4 字符），导致 LEVE
    // 被 else 分支 count=1 截断为首元素（spec 003 A 类回归：[8,10] -> [8]，
    // E3D 真值 `Level 8 10`）。此处补 LEVE 别名。
    // 注意：不可对全部 INTVEC 通用化读计数 —— 其余整型属性的头 4 字节并非
    // 计数（实测通用化会误读邻接字节、污染同元素后续几何属性）。
    let count = if attr_name == "LEVEL" || attr_name == "LEVE" || attr_name == "PTS" {
        let raw_count = i32::from_be_bytes(input[..4].try_into().unwrap());
        // 防御：计数非法（<=0 / 过大 / 数据不足）时回退为 1。
        if raw_count > 0 && raw_count <= 4096 && input.len() >= 4 + (raw_count as usize) * 4 {
            raw_count as usize
        } else {
            1
        }
    } else {
        1
    };

    let data_start = 4;
    let data_end = data_start + count * 4;

    if input.len() < data_end {
        return Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Eof,
        )));
    }

    let values: Vec<i32> = input[data_start..data_end]
        .chunks(4)
        .filter_map(|chunk| {
            if chunk.len() == 4 {
                Some(i32::from_be_bytes(chunk.try_into().unwrap()))
            } else {
                None
            }
        })
        .collect();

    Ok((input, NamedAttrValue::IntArrayType(values)))
}

/// 检查属性是否为表达式类型
pub fn check_is_expr(hash: i32) -> bool {
    use aios_core::consts::EXPR_ATT_SET;
    EXPR_ATT_SET.contains(&hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_integer() {
        let data = [0x00, 0x00, 0x00, 0x2A]; // 42
        let (_, result) = parse_integer(&data).unwrap();
        assert!(matches!(result, NamedAttrValue::IntegerType(42)));
    }

    #[test]
    fn test_parse_double_f32() {
        let data = [0x40, 0x49, 0x0F, 0xDB]; // 3.14159 as f32
        let (_, result) = parse_double(&data, true).unwrap();
        match result {
            NamedAttrValue::F32Type(v) => {
                assert!((v - 3.14159).abs() < 0.001);
            }
            _ => panic!("Expected F32Type"),
        }
    }

    #[test]
    fn test_parse_bool() {
        let data = [0x00, 0x00, 0x00, 0x04]; // 第2位为1
        let offset = 2 << 20; // 位位置=2
        let (_, result) = parse_bool(&data, offset).unwrap();
        assert!(matches!(result, NamedAttrValue::BoolType(true)));
    }

    #[test]
    fn test_check_is_expr() {
        // 这需要实际的表达式哈希值来测试
        // 暂时跳过具体测试
    }

    #[test]
    fn test_standard_header_size() {
        // 验证标准头部大小常量
        assert_eq!(STANDARD_ELEMENT_HEADER_WORDS, 6);
        assert_eq!(STANDARD_ELEMENT_HEADER_WORDS * 4, 24); // 24 bytes
    }

    #[test]
    fn test_detect_header_size() {
        // 测试头部大小检测
        let data = vec![0u8; 100];
        assert_eq!(detect_header_size(&data), STANDARD_ELEMENT_HEADER_WORDS);

        // 测试数据不足的情况
        let short_data = vec![0u8; 10];
        assert_eq!(
            detect_header_size(&short_data),
            STANDARD_ELEMENT_HEADER_WORDS
        );
    }

    #[test]
    fn test_f32_offset_adjustment() {
        // 测试 DOUBLE 类型节省 1 word
        assert_eq!(get_f32_offset_adjustment(DbAttributeType::DOUBLE), 1);

        // 测试 Vec3 相关类型节省 3 words
        assert_eq!(get_f32_offset_adjustment(DbAttributeType::Vec3Type), 3);
        assert_eq!(get_f32_offset_adjustment(DbAttributeType::DIRECTION), 3);
        assert_eq!(get_f32_offset_adjustment(DbAttributeType::POSITION), 3);
        assert_eq!(get_f32_offset_adjustment(DbAttributeType::ORIENTATION), 3);

        // 测试其他类型不受影响
        assert_eq!(get_f32_offset_adjustment(DbAttributeType::INTEGER), 0);
        assert_eq!(get_f32_offset_adjustment(DbAttributeType::STRING), 0);
        assert_eq!(get_f32_offset_adjustment(DbAttributeType::BOOL), 0);
    }
}
