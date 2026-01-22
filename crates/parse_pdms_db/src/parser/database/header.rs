//! 数据库文件头解析器
//!
//! 提供 PDMS 数据库文件头的解析功能

use aios_core::tool::db_tool::db1_dehash;
use nom::number::complete::{be_i32, be_u32};
use nom::IResult;

/// 数据库类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbType {
    /// 设计数据库
    Design,
    /// 目录数据库
    Catalog,
    /// 字典数据库
    Dictionary,
    /// 系统数据库
    System,
    /// 全局数据库
    Global,
    /// 未知类型
    Unknown,
}

impl DbType {
    /// 从类型名称创建
    pub fn from_name(name: &str) -> Self {
        match name.to_ascii_uppercase().as_str() {
            "DESI" => DbType::Design,
            "CATA" => DbType::Catalog,
            "DICT" => DbType::Dictionary,
            "SYST" => DbType::System,
            "GLB" | "GLOB" => DbType::Global,
            _ => DbType::Unknown,
        }
    }

    /// 从哈希值创建
    pub fn from_hash(hash: i32) -> Self {
        let name = db1_dehash(hash as u32);
        Self::from_name(&name)
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            DbType::Design => "DESI",
            DbType::Catalog => "CATA",
            DbType::Dictionary => "DICT",
            DbType::System => "SYST",
            DbType::Global => "GLOB",
            DbType::Unknown => "UNKNOWN",
        }
    }

    /// 是否为有效类型
    pub fn is_valid(&self) -> bool {
        !matches!(self, DbType::Unknown)
    }
}

impl std::fmt::Display for DbType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 数据库文件头结构
///
/// PDMS 数据库文件的固定头部信息
#[derive(Debug, Clone)]
pub struct DbHeader {
    /// 数据库编号
    pub dbnum: i32,
    /// 字段编号
    pub field_no: i32,
    /// 数据库类型哈希
    pub type_hash: i32,
    /// 数据库类型
    pub db_type: DbType,
    /// 索引区偏移量
    pub index_offset: u32,
    /// 数据区偏移量
    pub data_offset: u32,
}

impl DbHeader {
    /// 获取数据库类型名称
    pub fn type_name(&self) -> String {
        db1_dehash(self.type_hash as u32)
    }
}

/// 文件头偏移常量
/// 
/// PDMS 数据库文件头格式:
/// - bytes[0..4]: 保留字段 (通常为 0)
/// - bytes[4..8]: 字段编号 (field_no)
/// - bytes[8..12]: 数据库编号 (dbnum)
/// - bytes[32..36]: 类型哈希 (type_hash)
pub mod offsets {
    /// 保留字段偏移 (通常为 0)
    pub const RESERVED: usize = 0;
    /// 字段编号偏移
    pub const FIELD_NO: usize = 4;
    /// 数据库编号偏移
    pub const DB_NO: usize = 8;
    /// 类型哈希偏移
    pub const TYPE_HASH: usize = 32;
    /// 索引区偏移量位置
    pub const INDEX_OFFSET: usize = 36;
    /// 数据区偏移量位置
    pub const DATA_OFFSET: usize = 40;
    /// 最小头部长度
    pub const MIN_HEADER_SIZE: usize = 64;
}

/// 解析数据库文件头
///
/// # 格式
/// - bytes[0..4]: 保留字段 (通常为 0)
/// - bytes[4..8]: 字段编号
/// - bytes[8..12]: 数据库编号
/// - bytes[32..36]: 类型哈希
/// - bytes[36..40]: 索引区偏移
/// - bytes[40..44]: 数据区偏移
pub fn parse_db_header(input: &[u8]) -> IResult<&[u8], DbHeader> {
    use offsets::*;

    if input.len() < MIN_HEADER_SIZE {
        return Err(nom::Err::Incomplete(nom::Needed::new(
            MIN_HEADER_SIZE - input.len(),
        )));
    }

    let (_, dbnum) = be_i32(&input[DB_NO..DB_NO + 4])?;
    let (_, field_no) = be_i32(&input[FIELD_NO..FIELD_NO + 4])?;
    let (_, type_hash) = be_i32(&input[TYPE_HASH..TYPE_HASH + 4])?;
    let (_, index_offset) = be_u32(&input[INDEX_OFFSET..INDEX_OFFSET + 4])?;
    let (_, data_offset) = be_u32(&input[DATA_OFFSET..DATA_OFFSET + 4])?;

    let db_type = DbType::from_hash(type_hash);

    Ok((
        &input[MIN_HEADER_SIZE..],
        DbHeader {
            dbnum: dbnum,
            field_no,
            type_hash,
            db_type,
            index_offset,
            data_offset,
        },
    ))
}

