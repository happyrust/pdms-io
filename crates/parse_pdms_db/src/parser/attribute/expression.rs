//! 表达式解析器
//!
//! 提供 PDMS 表达式属性的解析功能，包括：
//! - 数学运算表达式
//! - 函数调用表达式
//! - 属性引用表达式
//! - 常量表达式

use crate::parse::convert_to_explicit_axis_string;
use crate::parser::numeric::{parse_explicit_f64_40, parse_explicit_num_00, parse_explicit_num_ff};
use aios_core::helper::parse_to_i16;
use aios_core::tool::db_tool::{convert_to_hash, db1_dehash};
use aios_core::types::RefU64;
use nom::IResult;
use nom::Parser;
use nom::bytes::complete::take;
use nom::number::complete::be_i32;
use std::collections::HashMap;

use super::axis::{is_axis_expression, parse_axis_expression_str};
use super::expression_payload::decode_expression_payload;
#[cfg(feature = "debug_parse")]
use super::expression_payload::scan_expression_payload_opcodes;

#[cfg(feature = "debug_parse")]
mod expr_fallback_stats {
    use std::sync::atomic::{AtomicU64, Ordering};

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct ExpressionParseStats {
        pub new_ok: u64,
        pub new_err: u64,
        pub fallback_ok: u64,
        pub fallback_err: u64,
    }

    static NEW_OK: AtomicU64 = AtomicU64::new(0);
    static NEW_ERR: AtomicU64 = AtomicU64::new(0);
    static FALLBACK_OK: AtomicU64 = AtomicU64::new(0);
    static FALLBACK_ERR: AtomicU64 = AtomicU64::new(0);

