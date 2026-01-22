//! 操作码定义与分发模块
//!
//! 基于 IDA Pro 对 core.dll `DBE_Builder::getExpressionTree` 的逆向分析。
//! 操作码按 `opcode / 100` 分类路由到不同处理器。

/// 操作码类别
///
/// 根据 core.dll 的分发机制，操作码按百位数分类：
/// - 1xx: 值类型
/// - 3xx: 布尔运算
/// - 4xx: 等值比较
/// - 5xx: 非等比较
/// - 6xx: 比较运算
/// - 7xx: 集合运算
/// - 8xx: 算术运算
/// - 9xx: 三角函数
/// - 10xx: 实数函数
/// - 11xx: Dope 函数
/// - 12xx: 变量
/// - 13xx: 字符串函数
/// - 14xx: 转换函数
/// - 15xx: 顺序函数
/// - 18xx: 通用函数
/// - 19xx: 历史函数
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpcodeCategory {
    /// 值类型 (101-199, ASCII 字符)
    Values = 1,
    /// 布尔运算 (301-399)
    Boolean = 3,
    /// 等值比较 (401-499)
    Equality = 4,
    /// 非等比较 (501-599)
    NonEquality = 5,
    /// 比较运算 (601-699)
    Comparison = 6,
    /// 集合运算 (701-799)
    InSet = 7,
    /// 算术运算 (801-805)
    Arithmetic = 8,
    /// 三角函数 (901-907)
    Trigonometric = 9,
    /// 实数函数 (1001-1012)
    RealFunctions = 10,
    /// Dope 函数 (1101-1199)
    DopeFunctions = 11,
    /// 变量 (1201-1299)
    Variables = 12,
    /// 字符串函数 (1301-1399)
    StringFunctions = 13,
    /// 转换函数 (1401-1499)
    ConversionFunctions = 14,
    /// 顺序函数 (1501-1599)
    OrderFunctions = 15,
    /// 通用函数 (1801-1899)
    GeneralFunctions = 18,
    /// 历史函数 (1901-1999)
    HistoryFunctions = 19,
    /// 未知类别
    Unknown = 0,
}

impl From<i32> for OpcodeCategory {
    fn from(opcode: i32) -> Self {
        match opcode / 100 {
            1 => OpcodeCategory::Values,
            3 => OpcodeCategory::Boolean,
            4 => OpcodeCategory::Equality,
            5 => OpcodeCategory::NonEquality,
            6 => OpcodeCategory::Comparison,
            7 => OpcodeCategory::InSet,
            8 => OpcodeCategory::Arithmetic,
            9 => OpcodeCategory::Trigonometric,
            10 => OpcodeCategory::RealFunctions,
            11 => OpcodeCategory::DopeFunctions,
            12 => OpcodeCategory::Variables,
            13 => OpcodeCategory::StringFunctions,
            14 => OpcodeCategory::ConversionFunctions,
            15 => OpcodeCategory::OrderFunctions,
            18 => OpcodeCategory::GeneralFunctions,
            19 => OpcodeCategory::HistoryFunctions,
            _ => OpcodeCategory::Unknown,
        }
    }
}

// ============================================================================
// 值类型操作码 (case 1, ASCII 字符)
// ============================================================================

