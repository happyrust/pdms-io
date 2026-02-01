use aios_core::tool::db_tool::db1_dehash;
use std::collections::BTreeMap;

use super::opcode::OpcodeCategory;

#[derive(Debug, Clone)]
pub enum DecodeError {
    InvalidPayload,
    InvalidLength,
    UnexpectedEof,
    StackUnderflow,
    UnknownOpcode(i32),
}

#[derive(Debug, Clone)]
struct ExprNode {
    text: String,
    prec: i32,
}

impl ExprNode {
    fn new(text: String, prec: i32) -> Self {
        Self { text, prec }
    }
}

const PREC_OR: i32 = 10;
const PREC_AND: i32 = 20;
const PREC_NOT: i32 = 30;
const PREC_COMPARE: i32 = 40;
const PREC_NEG: i32 = 80;
const PREC_ADD: i32 = 60;
const PREC_MUL: i32 = 70;
const PREC_FUNC: i32 = 90;
const PREC_VALUE: i32 = 100;

pub fn decode_expression_payload(input: &[u8]) -> Result<(usize, String), DecodeError> {
    let mut best: Option<(usize, String, usize)> = None;
    for start_words in 0..=2 {
        if let Ok((consumed, expr, len)) = decode_from_start(input, start_words) {
            let trimmed = expr.trim();
            if trimmed.is_empty() {
                continue;
            }
            if len >= 6 && trimmed.len() <= 1 {
                continue;
            }
            let pick = match best {
                Some((_, _, best_len)) => len > best_len,
                None => true,
            };
            if pick {
                best = Some((consumed, expr, len));
            }
        }
    }
    best.map(|(consumed, expr, _)| (consumed, expr))
        .ok_or(DecodeError::InvalidPayload)
}

/// 表达式 payload 的 opcode 扫描报告（用于定位“未覆盖 opcode / 回退原因”）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpcodeScanReport {
    /// 采用的起始 word 偏移（0..=2）。
    pub start_words: usize,
    /// payload 声明的 word 数（不含最前面的 len word）。
    pub declared_words: usize,
    /// 实际消费的字节数（从 `input` 起始算起）。
    pub consumed_bytes: usize,
    /// 解析后缀序列时遇到的 opcode 频次（仅统计 opcode，不统计数值常量/字符串内容等）。
    pub opcode_counts: BTreeMap<i32, u64>,
    /// 无法识别为 value opcode 或 operator opcode 的 opcode 列表（去重、升序）。
    pub unknown_opcodes: Vec<i32>,
}

/// 扫描表达式 payload 中的 opcode 分布。
///
/// - 会尝试 `start_words=0..=2`，并选择“未知 opcode 更少、有效长度更长”的解析起点。
/// - 本函数不会因为遇到未知 opcode 而失败（未知 opcode 会进入 `unknown_opcodes`），
///   仅在 payload 长度不合法/越界时返回错误。
pub fn scan_expression_payload_opcodes(input: &[u8]) -> Result<OpcodeScanReport, DecodeError> {
    let mut best: Option<OpcodeScanReport> = None;

    for start_words in 0..=2 {
        let Ok((consumed_bytes, words, start_idx, declared_words)) = read_words_from_start(input, start_words) else {
            continue;
        };

        let sliced = if words.len() > start_idx {
            &words[start_idx..]
        } else {
            &[]
        };
        let (opcode_counts, unknown_opcodes) = scan_words_for_opcodes(sliced)?;

        let report = OpcodeScanReport {
            start_words,
            declared_words,
            consumed_bytes,
            opcode_counts,
            unknown_opcodes,
        };

        let pick = match &best {
            None => true,
            Some(prev) => {
                // 先比未知 opcode 数（少者优），再比声明长度（大者优）
                let a = report.unknown_opcodes.len();
                let b = prev.unknown_opcodes.len();
                a < b || (a == b && report.declared_words > prev.declared_words)
            }
        };

        if pick {
            best = Some(report);
        }
    }

    best.ok_or(DecodeError::InvalidPayload)
}