    pub fn inc_new_ok() {
        NEW_OK.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_new_err() {
        NEW_ERR.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_fallback_ok() {
        FALLBACK_OK.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_fallback_err() {
        FALLBACK_ERR.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot() -> ExpressionParseStats {
        ExpressionParseStats {
            new_ok: NEW_OK.load(Ordering::Relaxed),
            new_err: NEW_ERR.load(Ordering::Relaxed),
            fallback_ok: FALLBACK_OK.load(Ordering::Relaxed),
            fallback_err: FALLBACK_ERR.load(Ordering::Relaxed),
        }
    }

    #[cfg(test)]
    pub fn reset_for_test() {
        NEW_OK.store(0, Ordering::Relaxed);
        NEW_ERR.store(0, Ordering::Relaxed);
        FALLBACK_OK.store(0, Ordering::Relaxed);
        FALLBACK_ERR.store(0, Ordering::Relaxed);
    }
}

/// 读取表达式解析回退统计（需启用 `parse_pdms_db` 的 `debug_parse` feature）。
#[cfg(feature = "debug_parse")]
pub fn expression_parse_stats() -> expr_fallback_stats::ExpressionParseStats {
    expr_fallback_stats::snapshot()
}

/// 数学运算符映射表
///
/// 将 PDMS 内部操作码映射为可读的表达式格式
pub fn get_math_operators() -> HashMap<i32, &'static str> {
    let mut map = HashMap::new();
    // 比较运算符
    map.insert(0x191, "{} EQ {}");
    map.insert(0x1F5, "{} NEQ {}");
    map.insert(0x259, "{} GT {}");
    map.insert(0x25B, "{} LT {}");
    map.insert(0x25D, "{} GE {}");
    map.insert(0x25F, "{} LE {}");
    // 算术运算符
    map.insert(0x321, "( -{} )");
    map.insert(0x322, "( {} + {} )");
    map.insert(0x323, "( {} - {} )");
    map.insert(0x324, "( {} * {} )");
    map.insert(0x325, "( {} / {} )");
    // 数学函数
    map.insert(0x3E9, "SQRT( {} )");
    map.insert(0x385, "SIN( {} )");
    map.insert(0x386, "COS( {} )");
    map.insert(0x387, "TAN( {} )");
    map.insert(0x388, "ASIN( {} )");
    map.insert(0x389, "ACOS( {} )");
    map.insert(0x38A, "ATAN( {} )");
    map.insert(0x38B, "ATANT( {}, {} )");
    map.insert(0x3EA, "POW( {}, {} )");
    map.insert(0x3EB, "LOG( {} )");
    map.insert(0x3EC, "ALOG( {} )");
    map.insert(0x3ED, "INT( {} )");
    map.insert(0x3EE, "NINT( {} )");
    map.insert(0x3EF, "ABS( {} )");
    // 字符串函数
    map.insert(0x515, "LEN( '{}' )");
    map.insert(0x51C, "MAT( {}, '{}' )");
    map.insert(0x522, "TRIM( {} )");
    map.insert(0x529, "OCCUR( '{}', '{}' )");
    map.insert(0x579, "REAL( '{}' )");
    map.insert(0x582, "STR( {} )");
    map
}

// ============================================================================
// 基于 opcode 枚举的运算符处理函数
// ============================================================================

use super::opcode::{
    ArithmeticOpcode, BooleanOpcode, ComparisonOpcode, GeneralFunctionOpcode, OpcodeCategory,
    RealFunctionOpcode, StringFunctionOpcode, TrigonometricOpcode,
};

/// 使用 opcode 枚举应用运算符到操作数栈
///
/// # 参数
/// - `opcode`: 操作码（十进制或十六进制均可）
/// - `stack`: 操作数栈
///
/// # 返回
/// 运算成功返回 `Some(结果字符串)`，否则返回 `None`
pub fn apply_operator(opcode: i32, stack: &mut Vec<String>) -> Option<String> {
    // 按操作码类别分发
    match OpcodeCategory::from(opcode) {
        OpcodeCategory::Boolean => apply_boolean(opcode, stack),
        OpcodeCategory::Equality | OpcodeCategory::NonEquality | OpcodeCategory::Comparison => {
            apply_comparison(opcode, stack)
        }
        OpcodeCategory::Arithmetic => apply_arithmetic(opcode, stack),
        OpcodeCategory::Trigonometric => apply_trigonometric(opcode, stack),
        OpcodeCategory::RealFunctions => apply_real_function(opcode, stack),
        OpcodeCategory::StringFunctions | OpcodeCategory::ConversionFunctions => {
            apply_string_function(opcode, stack)
        }
        OpcodeCategory::GeneralFunctions => apply_general_function(opcode, stack),
        _ => None,
    }
}

/// 应用布尔运算符（NOT, AND, OR）
fn apply_boolean(opcode: i32, stack: &mut Vec<String>) -> Option<String> {
    let op = BooleanOpcode::try_from(opcode).ok()?;

    match op.operand_count() {
        1 => {
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a]))
        }
        2 => {
            let b = stack.pop()?;
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a, b]))
        }
        _ => None,
    }
}

/// 应用比较运算符（EQ, NEQ, GT, LT, GE, LE）
fn apply_comparison(opcode: i32, stack: &mut Vec<String>) -> Option<String> {
    let op = ComparisonOpcode::try_from(opcode).ok()?;

    // 比较运算符都是双操作数
    let b = stack.pop()?;
    let a = stack.pop()?;
    Some(format_operator(op.format_template(), &[a, b]))
}

/// 应用算术运算符
fn apply_arithmetic(opcode: i32, stack: &mut Vec<String>) -> Option<String> {
    let op = ArithmeticOpcode::try_from(opcode).ok()?;

    match op.operand_count() {
        1 => {
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a]))
        }
        2 => {
            let b = stack.pop()?;
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a, b]))
        }
        _ => None,
    }
}

/// 应用三角函数运算符
fn apply_trigonometric(opcode: i32, stack: &mut Vec<String>) -> Option<String> {
    let op = TrigonometricOpcode::try_from(opcode).ok()?;

    match op.operand_count() {
        1 => {
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a]))
        }
        2 => {
            let b = stack.pop()?;
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a, b]))
        }
        _ => None,
    }
}

/// 应用实数函数运算符
fn apply_real_function(opcode: i32, stack: &mut Vec<String>) -> Option<String> {
    let op = RealFunctionOpcode::try_from(opcode).ok()?;

    match op.operand_count() {
        1 => {
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a]))
        }
        2 => {
            let b = stack.pop()?;
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a, b]))
        }
        _ => None,
    }
}

