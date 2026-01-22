//! 轴向数据解析器
//!
//! 提供 PDMS 轴向属性的解析功能，包括：
//! - 轴向表达式（X/Y/Z 正负方向）
//! - 轴向数值解析

use nom::number::complete::be_i32;
use nom::IResult;

/// 轴向类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
    Unknown,
}

impl Axis {
    /// 从索引创建轴向
    pub fn from_index(index: i32) -> Self {
        match index {
            1 => Axis::X,
            2 => Axis::Y,
            3 => Axis::Z,
            _ => Axis::Unknown,
        }
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
            Axis::Unknown => "UNKNOWN",
        }
    }
}

impl std::fmt::Display for Axis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 轴向表达式结果
#[derive(Debug, Clone)]
pub struct AxisExpression {
    /// 轴向
    pub axis: Axis,
    /// 是否为负方向
    pub negative: bool,
}

impl AxisExpression {
    /// 创建新的轴向表达式
    pub fn new(axis: Axis, negative: bool) -> Self {
        Self { axis, negative }
    }

    /// 转换为字符串表示
    pub fn to_string(&self) -> String {
        if self.negative {
            format!("-{}", self.axis)
        } else {
            self.axis.to_string()
        }
    }
}

/// 轴向表达式标识符
pub const AXIS_EXPR_IDENTIFIER: i32 = 0x1C000003u32 as i32;

/// 检查是否为轴向表达式
///
/// # 轴向表达式特征
/// - 数据长度至少 16 字节
/// - input[0..4]: 标识符必须为 0x1C000003
/// - input[12..16]: 轴索引必须在 1-3 范围内（1=X, 2=Y, 3=Z）
#[inline]
pub fn is_axis_expression(input: &[u8]) -> Result<bool, nom::Err<nom::error::Error<&[u8]>>> {
    if input.len() < 16 {
        return Ok(false);
    }

    let (_, identifier) = be_i32(input)?;
    if identifier != AXIS_EXPR_IDENTIFIER {
        return Ok(false);
    }

    let (_, axis_index) = be_i32(&input[12..16])?;
    Ok((1..=3).contains(&axis_index))
}

/// 解析轴向表达式
///
/// # 格式（16 字节）
/// - input[0..4]: 1C 00 00 03 => 标识轴向表达式
/// - input[4..8]: 00 00 00 02 => 表达式子类型
/// - input[8..12]: 正负标志（1=正，2=负）
/// - input[12..16]: 轴索引（1=X, 2=Y, 3=Z）
///
/// # 示例
/// ```ignore
/// let input = [0x1C, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02,
///              0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02];
/// let (_, expr) = parse_axis_expression(&input).unwrap();
/// assert_eq!(expr.axis, Axis::Y);
/// assert!(!expr.negative);
/// ```
pub fn parse_axis_expression(input: &[u8]) -> IResult<&[u8], AxisExpression> {
    let (input, _identifier) = be_i32(input)?;
    let (input, _expr_type) = be_i32(input)?;
    let (input, positive_flag) = be_i32(input)?;
    let (input, axis_index) = be_i32(input)?;

    let axis = Axis::from_index(axis_index);
    let negative = positive_flag == 2;

    Ok((input, AxisExpression::new(axis, negative)))
}

/// 解析轴向表达式并返回字符串结果
///
/// # 参数
/// - `input`: 输入字节
/// - `expression_type`: 表达式类型名称
///
/// # 返回
/// `(表达式类型, 轴向字符串)` 如 `("PAXI", "Y")` 或 `("PAXI", "-X")`
pub fn parse_axis_expression_str(
    input: &[u8],
    expression_type: String,
) -> IResult<&[u8], (String, String)> {
    let (input, expr) = parse_axis_expression(input)?;
    Ok((input, (expression_type, expr.to_string())))
}

/// 从轴向索引值解析轴向字符串
///
/// # 参数
/// - `axis_index`: 轴索引 (1=X, 2=Y, 3=Z)
/// - `positive_flag`: 正负标志 (1=正, 2=负)
#[inline]
pub fn axis_index_to_string(axis_index: i32, positive_flag: i32) -> String {
    let axis = Axis::from_index(axis_index);
    if positive_flag == 2 {
        format!("-{}", axis)
    } else {
        axis.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_axis_from_index() {
        assert_eq!(Axis::from_index(1), Axis::X);
        assert_eq!(Axis::from_index(2), Axis::Y);
        assert_eq!(Axis::from_index(3), Axis::Z);
        assert_eq!(Axis::from_index(0), Axis::Unknown);
        assert_eq!(Axis::from_index(4), Axis::Unknown);
    }

    #[test]
    fn test_axis_expression_to_string() {
        let expr = AxisExpression::new(Axis::X, false);
        assert_eq!(expr.to_string(), "X");

        let expr = AxisExpression::new(Axis::Y, true);
        assert_eq!(expr.to_string(), "-Y");
    }

    #[test]
    fn test_is_axis_expression() {
        // 有效的轴向表达式
        let valid = [
            0x1C, 0x00, 0x00, 0x03, // 标识符
            0x00, 0x00, 0x00, 0x02, // 子类型
            0x00, 0x00, 0x00, 0x01, // 正负标志
            0x00, 0x00, 0x00, 0x02, // 轴索引 (Y)
        ];
        assert!(is_axis_expression(&valid).unwrap());

        // 无效标识符
        let invalid_id = [
            0x1C, 0x00, 0x00, 0x04, // 错误的标识符
            0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02,
        ];
        assert!(!is_axis_expression(&invalid_id).unwrap());

        // 无效轴索引
        let invalid_axis = [
            0x1C, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x04, // 轴索引 4，无效
        ];
        assert!(!is_axis_expression(&invalid_axis).unwrap());

        // 数据太短
        let too_short = [0x1C, 0x00, 0x00, 0x03];
        assert!(!is_axis_expression(&too_short).unwrap());
    }

    #[test]
    fn test_parse_axis_expression() {
        let input = [
            0x1C, 0x00, 0x00, 0x03, // 标识符
            0x00, 0x00, 0x00, 0x02, // 子类型
            0x00, 0x00, 0x00, 0x01, // 正标志
            0x00, 0x00, 0x00, 0x02, // Y 轴
        ];
        let (rest, expr) = parse_axis_expression(&input).unwrap();
        assert!(rest.is_empty());
        assert_eq!(expr.axis, Axis::Y);
        assert!(!expr.negative);

        // 负方向
        let input_neg = [
            0x1C, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, // 负标志
            0x00, 0x00, 0x00, 0x01, // X 轴
        ];
        let (_, expr) = parse_axis_expression(&input_neg).unwrap();
        assert_eq!(expr.axis, Axis::X);
        assert!(expr.negative);
        assert_eq!(expr.to_string(), "-X");
    }

    #[test]
    fn test_axis_index_to_string() {
        assert_eq!(axis_index_to_string(1, 1), "X");
        assert_eq!(axis_index_to_string(2, 1), "Y");
        assert_eq!(axis_index_to_string(3, 2), "-Z");
    }
}
