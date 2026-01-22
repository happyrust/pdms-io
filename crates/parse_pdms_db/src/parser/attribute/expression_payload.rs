use aios_core::tool::db_tool::db1_dehash;

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
            0x65 | 0x66 | 0x67 | 0x68 | 0x69 | 0x6A | 0x6F | 0x72 | 0x74 | 0x75
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
    matches!(
        value,
        301 | 302 | 303
            | 401 | 501 | 601 | 603 | 605 | 607
            | 801 | 802 | 803 | 804 | 805
            | 901..=907
            | 1001..=1012
            | 1101 | 1102 | 1103
            | 1301..=1322
            | 1369 | 1370 | 1401 | 1410
            | 1822 | 1824 | 1825 | 1826 | 1827 | 1828 | 1829
    )
}

#[cfg(test)]
mod tests {
    use super::decode_expression_payload;

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
}
