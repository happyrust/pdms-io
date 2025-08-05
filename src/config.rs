//! 配置管理模块
//! 
//! 提供统一的环境变量配置管理，特别是测试路径的配置

use std::env;
use std::path::PathBuf;

/// 环境变量名称常量
pub const PDMS_TEST_PATH_ENV: &str = "PDMS_TEST_PATH";
/// 新的通用环境变量名称（推荐使用）
pub const PDMS_PROJECT_PATH_ENV: &str = "PDMS_PROJECT_PATH";

/// 默认的测试路径
pub const DEFAULT_TEST_PATH: &str = "/Volumes/DPC/work/e3d_models";

/// 配置管理结构体
pub struct Config;

impl Config {
    /// 获取测试基础路径
    /// 
    /// 优先从环境变量 PDMS_PROJECT_PATH 获取基础路径，
    /// 如果未设置则尝试 PDMS_TEST_PATH，
    /// 如果都未设置则使用默认路径
    /// 
    /// # 返回值
    /// * `String` - 测试基础路径
    pub fn get_test_base_path() -> String {
        env::var(PDMS_PROJECT_PATH_ENV)
            .or_else(|_| env::var(PDMS_TEST_PATH_ENV))
            .unwrap_or_else(|_| DEFAULT_TEST_PATH.to_string())
    }
    
    /// 获取完整的数据库路径
    /// 
    /// 支持两种方式：
    /// 1. 如果输入是相对路径，则与环境变量 PDMS_TEST_PATH 组合
    /// 2. 如果输入是绝对路径，则直接使用
    /// 
    /// # 参数
    /// * `input_path` - 输入的路径字符串
    /// 
    /// # 返回值
    /// * `String` - 完整的数据库路径
    pub fn get_database_path(input_path: &str) -> String {
        let path = PathBuf::from(input_path);
        
        // 如果是绝对路径，直接返回
        if path.is_absolute() {
            return input_path.to_string();
        }
        
        // 如果是相对路径，与基础路径组合
        let base_path = Self::get_test_base_path();
        let full_path = PathBuf::from(base_path).join(input_path);
        
        full_path.to_string_lossy().to_string()
    }
    
    /// 检查数据库文件是否存在
    /// 
    /// # 参数
    /// * `db_path` - 数据库路径
    /// 
    /// # 返回值
    /// * `bool` - 文件是否存在
    pub fn database_exists(db_path: &str) -> bool {
        std::path::Path::new(db_path).exists()
    }
    
    /// 获取环境变量配置信息
    /// 
    /// # 返回值
    /// * `ConfigInfo` - 配置信息结构体
    pub fn get_config_info() -> ConfigInfo {
        let base_path = Self::get_test_base_path();
        let is_env_set = env::var(PDMS_TEST_PATH_ENV).is_ok();
        
        ConfigInfo {
            base_path,
            is_env_set,
            env_var_name: PDMS_TEST_PATH_ENV.to_string(),
            default_path: DEFAULT_TEST_PATH.to_string(),
        }
    }
    
    /// 打印配置信息
    pub fn print_config_info() {
        let info = Self::get_config_info();
        println!("📋 PDMS 配置信息:");
        println!("   环境变量: {}", info.env_var_name);
        println!("   是否已设置: {}", if info.is_env_set { "是" } else { "否" });
        println!("   当前基础路径: {}", info.base_path);
        println!("   默认路径: {}", info.default_path);
        
        if !info.is_env_set {
            println!("   💡 提示: 可以通过设置环境变量来自定义基础路径:");
            println!("      export {}=\"/your/custom/path\"", info.env_var_name);
        }
    }
}

/// 配置信息结构体
#[derive(Debug, Clone)]
pub struct ConfigInfo {
    /// 当前使用的基础路径
    pub base_path: String,
    /// 环境变量是否已设置
    pub is_env_set: bool,
    /// 环境变量名称
    pub env_var_name: String,
    /// 默认路径
    pub default_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    
    #[test]
    fn test_get_test_base_path_default() {
        // 临时移除环境变量
        let original = env::var(PDMS_TEST_PATH_ENV).ok();
        env::remove_var(PDMS_TEST_PATH_ENV);
        
        let path = Config::get_test_base_path();
        assert_eq!(path, DEFAULT_TEST_PATH);
        
        // 恢复原始环境变量
        if let Some(original_value) = original {
            env::set_var(PDMS_TEST_PATH_ENV, original_value);
        }
    }
    
    #[test]
    fn test_get_test_base_path_from_env() {
        let test_path = "/tmp/test_path";
        env::set_var(PDMS_TEST_PATH_ENV, test_path);
        
        let path = Config::get_test_base_path();
        assert_eq!(path, test_path);
        
        // 清理
        env::remove_var(PDMS_TEST_PATH_ENV);
    }
    
    #[test]
    fn test_get_database_path_absolute() {
        let absolute_path = "/absolute/path/to/database";
        let result = Config::get_database_path(absolute_path);
        assert_eq!(result, absolute_path);
    }
    
    #[test]
    fn test_get_database_path_relative() {
        let test_base = "/tmp/test_base";
        env::set_var(PDMS_TEST_PATH_ENV, test_base);
        
        let relative_path = "ams000/ams1112_0001";
        let result = Config::get_database_path(relative_path);
        let expected = format!("{}/{}", test_base, relative_path);
        assert_eq!(result, expected);
        
        // 清理
        env::remove_var(PDMS_TEST_PATH_ENV);
    }
    
    #[test]
    fn test_config_info() {
        let info = Config::get_config_info();
        assert_eq!(info.env_var_name, PDMS_TEST_PATH_ENV);
        assert_eq!(info.default_path, DEFAULT_TEST_PATH);
        assert!(!info.base_path.is_empty());
    }
}