/// 值类型操作码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueOpcode {
    /// 'e' (0x65 = 101) - 数值表达式
    ValueExpression = 0x65,
    /// 'f' (0x66 = 102) - 文本
    Text = 0x66,
    /// 'g' (0x67 = 103) - 布尔值 (后跟 201=true, 202=false)
    BooleanValue = 0x67,
    /// 'h' (0x68 = 104) - 变量名
    VarName = 0x68,
    /// 'i' (0x69 = 105) - ID 表达式
    IdExpression = 0x69,
    /// 'j' (0x6A = 106) - 属性引用
    Attribute = 0x6A,
    /// 'k' (0x6B = 107) - 位置表达式
    Position = 0x6B,
    /// 'l' (0x6C = 108) - 方向表达式
    Direction = 0x6C,
    /// 'm' (0x6D = 109) - 方位表达式
    Orientation = 0x6D,
    /// 'o' (0x6F = 111) - PI 常量
    Pi = 0x6F,
    /// 'p' (0x70 = 112) - Ppoint
    Ppoint = 0x70,
    /// 'q' (0x71 = 113) - Pline
    Pline = 0x71,
    /// 'r' (0x72 = 114) - Word
    Word = 0x72,
    /// 't' (0x74 = 116) - 属性名
    AttName = 0x74,
    /// 'u' (0x75 = 117) - 名词名
    NounName = 0x75,
    /// 'v' (0x76 = 118) - 文本（同 'f'）
    TextAlt = 0x76,
}

impl TryFrom<i32> for ValueOpcode {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0x65 => Ok(ValueOpcode::ValueExpression),
            0x66 => Ok(ValueOpcode::Text),
            0x67 => Ok(ValueOpcode::BooleanValue),
            0x68 => Ok(ValueOpcode::VarName),
            0x69 => Ok(ValueOpcode::IdExpression),
            0x6A => Ok(ValueOpcode::Attribute),
            0x6B => Ok(ValueOpcode::Position),
            0x6C => Ok(ValueOpcode::Direction),
            0x6D => Ok(ValueOpcode::Orientation),
            0x6F => Ok(ValueOpcode::Pi),
            0x70 => Ok(ValueOpcode::Ppoint),
            0x71 => Ok(ValueOpcode::Pline),
            0x72 => Ok(ValueOpcode::Word),
            0x74 => Ok(ValueOpcode::AttName),
            0x75 => Ok(ValueOpcode::NounName),
            0x76 => Ok(ValueOpcode::TextAlt),
            _ => Err(()),
        }
    }
}

// ============================================================================
// 算术运算操作码 (case 8)
// ============================================================================

/// 算术运算操作码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithmeticOpcode {
    /// 801 (0x321) - 取负
    Negate = 801,
    /// 802 (0x322) - 加法
    Plus = 802,
    /// 803 (0x323) - 减法
    Sub = 803,
    /// 804 (0x324) - 乘法
    Mul = 804,
    /// 805 (0x325) - 除法
    Div = 805,
}

impl ArithmeticOpcode {
    /// 返回操作数个数
    pub fn operand_count(&self) -> usize {
        match self {
            ArithmeticOpcode::Negate => 1,
            _ => 2,
        }
    }

    /// 返回格式化模板
    pub fn format_template(&self) -> &'static str {
        match self {
            ArithmeticOpcode::Negate => "(-{})",
            ArithmeticOpcode::Plus => "({}+{})",
            ArithmeticOpcode::Sub => "({}-{})",
            ArithmeticOpcode::Mul => "({}*{})",
            ArithmeticOpcode::Div => "({}/{})",
        }
    }
}

impl TryFrom<i32> for ArithmeticOpcode {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            801 | 0x321 => Ok(ArithmeticOpcode::Negate),
            802 | 0x322 => Ok(ArithmeticOpcode::Plus),
            803 | 0x323 => Ok(ArithmeticOpcode::Sub),
            804 | 0x324 => Ok(ArithmeticOpcode::Mul),
            805 | 0x325 => Ok(ArithmeticOpcode::Div),
            _ => Err(()),
        }
    }
}

// ============================================================================
// 三角函数操作码 (case 9)
// ============================================================================

/// 三角函数操作码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrigonometricOpcode {
    /// 901 (0x385) - 正弦
    Sin = 901,
    /// 902 (0x386) - 余弦
    Cos = 902,
    /// 903 (0x387) - 正切
    Tan = 903,
    /// 904 (0x388) - 反正弦
    Asin = 904,
    /// 905 (0x389) - 反余弦
    Acos = 905,
    /// 906 (0x38A) - 反正切
    Atan = 906,
    /// 907 (0x38B) - 反正切2
    Atan2 = 907,
}

