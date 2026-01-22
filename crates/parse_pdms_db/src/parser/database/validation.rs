//! 数据库文件验证
//!
//! 提供 PDMS 数据库文件的验证功能

use super::header::{extract_db_type, DbType};
use std::path::Path;

/// 已知的数据库类型名称
pub const KNOWN_DB_TYPES: [&str; 6] = ["DESI", "CATA", "DICT", "SYST", "GLB", "GLOB"];

/// 检查文件扩展名是否为 PDMS 数据库
///
/// 支持的扩展名：.db, 无扩展名（数字文件名如 "001"）
#[inline]
pub fn is_db_file_extension(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        ext.eq_ignore_ascii_case("db")
    } else {
        // 无扩展名，检查是否为纯数字文件名
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or(false)
    }
}

/// 检查文件头是否包含有效的 PDMS 数据库类型
///
/// # 参数
/// - `header`: 文件头数据（至少 36 字节）
#[inline]
pub fn is_valid_db_header(header: &[u8]) -> bool {
    extract_db_type(header)
        .map(|t| t.is_valid())
        .unwrap_or(false)
}

/// 验证结果
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// 是否有效
    pub is_valid: bool,
    /// 数据库类型（如果识别）
    pub db_type: Option<DbType>,
    /// 错误信息（如果无效）
    pub error: Option<String>,
}

impl ValidationResult {
    /// 创建有效结果
    pub fn valid(db_type: DbType) -> Self {
        Self {
            is_valid: true,
            db_type: Some(db_type),
            error: None,
        }
    }

    /// 创建无效结果
    pub fn invalid(error: impl Into<String>) -> Self {
        Self {
            is_valid: false,
            db_type: None,
            error: Some(error.into()),
        }
    }
}

/// 完整验证数据库文件头
///
/// # 返回
/// - `ValidationResult` 包含验证结果和详细信息
pub fn validate_db_header(header: &[u8]) -> ValidationResult {
    if header.len() < 36 {
        return ValidationResult::invalid(format!(
            "Header too short: {} bytes, need at least 36",
            header.len()
        ));
    }

    match extract_db_type(header) {
        Some(db_type) if db_type.is_valid() => ValidationResult::valid(db_type),
        Some(_) => ValidationResult::invalid("Unknown database type"),
        None => ValidationResult::invalid("Failed to parse database type"),
    }
}

/// 检查数据完整性
///
/// 验证数据长度是否与索引区声明的一致
pub fn check_data_integrity(data: &[u8], expected_len: usize) -> bool {
    data.len() >= expected_len
}

/// 检查元素数据边界
///
/// 确保偏移量和长度不超出数据范围
#[inline]
pub fn check_element_bounds(data_len: usize, offset: u32, length: u32) -> bool {
    let end = offset as usize + length as usize;
    end <= data_len
}

#[cfg(test)]
mod tests {
    use super::*;
    use aios_core::tool::db_tool::db1_hash;
    use std::path::PathBuf;

    #[test]
    fn test_is_db_file_extension() {
        assert!(is_db_file_extension(Path::new("test.db")));
        assert!(is_db_file_extension(Path::new("test.DB")));
        assert!(is_db_file_extension(Path::new("001")));
        assert!(is_db_file_extension(Path::new("123")));
        assert!(!is_db_file_extension(Path::new("test.txt")));
        assert!(!is_db_file_extension(Path::new("test.com")));
        assert!(!is_db_file_extension(Path::new("abc"))); // 非数字无扩展名
    }

    #[test]
    fn test_is_valid_db_header() {
        let mut header = vec![0u8; 64];
        let hash = db1_hash("DESI") as i32;
        header[32..36].copy_from_slice(&hash.to_be_bytes());
        assert!(is_valid_db_header(&header));

        // 无效类型
        header[32..36].copy_from_slice(b"FAKE");
        assert!(!is_valid_db_header(&header));

        // 太短
        assert!(!is_valid_db_header(&[0u8; 20]));
    }

    #[test]
    fn test_validate_db_header() {
        let mut header = vec![0u8; 64];
        let hash = db1_hash("CATA") as i32;
        header[32..36].copy_from_slice(&hash.to_be_bytes());

        let result = validate_db_header(&header);
        assert!(result.is_valid);
        assert_eq!(result.db_type, Some(DbType::Catalog));
        assert!(result.error.is_none());
    }

    #[test]
    fn test_validate_db_header_short() {
        let result = validate_db_header(&[0u8; 20]);
        assert!(!result.is_valid);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("too short"));
    }

    #[test]
    fn test_check_data_integrity() {
        let data = vec![0u8; 1000];
        assert!(check_data_integrity(&data, 500));
        assert!(check_data_integrity(&data, 1000));
        assert!(!check_data_integrity(&data, 1001));
    }

    #[test]
    fn test_check_element_bounds() {
        assert!(check_element_bounds(1000, 0, 100));
        assert!(check_element_bounds(1000, 900, 100));
        assert!(!check_element_bounds(1000, 900, 200)); // 超出
        assert!(!check_element_bounds(1000, 1001, 0)); // 偏移超出
    }
}