/// 应用字符串函数运算符
fn apply_string_function(opcode: i32, stack: &mut Vec<String>) -> Option<String> {
    let op = StringFunctionOpcode::try_from(opcode).ok()?;

    match op.operand_count() {
        1 => {
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a]))
        }
        2 => {
            let b = stack.pop()?;
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a, b]))
        }
        3 => {
            let c = stack.pop()?;
            let b = stack.pop()?;
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a, b, c]))
        }
        _ => None,
    }
}

/// 应用通用函数运算符（IFTRUE, DISTCONVERT, SET, UNSET 等）
fn apply_general_function(opcode: i32, stack: &mut Vec<String>) -> Option<String> {
    let op = GeneralFunctionOpcode::try_from(opcode).ok()?;

    match op.operand_count() {
        1 => {
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a]))
        }
        2 => {
            let b = stack.pop()?;
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a, b]))
        }
        3 => {
            let c = stack.pop()?;
            let b = stack.pop()?;
            let a = stack.pop()?;
            Some(format_operator(op.format_template(), &[a, b, c]))
        }
        _ => None,
    }
}

/// 格式化运算符模板
fn format_operator(template: &str, args: &[String]) -> String {
    let mut result = template.to_string();
    for arg in args {
        result = result.replacen("{}", arg, 1);
    }
    result
}

/// 特殊常量映射
pub fn parse_expression_const(input: &[u8]) -> &'static str {
    if input.len() < 4 {
        return "";
    }
    match input[..4] {
        [0, 0, 0, 0x6F] => "PI",
        _ => "",
    }
}

/// 特殊函数表达式
pub fn get_expression_of_func(input: &[u8]) -> &'static str {
    if input.len() < 4 {
        return "";
    }
    match input[..4] {
        [0, 0, 0, 0xA] => "PREV",
        [0, 0, 0, 0xB] => "NEXT",
        [0x0, 0xA, 0x1D, 0xCB] => "BLRF NUM 1",
        [0x0, 0xD, 0xBC, 0xF9] => "CATR",
        [0x0, 0xD, 0x24, 0x5B] => "BLTP 1",
        _ => "",
    }
}

/// 解析表达式属性
///
/// # 格式
/// - input[0..4]: hash 值，确定表达式类型
/// - input[4..]: 表达式具体数据
///
/// # 返回
/// `(表达式类型名称, 表达式值字符串)`
/// 解析"方向字面量"表达式（PTCD/Ptcdirection 等 DIRECTION 属性的常量编码）。
///
/// 实测字节模式（ams5054 全库 PTCA 元素统计,7 words 定长）:
/// `06 06 05 02 15 <dir_token> 3D`
/// - 前缀 `06 06 05 02 15` 为固定 RPN 头(push direction literal);
/// - `dir_token`: 11..=16 → X, -X, Y, -Y, Z, -Z
///   (金标准: =13246/243899 token=13 → "Y"; =13246/243891 token=16 → "AXIS -Z");
/// - `3D` 为表达式结束符。
///
/// 输入为已剥离 attr hash 的表达式 payload。
fn parse_direction_literal_expression(input: &[u8]) -> Option<&'static str> {
    if input.len() < 28 {
        return None;
    }
    let word =
        |idx: usize| -> u32 { u32::from_be_bytes(input[idx * 4..idx * 4 + 4].try_into().unwrap()) };
    if word(0) != 0x06 || word(1) != 0x06 || word(2) != 0x05 || word(3) != 0x02 {
        return None;
    }
    if word(4) != 0x15 || word(6) != 0x3D {
        return None;
    }
    match word(5) {
        11 => Some("X"),
        12 => Some("-X"),
        13 => Some("Y"),
        14 => Some("-Y"),
        15 => Some("Z"),
        16 => Some("-Z"),
        _ => None,
    }
}