impl TrigonometricOpcode {
    /// 返回操作数个数
    pub fn operand_count(&self) -> usize {
        match self {
            TrigonometricOpcode::Atan2 => 2,
            _ => 1,
        }
    }

    /// 返回格式化模板
    pub fn format_template(&self) -> &'static str {
        match self {
            TrigonometricOpcode::Sin => "SIN({})",
            TrigonometricOpcode::Cos => "COS({})",
            TrigonometricOpcode::Tan => "TAN({})",
            TrigonometricOpcode::Asin => "ASIN({})",
            TrigonometricOpcode::Acos => "ACOS({})",
            TrigonometricOpcode::Atan => "ATAN({})",
            TrigonometricOpcode::Atan2 => "ATANT({},{})",
        }
    }
}

impl TryFrom<i32> for TrigonometricOpcode {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            901 | 0x385 => Ok(TrigonometricOpcode::Sin),
            902 | 0x386 => Ok(TrigonometricOpcode::Cos),
            903 | 0x387 => Ok(TrigonometricOpcode::Tan),
            904 | 0x388 => Ok(TrigonometricOpcode::Asin),
            905 | 0x389 => Ok(TrigonometricOpcode::Acos),
            906 | 0x38A => Ok(TrigonometricOpcode::Atan),
            907 | 0x38B => Ok(TrigonometricOpcode::Atan2),
            _ => Err(()),
        }
    }
}

// ============================================================================
// 实数函数操作码 (case 10)
// ============================================================================

/// 实数函数操作码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealFunctionOpcode {
    /// 1001 (0x3E9) - 平方根
    Sqrt = 1001,
    /// 1002 (0x3EA) - 幂
    Pow = 1002,
    /// 1003 (0x3EB) - 对数
    Log = 1003,
    /// 1004 (0x3EC) - 反对数
    Alog = 1004,
    /// 1005 (0x3ED) - 取整
    Int = 1005,
    /// 1006 (0x3EE) - 四舍五入取整
    Nint = 1006,
    /// 1007 (0x3EF) - 绝对值
    Abs = 1007,
    /// 1008 (0x3F0) - 最大值
    Max = 1008,
    /// 1009 (0x3F1) - 最小值
    Min = 1009,
}

impl RealFunctionOpcode {
    /// 返回操作数个数
    pub fn operand_count(&self) -> usize {
        match self {
            RealFunctionOpcode::Pow | RealFunctionOpcode::Max | RealFunctionOpcode::Min => 2,
            _ => 1,
        }
    }

    /// 返回格式化模板
    pub fn format_template(&self) -> &'static str {
        match self {
            RealFunctionOpcode::Sqrt => "SQRT({})",
            RealFunctionOpcode::Pow => "POW({},{})",
            RealFunctionOpcode::Log => "LOG({})",
            RealFunctionOpcode::Alog => "ALOG({})",
            RealFunctionOpcode::Int => "INT({})",
            RealFunctionOpcode::Nint => "NINT({})",
            RealFunctionOpcode::Abs => "ABS({})",
            RealFunctionOpcode::Max => "MAX({},{})",
            RealFunctionOpcode::Min => "MIN({},{})",
        }
    }
}

impl TryFrom<i32> for RealFunctionOpcode {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1001 | 0x3E9 => Ok(RealFunctionOpcode::Sqrt),
            1002 | 0x3EA => Ok(RealFunctionOpcode::Pow),
            1003 | 0x3EB => Ok(RealFunctionOpcode::Log),
            1004 | 0x3EC => Ok(RealFunctionOpcode::Alog),
            1005 | 0x3ED => Ok(RealFunctionOpcode::Int),
            1006 | 0x3EE => Ok(RealFunctionOpcode::Nint),
            1007 | 0x3EF => Ok(RealFunctionOpcode::Abs),
            1008 | 0x3F0 => Ok(RealFunctionOpcode::Max),
            1009 | 0x3F1 => Ok(RealFunctionOpcode::Min),
            _ => Err(()),
        }
    }
}

