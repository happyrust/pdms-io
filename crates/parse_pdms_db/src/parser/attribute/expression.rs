//! 表达式解析器
//!
//! 提供 PDMS 表达式属性的解析功能，包括：
//! - 数学运算表达式
//! - 函数调用表达式
//! - 属性引用表达式
//! - 常量表达式

use crate::parser::numeric::{parse_explicit_f64_40, parse_explicit_num_00, parse_explicit_num_ff};
use crate::parse::convert_to_explicit_axis_string;
use aios_core::types::RefU64;
use aios_core::helper::parse_to_i16;
use aios_core::tool::db_tool::{convert_to_hash, db1_dehash};
use nom::bytes::complete::take;
use nom::number::complete::be_i32;
use nom::IResult;
use nom::Parser;
use std::collections::HashMap;

use super::axis::{is_axis_expression, parse_axis_expression_str};
use super::expression_payload::decode_expression_payload;

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

use super::opcode::{ArithmeticOpcode, TrigonometricOpcode, RealFunctionOpcode, StringFunctionOpcode, GeneralFunctionOpcode, BooleanOpcode, ComparisonOpcode, OpcodeCategory};

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
        OpcodeCategory::Equality | OpcodeCategory::NonEquality | OpcodeCategory::Comparison => apply_comparison(opcode, stack),
        OpcodeCategory::Arithmetic => apply_arithmetic(opcode, stack),
        OpcodeCategory::Trigonometric => apply_trigonometric(opcode, stack),
        OpcodeCategory::RealFunctions => apply_real_function(opcode, stack),
        OpcodeCategory::StringFunctions | OpcodeCategory::ConversionFunctions => apply_string_function(opcode, stack),
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
pub fn parse_expression_attr(input: &[u8], refno: u64) -> IResult<&[u8], (String, String)> {
    let (input, hash_val) = take(4usize).parse(input)?;
    let expression_type = db1_dehash(convert_to_hash(hash_val).unsigned_abs());

    // 判断是否为轴向表达式
    if is_axis_expression(input)? {
        parse_axis_expression_str(input, expression_type)
    } else {
        match parse_other_expression(input, expression_type.clone(), refno) {
            Ok(result) => Ok(result),
            Err(_) => crate::parse_explict_tools::parse_expression_attr(input, RefU64(refno)),
        }
    }
}

/// 解析非轴向表达式
///
/// 支持的类型：
/// - 字符串类型 (flag == 0x66)
/// - PTCDI/PTCD 类型
/// - 通用表达式类型
pub fn parse_other_expression(
    input: &[u8],
    expression_type: String,
    refno: u64,
) -> IResult<&[u8], (String, String)> {
    // 检查最小长度
    if input.len() < 20 {
        return Ok((input, (expression_type, String::new())));
    }

    // 检查是否为字符串类型 (flag at offset 16 == 0x66)
    let (_, flag) = be_i32(&input[16..20])?;
    if flag == 0x66 {
        return parse_string_expression(input, expression_type);
    }

    // PTCDI/PTCD 类型处理
    if expression_type == "PTCDI" || expression_type == "PTCD" {
        return parse_ptcd_expression(input, expression_type, refno);
    }

    // 通用表达式处理：基于 payload→postfix→pretty 的复刻实现
    match decode_expression_payload(input) {
        Ok((consumed, value)) if !value.trim().is_empty() => {
            let trimmed = value.trim();
            let numeric_only = trimmed.parse::<f64>().is_ok();
            if numeric_only && input.len() > 32 {
                return Err(nom::Err::Error(nom::error::make_error(
                    input,
                    nom::error::ErrorKind::Verify,
                )));
            }
            Ok((&input[consumed..], (expression_type, value)))
        }
        _ => Err(nom::Err::Error(nom::error::make_error(
            input,
            nom::error::ErrorKind::Verify,
        ))),
    }
}

/// 解析字符串表达式
fn parse_string_expression(
    input: &[u8],
    expression_type: String,
) -> IResult<&[u8], (String, String)> {
    if input.len() < 24 {
        return Ok((input, (expression_type, String::new())));
    }

    let (_, str_len) = be_i32(&input[20..24])?;
    let str_len = str_len as usize;

    if input.len() < 24 + str_len * 4 {
        return Ok((input, (expression_type, String::new())));
    }

    let string: String = input[24..24 + str_len * 4]
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

    let result = format!("'{}'", string);
    Ok((&input[24 + str_len * 4..], (expression_type, result)))
}

/// 解析 PTCD/PTCDI 表达式
fn parse_ptcd_expression(
    input: &[u8],
    expression_type: String,
    refno: u64,
) -> IResult<&[u8], (String, String)> {
    // 复刻旧实现的长度与切片规则（PTCD/PTCDI 的 payload 结构与通用表达式不同）
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

    // 显式属性的 length 后有 4 bytes “无用区”，旧实现从 offset 4 开始取数据。
    let expression_data = &input[4..end];
    let (_rest, axis) = convert_to_explicit_axis_string(expression_data, RefU64(refno))?;
    let axis_result = match axis {
        aios_core::AttrVal::StringType(value) => value,
        _ => String::new(),
    };

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
    fn test_parse_ptcd_expression_incomplete_length_should_error() {
        // 伪造 PTCD/PTCDI: expression_length=10 => end=44，但 input 只有 8 bytes
        let input = [0u8, 0u8, 0x00, 0x0A, 0u8, 0u8, 0u8, 0u8];
        let err = parse_ptcd_expression(&input, "PTCD".to_string(), 0).unwrap_err();
        assert!(matches!(err, nom::Err::Incomplete(_)));
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
        let mut stack = vec!["condition".to_string(), "true_val".to_string(), "false_val".to_string()];
        let result = apply_operator(1822, &mut stack);
        assert_eq!(result, Some("IFTRUE(condition,true_val,false_val)".to_string()));
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