fn decode_from_start(
    input: &[u8],
    start_words: usize,
) -> Result<(usize, String, usize), DecodeError> {
    let start = start_words * 4;
    if input.len() < start + 4 {
        return Err(DecodeError::UnexpectedEof);
    }
    let len = read_i32(&input[start..start + 4]);
    if len < 0 {
        return Err(DecodeError::InvalidLength);
    }
    let len = len as usize;
    let total_words = len + 1;
    let total_bytes = total_words
        .checked_mul(4)
        .ok_or(DecodeError::InvalidLength)?;
    if input.len() < start + total_bytes {
        return Err(DecodeError::UnexpectedEof);
    }
    if len == 0 {
        return Ok((start + total_bytes, String::new(), len));
    }
    let mut words = Vec::with_capacity(len);
    for idx in 0..len {
        let offset = start + 4 + idx * 4;
        words.push(read_i32(&input[offset..offset + 4]));
    }
    if words.len() <= 1 {
        return Ok((start + total_bytes, String::new(), 0));
    }
    let mut start_idx = 1;
    if words.len() >= 3
        && words[0] == len as i32
        && words[1] == 1
        && matches!(
            words[2],
            // 允许更多 value opcode 作为“payload header”标识，避免误把 header 当表达式内容。
            0x65 | 0x66 | 0x67 | 0x68 | 0x69 | 0x6A
                | 0x6B | 0x6C | 0x6D
                | 0x6F | 0x70 | 0x71 | 0x72
                | 0x74 | 0x75 | 0x76
        )
    {
        start_idx = 2;
    }
    if words.len() <= start_idx {
        return Ok((start + total_bytes, String::new(), 0));
    }
    let expr = decode_words(&words[start_idx..])?;
    Ok((start + total_bytes, expr, len.saturating_sub(start_idx)))
}

fn read_words_from_start(
    input: &[u8],
    start_words: usize,
) -> Result<(usize, Vec<i32>, usize, usize), DecodeError> {
    let start = start_words * 4;
    if input.len() < start + 4 {
        return Err(DecodeError::UnexpectedEof);
    }
    let len = read_i32(&input[start..start + 4]);
    if len < 0 {
        return Err(DecodeError::InvalidLength);
    }
    let len = len as usize;
    let total_words = len + 1;
    let total_bytes = total_words
        .checked_mul(4)
        .ok_or(DecodeError::InvalidLength)?;
    if input.len() < start + total_bytes {
        return Err(DecodeError::UnexpectedEof);
    }
    if len == 0 {
        return Ok((start + total_bytes, Vec::new(), 0, 0));
    }
    let mut words = Vec::with_capacity(len);
    for idx in 0..len {
        let offset = start + 4 + idx * 4;
        words.push(read_i32(&input[offset..offset + 4]));
    }

    // 与 decode_from_start 保持一致：识别常见 header，并切掉。
    let mut start_idx = 1;
    if words.len() >= 3
        && words[0] == len as i32
        && words[1] == 1
        && matches!(
            words[2],
            0x65 | 0x66 | 0x67 | 0x68 | 0x69 | 0x6A
                | 0x6B | 0x6C | 0x6D
                | 0x6F | 0x70 | 0x71 | 0x72
                | 0x74 | 0x75 | 0x76
        )
    {
        start_idx = 2;
    }

    Ok((start + total_bytes, words, start_idx, len.saturating_sub(start_idx)))
}

fn decode_words(words: &[i32]) -> Result<String, DecodeError> {
    let mut stack: Vec<ExprNode> = Vec::new();
    let mut idx = 0;
    while idx < words.len() {
        let opcode = words[idx];
        if let Some(node) = parse_value_opcode(opcode, words, &mut idx, &mut stack)? {
            stack.push(node);
            continue;
        }
        if let Some(node) = apply_operator(opcode, &mut stack)? {
            stack.push(node);
            idx += 1;
            continue;
        }
        return Err(DecodeError::UnknownOpcode(opcode));
    }
    if stack.len() == 1 {
        Ok(stack.pop().unwrap_or_else(|| ExprNode::new(String::new(), PREC_VALUE)).text)
    } else {
        Err(DecodeError::InvalidPayload)
    }
}