// ============================================================================
// 字符串函数操作码 (case 13)
// ============================================================================

/// 字符串函数操作码
/// 
/// 基于 IDA 分析的 core.dll 函数名表：
/// SINE COSINE TANGENT SQRT ASIN ACOS ATAN ATANT BOOLEAN POWER LOG ALOG ABS INT NINT 
/// LENGTH REAL MATCH MAX MIN AFTER BEFORE STRING UPCASE LOWCASE SUBSTRING 
/// DEFINED UNDEFINED SIZE DLENGTH DMATCH DSUBSTRING TRIM MATCHWILD WIDTH PART 
/// SET UNSET ARRAY EMPTY OCCURS REPLACE VTEXT VVALUE VLOGICAL SPLIT IFTRUE DISTCONVERT
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringFunctionOpcode {
    /// 1301 (0x515) - 字符串长度
    Length = 1301,
    /// 1302 (0x516) - 转换为实数
    Real = 1302,
    /// 1303 (0x517) - 匹配
    Match = 1303,
    /// 1304 (0x518) - 之后
    After = 1304,
    /// 1305 (0x519) - 之前
    Before = 1305,
    /// 1306 (0x51A) - 转换为字符串
    String = 1306,
    /// 1307 (0x51B) - 转大写
    Upcase = 1307,
    /// 1308 (0x51C) - 转小写
    Lowcase = 1308,
    /// 1309 (0x51D) - 子串
    Substring = 1309,
    /// 1311 (0x51F) - 是否定义
    Defined = 1311,
    /// 1312 (0x520) - 是否未定义
    Undefined = 1312,
    /// 1313 (0x521) - 大小
    Size = 1313,
    /// 1314 (0x522) - 去空格
    Trim = 1314,
    /// 1315 (0x523) - 通配符匹配
    Matchwild = 1315,
    /// 1316 (0x524) - 宽度
    Width = 1316,
    /// 1317 (0x525) - 部分
    Part = 1317,
    /// 1321 (0x529) - 出现次数
    Occurs = 1321,
    /// 1322 (0x52A) - 替换
    Replace = 1322,
    /// 1369 (0x559) - VTEXT
    Vtext = 1369,
    /// 1370 (0x55A) - VVALUE
    Vvalue = 1370,
    /// 1401 (0x579) - 转换为实数（另一个版本）
    RealAlt = 1401,
    /// 1410 (0x582) - 转换为字符串 STR
    Str = 1410,
}

impl StringFunctionOpcode {
    /// 返回操作数个数
    pub fn operand_count(&self) -> usize {
        match self {
            // 一元函数
            StringFunctionOpcode::Length
            | StringFunctionOpcode::Real
            | StringFunctionOpcode::RealAlt
            | StringFunctionOpcode::Str
            | StringFunctionOpcode::Upcase
            | StringFunctionOpcode::Lowcase
            | StringFunctionOpcode::Trim
            | StringFunctionOpcode::Defined
            | StringFunctionOpcode::Undefined
            | StringFunctionOpcode::Size
            | StringFunctionOpcode::Width => 1,
            // 二元函数
            StringFunctionOpcode::Match
            | StringFunctionOpcode::After
            | StringFunctionOpcode::Before
            | StringFunctionOpcode::Matchwild
            | StringFunctionOpcode::Part
            | StringFunctionOpcode::Occurs
            | StringFunctionOpcode::Vtext
            | StringFunctionOpcode::Vvalue
            | StringFunctionOpcode::String => 2,
            // 三元函数
            StringFunctionOpcode::Substring
            | StringFunctionOpcode::Replace => 3,
        }
    }