/// 快速提取数据库类型
#[inline]
pub fn extract_db_type(input: &[u8]) -> Option<DbType> {
    if input.len() < 36 {
        return None;
    }
    let type_hash = i32::from_be_bytes(input[32..36].try_into().ok()?);
    Some(DbType::from_hash(type_hash))
}

/// 快速提取数据库编号
/// 
/// PDMS 文件头格式:
/// - bytes[0..4]: 保留字段 (通常为 0)
/// - bytes[4..8]: 字段编号
/// - bytes[8..12]: 数据库编号 (dbnum)
#[inline]
pub fn extract_db_no(input: &[u8]) -> Option<i32> {
    if input.len() < 12 {
        return None;
    }
    Some(i32::from_be_bytes(input[8..12].try_into().ok()?))
}

/// 快速提取字段编号
#[inline]
pub fn extract_field_no(input: &[u8]) -> Option<i32> {
    if input.len() < 8 {
        return None;
    }
    Some(i32::from_be_bytes(input[4..8].try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aios_core::tool::db_tool::db1_hash;

    fn make_test_header(db_type: &str) -> Vec<u8> {
        let mut data = vec![0u8; 64];
        // reserved = 0 (offset 0-4)
        data[0..4].copy_from_slice(&0i32.to_be_bytes());
        // field_no = 2 (offset 4-8)
        data[4..8].copy_from_slice(&2i32.to_be_bytes());
        // dbnum = 1 (offset 8-12) - PDMS 数据库编号存储在此位置
        data[8..12].copy_from_slice(&1i32.to_be_bytes());
        // type_hash (offset 32-36)
        let hash = db1_hash(db_type) as i32;
        data[32..36].copy_from_slice(&hash.to_be_bytes());
        // index_offset = 1024 (offset 36-40)
        data[36..40].copy_from_slice(&1024u32.to_be_bytes());
        // data_offset = 2048 (offset 40-44)
        data[40..44].copy_from_slice(&2048u32.to_be_bytes());
        data
    }

    #[test]
    fn test_db_type_from_name() {
        assert_eq!(DbType::from_name("DESI"), DbType::Design);
        assert_eq!(DbType::from_name("desi"), DbType::Design);
        assert_eq!(DbType::from_name("CATA"), DbType::Catalog);
        assert_eq!(DbType::from_name("DICT"), DbType::Dictionary);
        assert_eq!(DbType::from_name("SYST"), DbType::System);
        assert_eq!(DbType::from_name("GLB"), DbType::Global);
        assert_eq!(DbType::from_name("GLOB"), DbType::Global);
        assert_eq!(DbType::from_name("UNKNOWN"), DbType::Unknown);
    }

    #[test]
    fn test_db_type_is_valid() {
        assert!(DbType::Design.is_valid());
        assert!(DbType::Catalog.is_valid());
        assert!(!DbType::Unknown.is_valid());
    }

    #[test]
    fn test_parse_db_header() {
        let data = make_test_header("DESI");
        let (rest, header) = parse_db_header(&data).unwrap();

        assert!(rest.is_empty());
        assert_eq!(header.dbnum, 1);
        assert_eq!(header.field_no, 2);
        assert_eq!(header.db_type, DbType::Design);
        assert_eq!(header.index_offset, 1024);
        assert_eq!(header.data_offset, 2048);
    }

    #[test]
    fn test_extract_db_type() {
        let data = make_test_header("CATA");
        let db_type = extract_db_type(&data).unwrap();
        assert_eq!(db_type, DbType::Catalog);
    }

    #[test]
    fn test_extract_db_no() {
        let data = make_test_header("DESI");
        assert_eq!(extract_db_no(&data), Some(1));
    }

    #[test]
    fn test_extract_field_no() {
        let data = make_test_header("DESI");
        assert_eq!(extract_field_no(&data), Some(2));
    }

    #[test]
    fn test_short_data() {
        let short = [0u8; 32];
        assert!(parse_db_header(&short).is_err());
        assert!(extract_db_type(&short).is_none());
    }
}