/// 解析"数值字面量"表达式（PHEI/Pheight 等标量属性的常量编码,5 words 定长）。
///
/// 实测字节模式: `04 <00000028> <00000001> <value_i32> <00000000>`
/// - `0x28` 为符号/倍数段、`0x01` 为数字段(见 parse.rs flag==4 路径注释);
/// - 数值 = -(value_i32) / 10
///   (金标准: =13246/243891 PHEI value=-2230 → 223;
///    同款编码在隐式区 =13246/243899 PX value=-4110 → 411)。
fn parse_numeric_literal_expression(input: &[u8]) -> Option<String> {
    if input.len() < 20 {
        return None;
    }
    let word =
        |idx: usize| -> u32 { u32::from_be_bytes(input[idx * 4..idx * 4 + 4].try_into().unwrap()) };
    if word(0) != 0x04 || word(1) != 0x28 || word(2) != 0x01 || word(4) != 0 {
        return None;
    }
    let raw = word(3) as i32;
    let value = -(raw as f64) / 10.0;
    if (value - value.round()).abs() < f64::EPSILON {
        Some(format!("{}", value as i64))
    } else {
        Some(value.to_string())
    }
}

pub fn parse_expression_attr(input: &[u8], refno: u64) -> IResult<&[u8], (String, String)> {
    let (input, hash_val) = take(4usize).parse(input)?;
    let expression_type = db1_dehash(convert_to_hash(hash_val).unsigned_abs());

    // 方向字面量（PTCD 等 DIRECTION 常量）：固定 7-word 模式，优先精确匹配
    if let Some(direction) = parse_direction_literal_expression(input) {
        return Ok((&input[28..], (expression_type, direction.to_string())));
    }

    // 数值字面量（PHEI 等标量常量）：固定 5-word 模式
    if let Some(number) = parse_numeric_literal_expression(input) {
        return Ok((&input[20..], (expression_type, number)));
    }

    // 判断是否为轴向表达式
    if is_axis_expression(input)? {
        parse_axis_expression_str(input, expression_type)
    } else {
        match parse_other_expression(input, expression_type.clone(), refno) {
            Ok(result) => {
                #[cfg(feature = "debug_parse")]
                expr_fallback_stats::inc_new_ok();
                Ok(result)
            }
            Err(e) => {
                #[cfg(feature = "debug_parse")]
                {
                    expr_fallback_stats::inc_new_err();
                    log::debug!(
                        "expression parse fallback: refno={:?}, type={}, new_err={:?}",
                        RefU64(refno),
                        expression_type,
                        e
                    );
                }

                // 回退到旧实现（历史兼容）
                match crate::parse_explict_tools::parse_expression_attr(input, RefU64(refno)) {
                    Ok(ok) => {
                        #[cfg(feature = "debug_parse")]
                        expr_fallback_stats::inc_fallback_ok();
                        Ok(ok)
                    }
                    Err(e2) => {
                        #[cfg(feature = "debug_parse")]
                        {
                            expr_fallback_stats::inc_fallback_err();
                            log::debug!(
                                "expression parse fallback failed: refno={:?}, type={}, old_err={:?}",
                                RefU64(refno),
                                expression_type,
                                e2
                            );
                        }
                        Err(e2)
                    }
                }
            }
        }
    }
}

/// 解析非轴向表达式
///
/// 支持的类型：
/// - 字符串类型 (flag == 0x66)
/// - 通用表达式类型
pub fn parse_other_expression(
    input: &[u8],
    expression_type: String,
    refno: u64,
) -> IResult<&[u8], (String, String)> {
    // 字符串类型：按结构判断（避免依赖 expression_type 等“特殊情况”）
    if input.len() >= 20 {
        let flag = i32::from_be_bytes(input[16..20].try_into().unwrap_or([0; 4]));
        if flag == 0x66 {
            return parse_string_expression(input, expression_type);
        }
    }

    // 通用表达式：payload → postfix → pretty
    match decode_expression_payload(input) {
        Ok((consumed, value)) if !value.trim().is_empty() => {
            return Ok((&input[consumed..], (expression_type, value)));
        }
        Ok(_) => {}
        Err(e) => {
            // debug_parse 下尽量提供“通用线索”，避免为某个 expression_type 单独写特判。
            #[cfg(feature = "debug_parse")]
            {
                if let Ok(report) = scan_expression_payload_opcodes(input) {
                    // 取 top 6 opcode（按频次降序）
                    let mut top = report
                        .opcode_counts
                        .iter()
                        .map(|(k, v)| (*k, *v))
                        .collect::<Vec<_>>();
                    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                    top.truncate(6);

                    log::debug!(
                        "expression payload decode failed: refno={:?}, type={}, err={:?}, start_words={}, declared_words={}, unknown={:?}, top={:?}",
                        RefU64(refno),
                        expression_type,
                        e,
                        report.start_words,
                        report.declared_words,
                        report.unknown_opcodes,
                        top,
                    );
                } else {
                    log::debug!(
                        "expression payload decode failed: refno={:?}, type={}, err={:?} (opcode scan failed)",
                        RefU64(refno),
                        expression_type,
                        e,
                    );
                }
            }
        }
    };

    // 轴向/坐标类显式表达式：尝试按“显式轴向字符串”通用规则解析
    if let Ok(result) = parse_explicit_axis_string_expression(input, expression_type.clone(), refno)
    {
        return Ok(result);
    }

    Err(nom::Err::Error(nom::error::make_error(
        input,
        nom::error::ErrorKind::Verify,
    )))
}