    /// 返回格式化模板
    pub fn format_template(&self) -> &'static str {
        match self {
            StringFunctionOpcode::Length => "LEN({})",
            StringFunctionOpcode::Real | StringFunctionOpcode::RealAlt => "REAL({})",
            StringFunctionOpcode::Match => "MAT({},'{}')",
            StringFunctionOpcode::After => "AFTER({},{})",
            StringFunctionOpcode::Before => "BEFORE({},{})",
            StringFunctionOpcode::String => "STRING({},{})",
            StringFunctionOpcode::Upcase => "UPCASE({})",
            StringFunctionOpcode::Lowcase => "LOWCASE({})",
            StringFunctionOpcode::Substring => "SUBSTRING({},{},{})",
            StringFunctionOpcode::Defined => "DEFINED({})",
            StringFunctionOpcode::Undefined => "UNDEFINED({})",
            StringFunctionOpcode::Size => "SIZE({})",
            StringFunctionOpcode::Trim => "TRIM({})",
            StringFunctionOpcode::Matchwild => "MATCHWILD({},{})",
            StringFunctionOpcode::Width => "WIDTH({})",
            StringFunctionOpcode::Part => "PART({},{})",
            StringFunctionOpcode::Occurs => "OCCURS({},{})",
            StringFunctionOpcode::Replace => "REPLACE({},{},{})",
            StringFunctionOpcode::Vtext => "VTEXT({},{})",
            StringFunctionOpcode::Vvalue => "VVALUE({},{})",
            StringFunctionOpcode::Str => "STR({})",
        }
    }
}

impl TryFrom<i32> for StringFunctionOpcode {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1301 | 0x515 => Ok(StringFunctionOpcode::Length),
            1302 | 0x516 => Ok(StringFunctionOpcode::Real),
            1303 | 0x517 => Ok(StringFunctionOpcode::Match),
            1304 | 0x518 => Ok(StringFunctionOpcode::After),
            1305 | 0x519 => Ok(StringFunctionOpcode::Before),
            1306 | 0x51A => Ok(StringFunctionOpcode::String),
            1307 | 0x51B => Ok(StringFunctionOpcode::Upcase),
            1308 | 0x51C => Ok(StringFunctionOpcode::Lowcase),
            1309 | 0x51D => Ok(StringFunctionOpcode::Substring),
            1311 | 0x51F => Ok(StringFunctionOpcode::Defined),
            1312 | 0x520 => Ok(StringFunctionOpcode::Undefined),
            1313 | 0x521 => Ok(StringFunctionOpcode::Size),
            1314 | 0x522 => Ok(StringFunctionOpcode::Trim),
            1315 | 0x523 => Ok(StringFunctionOpcode::Matchwild),
            1316 | 0x524 => Ok(StringFunctionOpcode::Width),
            1317 | 0x525 => Ok(StringFunctionOpcode::Part),
            1321 | 0x529 => Ok(StringFunctionOpcode::Occurs),
            1322 | 0x52A => Ok(StringFunctionOpcode::Replace),
            1369 | 0x559 => Ok(StringFunctionOpcode::Vtext),
            1370 | 0x55A => Ok(StringFunctionOpcode::Vvalue),
            1401 | 0x579 => Ok(StringFunctionOpcode::RealAlt),
            1410 | 0x582 => Ok(StringFunctionOpcode::Str),
            _ => Err(()),
        }
    }
}

// ============================================================================
// 布尔运算操作码 (case 3)
// ============================================================================

/// 布尔运算操作码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOpcode {
    /// 301 (0x12D) - 逻辑非
    Not = 301,
    /// 302 (0x12E) - 逻辑与
    And = 302,
    /// 303 (0x12F) - 逻辑或
    Or = 303,
}

impl BooleanOpcode {
    /// 返回操作数个数
    pub fn operand_count(&self) -> usize {
        match self {
            BooleanOpcode::Not => 1,
            BooleanOpcode::And | BooleanOpcode::Or => 2,
        }
    }

    /// 返回格式化模板
    pub fn format_template(&self) -> &'static str {
        match self {
            BooleanOpcode::Not => "NOT({})",
            BooleanOpcode::And => "{} AND {}",
            BooleanOpcode::Or => "{} OR {}",
        }
    }
}