fn scan_words_for_opcodes(
    words: &[i32],
) -> Result<(BTreeMap<i32, u64>, Vec<i32>), DecodeError> {
    let mut counts: BTreeMap<i32, u64> = BTreeMap::new();
    let mut unknown: BTreeMap<i32, u64> = BTreeMap::new();

    let mut idx = 0;
    while idx < words.len() {
        let opcode = words[idx];

        // value opcode：需要按长度跳过，避免把常量当作 opcode。
        if let Some(next) = skip_value_opcode(opcode, words, idx)? {
            *counts.entry(opcode).or_default() += 1;
            idx = next;
            continue;
        }

        // operator opcode：固定 1 word。
        if is_operator_opcode(opcode) {
            *counts.entry(opcode).or_default() += 1;
            idx += 1;
            continue;
        }

        // 未知：按 1 word 前进（无法确定其是否为变长块）。
        *counts.entry(opcode).or_default() += 1;
        *unknown.entry(opcode).or_default() += 1;
        idx += 1;
    }

    Ok((
        counts,
        unknown.keys().copied().collect::<Vec<_>>(),
    ))
}

fn skip_value_opcode(
    opcode: i32,
    words: &[i32],
    idx: usize,
) -> Result<Option<usize>, DecodeError> {
    let next = match opcode {
        0x65 => {
            let count = read_next_word(words, idx + 1)?;
            if count <= 1 {
                return Err(DecodeError::InvalidLength);
            }
            let count = count as usize;
            let data_count = count.saturating_sub(1);
            let data_start = idx + 2;
            let data_end = data_start
                .checked_add(data_count)
                .ok_or(DecodeError::InvalidLength)?;
            if data_end > words.len() {
                return Err(DecodeError::UnexpectedEof);
            }
            data_end
        }
        0x66 | 0x76 | 0x68 | 0x69 => {
            let len = read_next_word(words, idx + 1)?;
            if len < 0 {
                return Err(DecodeError::InvalidLength);
            }
            let len = len as usize;
            let data_start = idx + 2;
            let data_end = data_start
                .checked_add(len)
                .ok_or(DecodeError::InvalidLength)?;
            if data_end > words.len() {
                return Err(DecodeError::UnexpectedEof);
            }
            data_end
        }
        0x67 => {
            read_next_word(words, idx + 1)?;
            idx + 2
        }
        0x6A => {
            let data_start = idx + 1;
            let data_end = data_start + 5;
            if data_end > words.len() {
                return Err(DecodeError::UnexpectedEof);
            }
            let mut cursor = data_end;
            if cursor < words.len() && words[cursor] == 2100 {
                cursor += 1;
                if cursor >= words.len() {
                    return Err(DecodeError::UnexpectedEof);
                }
                let argc = words[cursor];
                if argc < 0 {
                    return Err(DecodeError::InvalidLength);
                }
                cursor += 1;
                let argc = argc as usize;
                if cursor + argc > words.len() {
                    return Err(DecodeError::UnexpectedEof);
                }
                cursor += argc;
            }
            if cursor < words.len() && words[cursor] == 2200 {
                if cursor + 3 > words.len() {
                    return Err(DecodeError::UnexpectedEof);
                }
                cursor += 3;
            }
            if cursor < words.len() {
                match words[cursor] {
                    1601 => cursor += 1,
                    1602 => {
                        cursor += 1;
                        cursor = skip_len_block(words, cursor)?;
                    }
                    _ => {}
                }
            }
            if cursor < words.len() {
                match words[cursor] {
                    1703 => {
                        if cursor + 3 > words.len() {
                            return Err(DecodeError::UnexpectedEof);
                        }
                        cursor += 3;
                    }
                    1702 => {
                        cursor += 1;
                        cursor = skip_len_block(words, cursor)?;
                    }
                    1701 => cursor += 1,
                    _ => {}
                }
            }
            cursor
        }
        0x6F => idx + 1,
        0x72 | 0x74 | 0x75 => {
            read_next_word(words, idx + 1)?;
            idx + 2
        }
        _ => return Ok(None),
    };

    Ok(Some(next))
}