/// 解析字符串表达式
fn parse_string_expression(
    input: &[u8],
    expression_type: String,
) -> IResult<&[u8], (String, String)> {
    if input.len() < 24 {
        return Err(nom::Err::Incomplete(nom::Needed::new(24 - input.len())));
    }

    let (_, str_len) = be_i32(&input[20..24])?;
    if str_len < 0 {
        return Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Verify,
        )));
    }
    let str_len = str_len as usize;

    let total = 24usize
        .checked_add(str_len.checked_mul(4).ok_or_else(|| {
            nom::Err::Error(nom::error::make_error(
                input,
                nom::error::ErrorKind::TooLarge,
            ))
        })?)
        .ok_or_else(|| {
            nom::Err::Error(nom::error::make_error(
                input,
                nom::error::ErrorKind::TooLarge,
            ))
        })?;

    if input.len() < total {
        return Err(nom::Err::Incomplete(nom::Needed::new(total - input.len())));
    }

    // PDMS 中 string expr 通常为“每字符占 4 字节”，我们取低 8 bit 还原为 bytes，再按 UTF-8(损失容错) 组装。
    let mut bytes = Vec::with_capacity(str_len);
    for chunk in input[24..total].chunks(4) {
        let val = i32::from_be_bytes(chunk.try_into().unwrap_or([0; 4]));
        bytes.push((val as u32 & 0xFF) as u8);
    }
    while bytes.last().copied() == Some(0) {
        bytes.pop();
    }
    let string = String::from_utf8_lossy(&bytes).to_string();
    let escaped = string.replace('\'', "''");

    let result = format!("'{}'", escaped);
    Ok((&input[total..], (expression_type, result)))
}

/// 尝试将表达式解析为“显式轴向字符串”（如 PTCD/PTCDI 这类）。
///
/// 该结构在数据库中并不总是与 postfix payload 兼容，故作为通用回退路径之一。
fn parse_explicit_axis_string_expression(
    input: &[u8],
    expression_type: String,
    refno: u64,
) -> IResult<&[u8], (String, String)> {
    // 该类结构的长度字段为 u16，且 length 后有 4 bytes “无用区”，数据从 offset 4 开始。
    if input.len() < 4 {
        return Err(nom::Err::Incomplete(nom::Needed::Unknown));
    }

    // 读取表达式长度
    let (_, expression_length) = nom::number::complete::be_u16(&input[2..4])?;
    let end = (expression_length as usize)
        .checked_mul(4)
        .and_then(|v| v.checked_add(4))
        .ok_or_else(|| nom::Err::Incomplete(nom::Needed::Unknown))?;

    if end > input.len() {
        return Err(nom::Err::Incomplete(nom::Needed::Unknown));
    }

    let expression_data = &input[4..end];
    // 结构门槛：显式轴向字符串（如 PTCD/PTCDI）在实际数据中通常是较长的显式块。
    // 过短的数据更可能是其它表达式/残缺数据；若仍尝试走 convert_to_explicit_axis_string，
    // 容易误判并“成功”返回非空字符串，从而掩盖真正的 payload 结构。
    if expression_data.len() < 20 {
        return Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Verify,
        )));
    }
    let (_rest, axis) = convert_to_explicit_axis_string(expression_data, RefU64(refno))?;
    let axis_result = match axis {
        aios_core::AttrVal::StringType(value) => value,
        _ => String::new(),
    };

    if axis_result.trim().is_empty() {
        return Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Verify,
        )));
    }

    Ok((&input[end..], (expression_type, axis_result)))
}