impl TryFrom<i32> for BooleanOpcode {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            301 | 0x12D => Ok(BooleanOpcode::Not),
            302 | 0x12E => Ok(BooleanOpcode::And),
            303 | 0x12F => Ok(BooleanOpcode::Or),
            _ => Err(()),
        }
    }
}

// ============================================================================
// 比较运算操作码 (case 4, 5, 6)
// ============================================================================

/// 比较运算操作码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOpcode {
    /// 401 (0x191) - 等于
    Eq = 401,
    /// 501 (0x1F5) - 不等于
    Neq = 501,
    /// 601 (0x259) - 大于
    Gt = 601,
    /// 603 (0x25B) - 小于
    Lt = 603,
    /// 605 (0x25D) - 大于等于
    Ge = 605,
    /// 607 (0x25F) - 小于等于
    Le = 607,
}

impl ComparisonOpcode {
    /// 返回格式化模板
    pub fn format_template(&self) -> &'static str {
        match self {
            ComparisonOpcode::Eq => "{} EQ {}",
            ComparisonOpcode::Neq => "{} NEQ {}",
            ComparisonOpcode::Gt => "{} GT {}",
            ComparisonOpcode::Lt => "{} LT {}",
            ComparisonOpcode::Ge => "{} GE {}",
            ComparisonOpcode::Le => "{} LE {}",
        }
    }
}

impl TryFrom<i32> for ComparisonOpcode {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            401 | 0x191 => Ok(ComparisonOpcode::Eq),
            501 | 0x1F5 => Ok(ComparisonOpcode::Neq),
            601 | 0x259 => Ok(ComparisonOpcode::Gt),
            603 | 0x25B => Ok(ComparisonOpcode::Lt),
            605 | 0x25D => Ok(ComparisonOpcode::Ge),
            607 | 0x25F => Ok(ComparisonOpcode::Le),
            _ => Err(()),
        }
    }
}

// ============================================================================
// 通用函数操作码 (case 18)
// ============================================================================

/// 通用函数操作码
/// 
/// 包含 IFTRUE、DISTCONVERT 等高级控制函数
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneralFunctionOpcode {
    /// 1822 (0x071E) - 条件表达式 IFTRUE(condition, true_value, false_value)
    Iftrue = 1822,
    /// 1824 (0x0720) - 距离转换 DISTCONVERT(value)
    Distconvert = 1824,
    /// 1825 (0x0721) - SET 函数
    Set = 1825,
    /// 1826 (0x0722) - UNSET 函数
    Unset = 1826,
    /// 1827 (0x0723) - ARRAY 函数
    Array = 1827,
    /// 1828 (0x0724) - EMPTY 函数
    Empty = 1828,
    /// 1829 (0x0725) - SPLIT 函数
    Split = 1829,
}

impl GeneralFunctionOpcode {
    /// 返回操作数个数
    pub fn operand_count(&self) -> usize {
        match self {
            // 一元函数
            GeneralFunctionOpcode::Distconvert
            | GeneralFunctionOpcode::Unset
            | GeneralFunctionOpcode::Empty => 1,
            // 二元函数
            GeneralFunctionOpcode::Set
            | GeneralFunctionOpcode::Array
            | GeneralFunctionOpcode::Split => 2,
            // 三元函数
            GeneralFunctionOpcode::Iftrue => 3,
        }
    }

    /// 返回格式化模板
    pub fn format_template(&self) -> &'static str {
        match self {
            GeneralFunctionOpcode::Iftrue => "IFTRUE({},{},{})",
            GeneralFunctionOpcode::Distconvert => "DISTCONVERT({})",
            GeneralFunctionOpcode::Set => "SET({},{})",
            GeneralFunctionOpcode::Unset => "UNSET({})",
            GeneralFunctionOpcode::Array => "ARRAY({},{})",
            GeneralFunctionOpcode::Empty => "EMPTY({})",
            GeneralFunctionOpcode::Split => "SPLIT({},{})",
        }
    }
}