fn parse_value_opcode(
    opcode: i32,
    words: &[i32],
    idx: &mut usize,
    stack: &mut Vec<ExprNode>,
) -> Result<Option<ExprNode>, DecodeError> {
    match opcode {
        0x65 => {
            let count = read_next_word(words, *idx + 1)?;
            if count <= 1 {
                return Err(DecodeError::InvalidLength);
            }
            let count = count as usize;
            let data_count = count.saturating_sub(1);
            let data_start = *idx + 2;
            let data_end = data_start
                .checked_add(data_count)
                .ok_or(DecodeError::InvalidLength)?;
            if data_end > words.len() {
                return Err(DecodeError::UnexpectedEof);
            }
            if data_count < 3 {
                let value = words[data_start];
                *idx = data_end;
                return Ok(Some(ExprNode::new(value.to_string(), PREC_VALUE)));
            }
            let v12 = words[data_start];
            let v13 = words[data_start + 1];
            let v14 = words[data_start + 2];
            let exp = words.get(data_start + 3).copied().unwrap_or(0);
            let mut unit = words.get(data_start + 4).copied().unwrap_or(0);
            if unit == 6 || unit == 7 {
                unit = 0;
            }
            let _unit = unit;
            let base = decode_value_expr_base(v12, v13, v14);
            let mut value = format_value_f6(base);
            if exp != 0 {
                value.push_str(" EX ");
                value.push_str(&exp.to_string());
            }
            *idx = data_end;
            Ok(Some(ExprNode::new(value.to_string(), PREC_VALUE)))
        }
        0x66 | 0x76 => {
            let len = read_next_word(words, *idx + 1)?;
            if len < 0 {
                return Err(DecodeError::InvalidLength);
            }
            let len = len as usize;
            let data_start = *idx + 2;
            let data_end = data_start
                .checked_add(len)
                .ok_or(DecodeError::InvalidLength)?;
            if data_end > words.len() {
                return Err(DecodeError::UnexpectedEof);
            }
            let text = decode_word_string(&words[data_start..data_end]);
            let escaped = text.replace('\'', "''");
            *idx = data_end;
            Ok(Some(ExprNode::new(format!("'{}'", escaped), PREC_VALUE)))
        }
        0x67 => {
            let v = read_next_word(words, *idx + 1)?;
            let text = match v {
                201 => "true",
                202 => "false",
                _ => "false",
            };
            *idx += 2;
            Ok(Some(ExprNode::new(text.to_string(), PREC_VALUE)))
        }
        0x68 => {
            let len = read_next_word(words, *idx + 1)?;
            if len < 0 {
                return Err(DecodeError::InvalidLength);
            }
            let len = len as usize;
            let data_start = *idx + 2;
            let data_end = data_start
                .checked_add(len)
                .ok_or(DecodeError::InvalidLength)?;
            if data_end > words.len() {
                return Err(DecodeError::UnexpectedEof);
            }
            let text = decode_word_string(&words[data_start..data_end]);
            *idx = data_end;
            Ok(Some(ExprNode::new(text, PREC_VALUE)))
        }
        0x69 => {
            let len = read_next_word(words, *idx + 1)?;
            if len <= 0 {
                return Err(DecodeError::InvalidLength);
            }
            let len = len as usize;
            let data_start = *idx + 2;
            let data_end = data_start
                .checked_add(len)
                .ok_or(DecodeError::InvalidLength)?;
            if data_end > words.len() {
                return Err(DecodeError::UnexpectedEof);
            }
            let mut parts = Vec::with_capacity(len);
            for v in &words[data_start..data_end] {
                parts.push(format!("0x{:08X}", *v as u32));
            }
            let text = format!("ID {}", parts.join(" "));
            *idx = data_end;
            Ok(Some(ExprNode::new(text, PREC_VALUE)))
        }
        0x6A => {
            let data_start = *idx + 1;
            let data_end = data_start + 5;
            if data_end > words.len() {
                return Err(DecodeError::UnexpectedEof);
            }
            let hash = words[data_start + 1];
            let low = words[data_start + 2];
            let high = words[data_start + 3];
            let suffix_value = words[data_start + 4];
            let hash_val = hash.unsigned_abs() as u32;
            let mut name = db1_dehash(hash_val);
            if name.is_empty() {
                name = "unknown attribute".to_string();
            }
            let mut text = if hash_val <= 387_951_929 {
                format!("ATTRIB {}", name)
            } else {
                name.clone()
            };
            if suffix_value != 0 {
                let suffix_name = db1_dehash(suffix_value.unsigned_abs());
                let suffix = if suffix_name.is_empty() {
                    if suffix_value > 0 && suffix_value < 531_442 {
                        suffix_value.to_string()
                    } else {
                        format!("0x{:08X}", suffix_value as u32)
                    }
                } else {
                    suffix_name
                };
                text.push(' ');
                text.push_str(&suffix);
            }
            if suffix_value == 0 {
                if low == -1 {
                    if let Some(idx_expr) = stack.pop() {
                        text.push('[');
                        text.push_str(&idx_expr.text);
                        text.push_str(" ]");
                    }
                } else if low != 0 {
                    if (low == 1 && high == 1) && name != "PARA" {
                        // 对多数非 PARA 属性，索引 1 为默认值，不显示
                    } else if high == low || high == 0 {
                        text.push('[');
                        text.push_str(&low.to_string());
                        text.push_str(" ]");
                    } else {
                        text.push(' ');
                        text.push_str(&low.to_string());
                        text.push_str(" TO ");
                        text.push_str(&high.to_string());
                    }
                }
            }
            let mut cursor = data_end;
            if cursor < words.len() && words[cursor] == 2100 {
                cursor += 1;
                if cursor >= words.len() {
                    return Err(DecodeError::UnexpectedEof);
                }
                let argc = words[cursor];
                if argc < 0 {
                    return Err(DecodeError::InvalidLength);
                }
                cursor += 1;
                for _ in 0..argc {
                    if stack.pop().is_none() {
                        return Err(DecodeError::StackUnderflow);
                    }
                    if cursor >= words.len() {
                        return Err(DecodeError::UnexpectedEof);
                    }
                    cursor += 1;
                }
            }
            if cursor < words.len() && words[cursor] == 2200 {
                if cursor + 3 > words.len() {
                    return Err(DecodeError::UnexpectedEof);
                }
                cursor += 3;
            }
            if cursor < words.len() {
                match words[cursor] {
                    1601 => cursor += 1,
                    1602 => {
                        cursor += 1;
                        cursor = skip_len_block(words, cursor)?;
                    }
                    _ => {}
                }
            }
            if cursor < words.len() {
                match words[cursor] {
                    1703 => {
                        if cursor + 3 > words.len() {
                            return Err(DecodeError::UnexpectedEof);
                        }
                        cursor += 3;
                    }
                    1702 => {
                        cursor += 1;
                        cursor = skip_len_block(words, cursor)?;
                    }
                    1701 => cursor += 1,
                    _ => {}
                }
            }
            *idx = cursor;
            Ok(Some(ExprNode::new(text, PREC_VALUE)))
        }
        0x6F => {
            *idx += 1;
            Ok(Some(ExprNode::new("PI".to_string(), PREC_VALUE)))
        }
        0x72 | 0x74 | 0x75 => {
            let value = read_next_word(words, *idx + 1)?;
            let name = db1_dehash(value.unsigned_abs());
            let text = if name.is_empty() {
                format!("0x{:08X}", value as u32)
            } else {
                name
            };
            *idx += 2;
            Ok(Some(ExprNode::new(text, PREC_VALUE)))
        }
        _ => Ok(None),
    }
}