/// 解析表达式中的数值
///
/// 根据标志位选择合适的解析器
pub fn parse_expression_number(input: &[u8]) -> IResult<&[u8], f64> {
    if input.len() < 12 {
        return Err(nom::Err::Incomplete(nom::Needed::new(12 - input.len())));
    }

    let num_flag = parse_to_i16(&input[8..10]);
    match num_flag {
        0 => parse_explicit_num_00(input),
        0x4000 => parse_explicit_f64_40(input),
        -1 => parse_explicit_num_ff(input),
        _ => Ok((input, 0.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_math_operators() {
        let ops = get_math_operators();
        assert_eq!(ops.get(&0x322), Some(&"( {} + {} )"));
        assert_eq!(ops.get(&0x324), Some(&"( {} * {} )"));
        assert_eq!(ops.get(&0x385), Some(&"SIN( {} )"));
    }

    #[test]
    fn test_parse_expression_const() {
        assert_eq!(parse_expression_const(&[0, 0, 0, 0x6F]), "PI");
        assert_eq!(parse_expression_const(&[0, 0, 0, 0]), "");
        assert_eq!(parse_expression_const(&[0, 0]), "");
    }

    #[test]
    fn test_get_expression_of_func() {
        assert_eq!(get_expression_of_func(&[0, 0, 0, 0xA]), "PREV");
        assert_eq!(get_expression_of_func(&[0, 0, 0, 0xB]), "NEXT");
        assert_eq!(get_expression_of_func(&[0, 0, 0, 0]), "");
    }

    #[test]
    fn test_parse_explicit_axis_string_expression_incomplete_length_should_error() {
        // 伪造 axis-string 表达式: expression_length=10 => end=44，但 input 只有 8 bytes
        let input = [0u8, 0u8, 0x00, 0x0A, 0u8, 0u8, 0u8, 0u8];
        let err = parse_explicit_axis_string_expression(&input, "PTCD".to_string(), 0).unwrap_err();
        assert!(matches!(err, nom::Err::Incomplete(_)));
    }

    #[test]
    fn test_parse_explicit_axis_string_expression_rejects_short_blocks() {
        // 构造一个“显式轴向字符串”外壳，但 expression_data 仅 2 words(8 bytes)。
        // 该类短块不应进入显式轴向字符串解析，避免误判。
        let mut input = Vec::new();
        input.extend_from_slice(&[0x00, 0x00]); // reserved
        input.extend_from_slice(&2u16.to_be_bytes()); // length_words=2 => end=12
        input.extend_from_slice(&[0u8; 8]); // expression_data

        let err = parse_explicit_axis_string_expression(&input, "ANY".to_string(), 0).unwrap_err();
        assert!(matches!(err, nom::Err::Error(_)));
    }

    #[test]
    fn test_parse_explicit_axis_string_expression_accepts_known_axis_pattern() {
        // 构造一个满足 convert_to_explicit_axis_string 的已知模式：返回 "-Z"。
        // expression_data 至少 5*u32(20 bytes) 才会进入显式分支，随后默认分支识别 [0x10,0x3D]。
        let mut expression_data = Vec::new();
        for _ in 0..5 {
            expression_data.extend_from_slice(&0u32.to_be_bytes());
        }
        // 默认分支匹配：00 00 00 10 00 00 00 3D => "-Z"
        expression_data.extend_from_slice(&0x0000_0010u32.to_be_bytes());
        expression_data.extend_from_slice(&0x0000_003Du32.to_be_bytes());

        let length_words: u16 = (expression_data.len() / 4) as u16;

        let mut input = Vec::new();
        input.extend_from_slice(&[0x00, 0x00]); // reserved
        input.extend_from_slice(&length_words.to_be_bytes());
        input.extend_from_slice(&expression_data);

        let (rest, (_ty, value)) =
            parse_explicit_axis_string_expression(&input, "ANY".to_string(), 0).unwrap();
        assert!(rest.is_empty());
        assert_eq!(value, "-Z");
    }

    #[test]
    fn test_parse_other_expression_string_incomplete_should_error() {
        // flag==0x66 但缺少字符串内容：应当返回 Incomplete，而非静默返回空串。
        // 结构：header(24) = 6 words，其中 flag@16..20, str_len@20..24
        let mut input = vec![0u8; 24];
        input[16..20].copy_from_slice(&0x66i32.to_be_bytes());
        input[20..24].copy_from_slice(&2i32.to_be_bytes()); // 需要 2 个 char word，但不给

        let err = parse_other_expression(&input, "PSTR".to_string(), 0).unwrap_err();
        assert!(matches!(err, nom::Err::Incomplete(_)));
    }

    #[test]
    fn test_parse_other_expression_string_escape_single_quote() {
        // 构造字符串 A'B，期望输出 'A''B'
        let mut input = vec![0u8; 24 + 3 * 4];
        input[16..20].copy_from_slice(&0x66i32.to_be_bytes());
        input[20..24].copy_from_slice(&3i32.to_be_bytes());
        // 'A' '\'' 'B'
        input[24..28].copy_from_slice(&0x41i32.to_be_bytes());
        input[28..32].copy_from_slice(&0x27i32.to_be_bytes());
        input[32..36].copy_from_slice(&0x42i32.to_be_bytes());

        let (rest, (_ty, value)) = parse_other_expression(&input, "PSTR".to_string(), 0).unwrap();
        assert!(rest.is_empty());
        assert_eq!(value, "'A''B'");
    }

    #[cfg(feature = "debug_parse")]
    #[test]
    fn test_expression_parse_stats_new_ok_increments_on_string_expr() {
        expr_fallback_stats::reset_for_test();

        // hash(4) + header(24) + 1 char word(4)
        // - flag@16..20 = 0x66
        // - str_len@20..24 = 1
        let mut bytes = vec![0u8; 4 + 24 + 4];
        // flag
        bytes[4 + 16..4 + 20].copy_from_slice(&0x66i32.to_be_bytes());
        // str_len
        bytes[4 + 20..4 + 24].copy_from_slice(&1i32.to_be_bytes());
        // 'A'
        bytes[4 + 24..4 + 28].copy_from_slice(&0x41i32.to_be_bytes());

        let (_rest, (_ty, value)) = parse_expression_attr(&bytes, 0).unwrap();
        assert_eq!(value, "'A'");

        let stats = expression_parse_stats();
        assert_eq!(stats.new_ok, 1);
        assert_eq!(stats.new_err, 0);
        assert_eq!(stats.fallback_ok, 0);
        assert_eq!(stats.fallback_err, 0);
    }

    #[cfg(feature = "debug_parse")]
    #[test]
    fn test_expression_parse_stats_fallback_err_increments_on_invalid_payload() {
        expr_fallback_stats::reset_for_test();

        // hash(4) + payload(20 zeros) => 非字符串、非 PTCD，payload 解码会失败，触发回退；
        // 回退解析同样会失败（缺少完整结构），因此 fallback_err++。
        let bytes = vec![0u8; 4 + 20];
        let _ = parse_expression_attr(&bytes, 0);

        let stats = expression_parse_stats();
        assert_eq!(stats.new_ok, 0);
        assert_eq!(stats.new_err, 1);
        assert_eq!(stats.fallback_ok, 0);
        assert_eq!(stats.fallback_err, 1);
    }

    #[test]
    fn test_apply_operator_arithmetic() {
        // 测试加法 (802 = 0x322)
        let mut stack = vec!["a".to_string(), "b".to_string()];
        let result = apply_operator(802, &mut stack);
        assert_eq!(result, Some("(a+b)".to_string()));
        assert!(stack.is_empty());

        // 测试取负 (801 = 0x321)
        let mut stack = vec!["x".to_string()];
        let result = apply_operator(801, &mut stack);
        assert_eq!(result, Some("(-x)".to_string()));
    }

    #[test]
    fn test_apply_operator_trigonometric() {
        // 测试 SIN (901 = 0x385)
        let mut stack = vec!["45".to_string()];
        let result = apply_operator(901, &mut stack);
        assert_eq!(result, Some("SIN(45)".to_string()));

        // 测试 COS (902 = 0x386)
        let mut stack = vec!["90".to_string()];
        let result = apply_operator(902, &mut stack);
        assert_eq!(result, Some("COS(90)".to_string()));
    }

    #[test]
    fn test_apply_operator_real_functions() {
        // 测试 MAX (1008 = 0x3F0)
        let mut stack = vec!["a".to_string(), "b".to_string()];
        let result = apply_operator(1008, &mut stack);
        assert_eq!(result, Some("MAX(a,b)".to_string()));

        // 测试 SQRT (1001 = 0x3E9)
        let mut stack = vec!["16".to_string()];
        let result = apply_operator(1001, &mut stack);
        assert_eq!(result, Some("SQRT(16)".to_string()));
    }

    #[test]
    fn test_apply_operator_string_functions() {
        // 测试一元函数 LENGTH (1301 = 0x515)
        let mut stack = vec!["'hello'".to_string()];
        let result = apply_operator(1301, &mut stack);
        assert_eq!(result, Some("LEN('hello')".to_string()));
        assert!(stack.is_empty());

        // 测试一元函数 TRIM (1314 = 0x522)
        let mut stack = vec!["mystring".to_string()];
        let result = apply_operator(1314, &mut stack);
        assert_eq!(result, Some("TRIM(mystring)".to_string()));

        // 测试二元函数 OCCURS (1321 = 0x529)
        let mut stack = vec!["text".to_string(), "pattern".to_string()];
        let result = apply_operator(1321, &mut stack);
        assert_eq!(result, Some("OCCURS(text,pattern)".to_string()));

        // 测试三元函数 SUBSTRING (1309 = 0x51D)
        let mut stack = vec!["str".to_string(), "1".to_string(), "5".to_string()];
        let result = apply_operator(1309, &mut stack);
        assert_eq!(result, Some("SUBSTRING(str,1,5)".to_string()));
        assert!(stack.is_empty());
    }

    #[test]
    fn test_apply_operator_boolean() {
        // 测试 NOT (301 = 0x12D)
        let mut stack = vec!["true".to_string()];
        let result = apply_operator(301, &mut stack);
        assert_eq!(result, Some("NOT(true)".to_string()));
        assert!(stack.is_empty());

        // 测试 AND (302 = 0x12E)
        let mut stack = vec!["a".to_string(), "b".to_string()];
        let result = apply_operator(302, &mut stack);
        assert_eq!(result, Some("a AND b".to_string()));

        // 测试 OR (303 = 0x12F)
        let mut stack = vec!["x".to_string(), "y".to_string()];
        let result = apply_operator(303, &mut stack);
        assert_eq!(result, Some("x OR y".to_string()));
    }

    #[test]
    fn test_apply_operator_comparison() {
        // 测试 EQ (401 = 0x191)
        let mut stack = vec!["a".to_string(), "b".to_string()];
        let result = apply_operator(401, &mut stack);
        assert_eq!(result, Some("a EQ b".to_string()));

        // 测试 LT (603 = 0x25B)
        let mut stack = vec!["x".to_string(), "5".to_string()];
        let result = apply_operator(603, &mut stack);
        assert_eq!(result, Some("x LT 5".to_string()));

        // 测试 GE (605 = 0x25D)
        let mut stack = vec!["count".to_string(), "10".to_string()];
        let result = apply_operator(605, &mut stack);
        assert_eq!(result, Some("count GE 10".to_string()));
    }

    #[test]
    fn test_apply_operator_general_functions() {
        // 测试 IFTRUE (1822 = 0x071E)
        let mut stack = vec![
            "condition".to_string(),
            "true_val".to_string(),
            "false_val".to_string(),
        ];
        let result = apply_operator(1822, &mut stack);
        assert_eq!(
            result,
            Some("IFTRUE(condition,true_val,false_val)".to_string())
        );
        assert!(stack.is_empty());

        // 测试 UNSET (1826 = 0x0722)
        let mut stack = vec!["attr".to_string()];
        let result = apply_operator(1826, &mut stack);
        assert_eq!(result, Some("UNSET(attr)".to_string()));

        // 测试 DISTCONVERT (1824 = 0x0720)
        let mut stack = vec!["100".to_string()];
        let result = apply_operator(1824, &mut stack);
        assert_eq!(result, Some("DISTCONVERT(100)".to_string()));
    }
}