impl TryFrom<i32> for GeneralFunctionOpcode {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1822 | 0x071E => Ok(GeneralFunctionOpcode::Iftrue),
            1824 | 0x0720 => Ok(GeneralFunctionOpcode::Distconvert),
            1825 | 0x0721 => Ok(GeneralFunctionOpcode::Set),
            1826 | 0x0722 => Ok(GeneralFunctionOpcode::Unset),
            1827 | 0x0723 => Ok(GeneralFunctionOpcode::Array),
            1828 | 0x0724 => Ok(GeneralFunctionOpcode::Empty),
            1829 | 0x0725 => Ok(GeneralFunctionOpcode::Split),
            _ => Err(()),
        }
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 判断操作码是否为终止标记
#[inline]
pub fn is_terminator(opcode: i32) -> bool {
    opcode == 0 || opcode == 2
}

/// 判断操作码是否需要继续解析
#[inline]
pub fn should_continue_parsing(opcode: i32) -> bool {
    !is_terminator(opcode) && OpcodeCategory::from(opcode) != OpcodeCategory::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_category() {
        assert_eq!(OpcodeCategory::from(101), OpcodeCategory::Values);
        assert_eq!(OpcodeCategory::from(0x65), OpcodeCategory::Values);
        assert_eq!(OpcodeCategory::from(801), OpcodeCategory::Arithmetic);
        assert_eq!(OpcodeCategory::from(0x321), OpcodeCategory::Arithmetic);
        assert_eq!(OpcodeCategory::from(901), OpcodeCategory::Trigonometric);
        assert_eq!(OpcodeCategory::from(1001), OpcodeCategory::RealFunctions);
    }

    #[test]
    fn test_arithmetic_opcode() {
        assert!(ArithmeticOpcode::try_from(801).is_ok());
        assert!(ArithmeticOpcode::try_from(0x321).is_ok());
        assert_eq!(
            ArithmeticOpcode::try_from(802).unwrap().format_template(),
            "({}+{})"
        );
    }

    #[test]
    fn test_trigonometric_opcode() {
        assert!(TrigonometricOpcode::try_from(901).is_ok());
        assert!(TrigonometricOpcode::try_from(0x385).is_ok());
        assert_eq!(
            TrigonometricOpcode::try_from(901).unwrap().format_template(),
            "SIN({})"
        );
    }

    #[test]
    fn test_real_function_opcode() {
        assert!(RealFunctionOpcode::try_from(1001).is_ok());
        assert!(RealFunctionOpcode::try_from(0x3E9).is_ok());
        assert_eq!(RealFunctionOpcode::try_from(1008).unwrap().operand_count(), 2);
    }

    #[test]
    fn test_string_function_opcode() {
        // 测试基本解析
        assert!(StringFunctionOpcode::try_from(1301).is_ok()); // LENGTH
        assert!(StringFunctionOpcode::try_from(0x515).is_ok()); // LENGTH (hex)
        assert!(StringFunctionOpcode::try_from(1314).is_ok()); // TRIM
        assert!(StringFunctionOpcode::try_from(0x522).is_ok()); // TRIM (hex)
        
        // 测试操作数个数
        assert_eq!(StringFunctionOpcode::try_from(1301).unwrap().operand_count(), 1); // LENGTH: 1参数
        assert_eq!(StringFunctionOpcode::try_from(1303).unwrap().operand_count(), 2); // MATCH: 2参数
        assert_eq!(StringFunctionOpcode::try_from(1309).unwrap().operand_count(), 3); // SUBSTRING: 3参数
        
        // 测试格式化模板
        assert_eq!(
            StringFunctionOpcode::try_from(1301).unwrap().format_template(),
            "LEN({})"
        );
        assert_eq!(
            StringFunctionOpcode::try_from(1314).unwrap().format_template(),
            "TRIM({})"
        );
        assert_eq!(
            StringFunctionOpcode::try_from(1309).unwrap().format_template(),
            "SUBSTRING({},{},{})"
        );
    }

    #[test]
    fn test_general_function_opcode() {
        // 测试 IFTRUE
        assert!(GeneralFunctionOpcode::try_from(1822).is_ok());
        assert!(GeneralFunctionOpcode::try_from(0x071E).is_ok());
        assert_eq!(GeneralFunctionOpcode::try_from(1822).unwrap(), GeneralFunctionOpcode::Iftrue);
        
        // 测试 DISTCONVERT
        assert!(GeneralFunctionOpcode::try_from(1824).is_ok());
        assert!(GeneralFunctionOpcode::try_from(0x0720).is_ok());
        
        // 测试操作数个数
        assert_eq!(GeneralFunctionOpcode::try_from(1822).unwrap().operand_count(), 3); // IFTRUE: 3参数
        assert_eq!(GeneralFunctionOpcode::try_from(1824).unwrap().operand_count(), 1); // DISTCONVERT: 1参数
        assert_eq!(GeneralFunctionOpcode::try_from(1825).unwrap().operand_count(), 2); // SET: 2参数
        assert_eq!(GeneralFunctionOpcode::try_from(1826).unwrap().operand_count(), 1); // UNSET: 1参数
        
        // 测试格式化模板
        assert_eq!(
            GeneralFunctionOpcode::try_from(1822).unwrap().format_template(),
            "IFTRUE({},{},{})"
        );
        assert_eq!(
            GeneralFunctionOpcode::try_from(1826).unwrap().format_template(),
            "UNSET({})"
        );
    }

    #[test]
    fn test_boolean_opcode() {
        // 测试 NOT
        assert!(BooleanOpcode::try_from(301).is_ok());
        assert!(BooleanOpcode::try_from(0x12D).is_ok());
        assert_eq!(BooleanOpcode::try_from(301).unwrap(), BooleanOpcode::Not);
        
        // 测试 AND 和 OR
        assert!(BooleanOpcode::try_from(302).is_ok()); // AND
        assert!(BooleanOpcode::try_from(303).is_ok()); // OR
        
        // 测试操作数个数
        assert_eq!(BooleanOpcode::try_from(301).unwrap().operand_count(), 1); // NOT: 1参数
        assert_eq!(BooleanOpcode::try_from(302).unwrap().operand_count(), 2); // AND: 2参数
        assert_eq!(BooleanOpcode::try_from(303).unwrap().operand_count(), 2); // OR: 2参数
        
        // 测试格式化模板
        assert_eq!(BooleanOpcode::try_from(301).unwrap().format_template(), "NOT({})");
        assert_eq!(BooleanOpcode::try_from(302).unwrap().format_template(), "{} AND {}");
        assert_eq!(BooleanOpcode::try_from(303).unwrap().format_template(), "{} OR {}");
    }

    #[test]
    fn test_comparison_opcode() {
        // 测试 EQ
        assert!(ComparisonOpcode::try_from(401).is_ok());
        assert!(ComparisonOpcode::try_from(0x191).is_ok());
        assert_eq!(ComparisonOpcode::try_from(401).unwrap(), ComparisonOpcode::Eq);
        
        // 测试所有比较运算符
        assert!(ComparisonOpcode::try_from(501).is_ok()); // NEQ
        assert!(ComparisonOpcode::try_from(601).is_ok()); // GT
        assert!(ComparisonOpcode::try_from(603).is_ok()); // LT
        assert!(ComparisonOpcode::try_from(605).is_ok()); // GE
        assert!(ComparisonOpcode::try_from(607).is_ok()); // LE
        
        // 测试格式化模板
        assert_eq!(ComparisonOpcode::try_from(401).unwrap().format_template(), "{} EQ {}");
        assert_eq!(ComparisonOpcode::try_from(603).unwrap().format_template(), "{} LT {}");
    }
}