fn apply_operator(opcode: i32, stack: &mut Vec<ExprNode>) -> Result<Option<ExprNode>, DecodeError> {
    let node = match opcode {
        301 => {
            let a = pop_one(stack)?;
            let inner = wrap_if(&a, a.prec < PREC_NOT);
            ExprNode::new(format!("NOT {}", inner), PREC_NOT)
        }
        302 => make_binary(stack, "AND", PREC_AND, false)?,
        303 => make_binary(stack, "OR", PREC_OR, false)?,
        401 => make_binary(stack, "EQ", PREC_COMPARE, false)?,
        501 => make_binary(stack, "NEQ", PREC_COMPARE, false)?,
        601 => make_binary(stack, "GT", PREC_COMPARE, false)?,
        // 0x25A(602) 在部分 DB 中也可见，语义等同 GT。
        602 => make_binary(stack, "GT", PREC_COMPARE, false)?,
        603 => make_binary(stack, "LT", PREC_COMPARE, false)?,
        605 => make_binary(stack, "GE", PREC_COMPARE, false)?,
        607 => make_binary(stack, "LE", PREC_COMPARE, false)?,
        801 => {
            let a = pop_one(stack)?;
            let inner = wrap_if(&a, a.prec <= PREC_NEG);
            ExprNode::new(format!("- {}", inner), PREC_NEG)
        }
        802 => make_binary(stack, "+", PREC_ADD, false)?,
        803 => make_binary(stack, "-", PREC_ADD, true)?,
        804 => make_binary(stack, "*", PREC_MUL, false)?,
        805 => make_binary(stack, "/", PREC_MUL, true)?,
        901 => make_function(stack, "SIN", 1)?,
        902 => make_function(stack, "COS", 1)?,
        903 => make_function(stack, "TAN", 1)?,
        904 => make_function(stack, "ASIN", 1)?,
        905 => make_function(stack, "ACOS", 1)?,
        906 => make_function(stack, "ATAN", 1)?,
        907 => make_function(stack, "ATAN2", 2)?,
        1001 => make_function(stack, "SQRT", 1)?,
        1002 => make_function(stack, "POW", 2)?,
        1003 => make_function(stack, "LOG", 1)?,
        1004 => make_function(stack, "ALOG", 1)?,
        1005 => make_function(stack, "INT", 1)?,
        1006 => make_function(stack, "NINT", 1)?,
        1007 => make_function(stack, "ABS", 1)?,
        1008 => make_function(stack, "MAX", 2)?,
        1009 => make_function(stack, "MIN", 2)?,
        1010 => make_function(stack, "NOMBORE", 1)?,
        1011 => make_function(stack, "DISTCONVERT", 1)?,
        1012 => make_function(stack, "REAL", 1)?,
        1101 => make_function(stack, "COMP", 2)?,
        1102 | 1103 => make_function(stack, "ARRAY", 2)?,
        1201..=1220 => {
            let name = format!("VAR{}", opcode);
            if stack.is_empty() {
                ExprNode::new(name, PREC_VALUE)
            } else {
                make_function(stack, &name, 1)?
            }
        }
        1301 => make_function(stack, "LENGTH", 1)?,
        1302 => make_function(stack, "REAL", 1)?,
        1303 => make_function(stack, "MATCH", 2)?,
        1304 => make_function(stack, "AFTER", 2)?,
        1305 => make_function(stack, "BEFORE", 2)?,
        1306 => make_function(stack, "STRING", 2)?,
        1307 => make_function(stack, "UPCASE", 1)?,
        1308 => make_function(stack, "LOWCASE", 1)?,
        1309 => make_function(stack, "SUBSTRING", 3)?,
        1311 => make_function(stack, "DEFINED", 1)?,
        1312 => make_function(stack, "UNDEFINED", 1)?,
        1313 => make_function(stack, "SIZE", 1)?,
        1314 => make_function(stack, "TRIM", 1)?,
        1315 => make_function(stack, "MATCHWILD", 2)?,
        1316 => make_function(stack, "WIDTH", 1)?,
        1317 => make_function(stack, "PART", 2)?,
        1321 => make_function(stack, "OCCURS", 2)?,
        1322 => make_function(stack, "REPLACE", 3)?,
        1369 => make_function(stack, "VTEXT", 2)?,
        1370 => make_function(stack, "VVALUE", 2)?,
        1401 => make_function(stack, "REAL", 1)?,
        1410 => make_function(stack, "STR", 1)?,
        1822 => make_function(stack, "IFTRUE", 3)?,
        1824 => make_function(stack, "DISTCONVERT", 1)?,
        1825 => make_function(stack, "SET", 2)?,
        1826 => make_function(stack, "UNSET", 1)?,
        1827 => make_function(stack, "ARRAY", 2)?,
        1828 => make_function(stack, "EMPTY", 1)?,
        1829 => make_function(stack, "SPLIT", 2)?,
        _ => return Ok(None),
    };
    Ok(Some(node))
}

fn make_binary(
    stack: &mut Vec<ExprNode>,
    op: &str,
    prec: i32,
    right_tight: bool,
) -> Result<ExprNode, DecodeError> {
    let right = pop_one(stack)?;
    let left = pop_one(stack)?;
    let left_text = wrap_if(&left, left.prec < prec);
    let mut right_paren = right.prec < prec;
    if right_tight && right.prec == prec {
        right_paren = true;
    }
    let right_text = wrap_if(&right, right_paren);
    Ok(ExprNode::new(
        format!("{} {} {}", left_text, op, right_text),
        prec,
    ))
}

fn make_function(
    stack: &mut Vec<ExprNode>,
    name: &str,
    argc: usize,
) -> Result<ExprNode, DecodeError> {
    if stack.len() < argc {
        return Err(DecodeError::StackUnderflow);
    }
    let mut args = Vec::with_capacity(argc);
    for _ in 0..argc {
        args.push(stack.pop().ok_or(DecodeError::StackUnderflow)?);
    }
    args.reverse();
    let mut text = String::new();
    text.push_str(name);
    text.push_str(" ( ");
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            text.push_str(" , ");
        }
        text.push_str(&arg.text);
    }
    text.push_str(" )");
    Ok(ExprNode::new(text, PREC_FUNC))
}

fn pop_one(stack: &mut Vec<ExprNode>) -> Result<ExprNode, DecodeError> {
    stack.pop().ok_or(DecodeError::StackUnderflow)
}

fn wrap_if(node: &ExprNode, need_paren: bool) -> String {
    if need_paren {
        format!("({})", node.text)
    } else {
        node.text.clone()
    }
}

fn read_i32(bytes: &[u8]) -> i32 {
    i32::from_be_bytes(bytes[..4].try_into().unwrap_or([0; 4]))
}

fn read_next_word(words: &[i32], idx: usize) -> Result<i32, DecodeError> {
    words.get(idx).copied().ok_or(DecodeError::UnexpectedEof)
}

fn skip_len_block(words: &[i32], cursor: usize) -> Result<usize, DecodeError> {
    if cursor >= words.len() {
        return Err(DecodeError::UnexpectedEof);
    }
    let len = words[cursor];
    if len <= 0 {
        return Err(DecodeError::InvalidLength);
    }
    let len = len as usize;
    if cursor + len > words.len() {
        return Err(DecodeError::UnexpectedEof);
    }
    Ok(cursor + len)
}

fn decode_word_string(words: &[i32]) -> String {
    let mut bytes = Vec::with_capacity(words.len());
    for w in words {
        bytes.push((*w as u32 & 0xFF) as u8);
    }
    String::from_utf8_lossy(&bytes).to_string()
}

fn decode_value_expr_base(a2: i32, a3: i32, a4: i32) -> f64 {
    let a2u = a2 as u32;
    let a3u = a3 as u32;
    let a4u = a4 as u32;
    if (a4u & 0xC0000000) != 0x40000000 {
        let v = (a2 as f64) * 0.000030517578125 + (a3 as f64) * 9.313225746154785e-10;
        return v * 2_f64.powi(a4);
    }
    let mut hi = (a2u & 0x1FFFFF) | ((a4u & 0x7FF) << 20);
    if (a2u & 0x40000000) != 0 {
        hi |= 0x80000000;
    }
    let bits = ((hi as u64) << 32) | (a3u as u64);
    f64::from_bits(bits)
}

fn format_value_f6(value: f64) -> String {
    let mut text = format!("{:.6}", value);
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" {
        text = "0".to_string();
    }
    text
}

fn is_operator_opcode(value: i32) -> bool {
    // 避免在这里再维护一份“operator 白名单”，直接复用 opcode 分类。
    // 仅用于 opcode 扫描：value opcode 已在 skip_value_opcode 中处理。
    !matches!(
        OpcodeCategory::from(value),
        OpcodeCategory::Unknown | OpcodeCategory::Values
    )
}

#[cfg(test)]
mod tests {
    use super::decode_expression_payload;
    use super::scan_expression_payload_opcodes;

    fn be_i32(v: i32) -> [u8; 4] {
        v.to_be_bytes()
    }

    #[test]
    fn test_decode_expression_payload_attr_index_1() {
        let input: [u8; 76] = [
            0x1C, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00, 0x11, 0x00,
            0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x65, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00,
            0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x6A, 0x00, 0x00, 0x00, 0x02,
            0x00, 0x0D, 0x20, 0xC7, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x41, 0x00, 0x00, 0x06, 0xA5,
        ];
        let (consumed, value) = decode_expression_payload(&input).unwrap();
        assert_eq!("ATTRIB DESP[1 ]", value);
        assert_eq!(consumed, input.len());
    }

    #[test]
    fn test_decode_expression_payload_attr_index_2() {
        let input: [u8; 76] = [
            0x1C, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00, 0x11, 0x00,
            0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x65, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00,
            0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x6A, 0x00, 0x00, 0x00, 0x02,
            0x00, 0x0D, 0x20, 0xC7, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x41, 0x00, 0x00, 0x06, 0xA5,
        ];
        let (consumed, value) = decode_expression_payload(&input).unwrap();
        assert_eq!("ATTRIB DESP[2 ]", value);
        assert_eq!(consumed, input.len());
    }

    #[test]
    fn test_decode_expression_payload_gt_alt_opcode_602() {
        // len=8, words=[dummy, 1, 2, GT(602)] in postfix form:
        // 1 -> 0x65, 2, 1
        // 2 -> 0x65, 2, 2
        let words: [i32; 8] = [0, 0x65, 2, 1, 0x65, 2, 2, 602];
        let mut input = Vec::with_capacity((words.len() + 1) * 4);
        input.extend_from_slice(&be_i32(words.len() as i32));
        for w in words {
            input.extend_from_slice(&be_i32(w));
        }

        let (consumed, value) = decode_expression_payload(&input).unwrap();
        assert_eq!(consumed, input.len());
        assert_eq!(value, "1 GT 2");
    }

    #[test]
    fn test_decode_expression_payload_header_text_alt_0x76() {
        // payload 内部带 header: [len, 1, 0x76, ...]，应识别并跳过 header。
        // 0x76 的语义等同 0x66（文本）。
        let words: [i32; 5] = [5, 1, 0x76, 1, 0x41]; // 'A'
        let mut input = Vec::with_capacity((words.len() + 1) * 4);
        input.extend_from_slice(&be_i32(words.len() as i32));
        for w in words {
            input.extend_from_slice(&be_i32(w));
        }

        let (consumed, value) = decode_expression_payload(&input).unwrap();
        assert_eq!(consumed, input.len());
        assert_eq!(value, "'A'");
    }

    #[test]
    fn test_scan_expression_payload_opcodes_counts_only_opcodes() {
        // 与 test_decode_expression_payload_gt_alt_opcode_602 相同的 payload，验证扫描只统计 opcode。
        let words: [i32; 8] = [0, 0x65, 2, 1, 0x65, 2, 2, 602];
        let mut input = Vec::with_capacity((words.len() + 1) * 4);
        input.extend_from_slice(&be_i32(words.len() as i32));
        for w in words {
            input.extend_from_slice(&be_i32(w));
        }

        let report = scan_expression_payload_opcodes(&input).unwrap();
        assert_eq!(report.opcode_counts.get(&0x65).copied().unwrap_or(0), 2);
        assert_eq!(report.opcode_counts.get(&602).copied().unwrap_or(0), 1);
        // 常量 1/2 不应被当作 opcode 统计
        assert!(report.opcode_counts.get(&1).is_none());
        assert!(report.opcode_counts.get(&2).is_none());
        assert!(report.unknown_opcodes.is_empty());
    }

    #[test]
    fn test_scan_expression_payload_opcodes_reports_unknown() {
        let words: [i32; 2] = [0, 9999];
        let mut input = Vec::with_capacity((words.len() + 1) * 4);
        input.extend_from_slice(&be_i32(words.len() as i32));
        for w in words {
            input.extend_from_slice(&be_i32(w));
        }

        let report = scan_expression_payload_opcodes(&input).unwrap();
        assert!(report.unknown_opcodes.contains(&9999));
    }
}
