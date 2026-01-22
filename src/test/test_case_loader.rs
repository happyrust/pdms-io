//! JSON 测试用例加载器
//!
//! 从 JSON 文件自动录入和运行表达式测试案例
//!
//! JSON 格式示例：
//! ```json
//! {
//!   "description": "PDMS 表达式解析测试用例集",
//!   "cases": [
//!     {
//!       "name": "PHEI 表达式测试",
//!       "refno": "13246/514326",
//!       "file_path": "D:/AVEVA/Projects/E3D2.1/AvevaMarineSample/ams000/ams5054_0001",
//!       "attr": "PHEI",
//!       "expected": "-1 TIMES SUM PARAM 4 PARAM 7",
//!       "enabled": true
//!     }
//!   ]
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// 测试用例 JSON 结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseJson {
    pub name: String,
    pub refno: String,
    pub file_path: String,
    #[serde(default)]
    pub fixture_path: Option<String>,
    pub attr: String,
    pub expected: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// 测试套件 JSON 结构
#[derive(Debug, Serialize, Deserialize)]
pub struct TestSuiteJson {
    pub description: Option<String>,
    pub cases: Vec<TestCaseJson>,
}

/// 从 JSON 文件加载测试用例
pub fn load_test_cases_from_json(path: &str) -> Result<Vec<TestCaseJson>, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("读取文件失败: {}", e))?;
    let suite: TestSuiteJson = serde_json::from_str(&content)
        .map_err(|e| format!("解析 JSON 失败: {}", e))?;
    
    // 只返回启用的测试用例
    Ok(suite.cases.into_iter().filter(|c| c.enabled).collect())
}

/// 保存测试用例到 JSON 文件
pub fn save_test_cases_to_json(path: &str, cases: &[TestCaseJson]) -> Result<(), String> {
    let suite = TestSuiteJson {
        description: Some("PDMS 表达式解析测试用例集".to_string()),
        cases: cases.to_vec(),
    };
    let content = serde_json::to_string_pretty(&suite)
        .map_err(|e| format!("序列化 JSON 失败: {}", e))?;
    fs::write(path, content).map_err(|e| format!("写入文件失败: {}", e))?;
    Ok(())
}

/// 从 CSV 文件加载测试用例（兼容旧格式）
pub fn load_test_cases_from_csv(path: &str) -> Result<Vec<TestCaseJson>, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("读取文件失败: {}", e))?;
    let mut cases = Vec::new();
    
    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        
        let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        if parts.len() < 5 {
            return Err(format!("第 {} 行格式错误: {}", line_num + 1, line));
        }
        
        cases.push(TestCaseJson {
            name: parts[0].to_string(),
            refno: parts[1].to_string(),
            file_path: parts[2].to_string(),
            fixture_path: None,
            attr: parts[3].to_string(),
            expected: parts[4].to_string(),
            enabled: true,
        });
    }
    
    Ok(cases)
}

/// 生成示例 JSON 文件
pub fn generate_sample_json(path: &str) -> Result<(), String> {
    let sample_cases = vec![
        TestCaseJson {
            name: "PHEI 表达式测试".to_string(),
            refno: "13246/514326".to_string(),
            file_path: "D:/AVEVA/Projects/E3D2.1/AvevaMarineSample/ams000/ams5054_0001".to_string(),
            fixture_path: None,
            attr: "PHEI".to_string(),
            expected: "-1 TIMES SUM PARAM 4 PARAM 7".to_string(),
            enabled: true,
        },
        TestCaseJson {
            name: "PBOF 表达式测试".to_string(),
            refno: "13246/514326".to_string(),
            file_path: "D:/AVEVA/Projects/E3D2.1/AvevaMarineSample/ams000/ams5054_0001".to_string(),
            fixture_path: None,
            attr: "PBOF".to_string(),
            expected: "PARAM 1 * PARAM 2".to_string(),
            enabled: true,
        },
    ];
    
    save_test_cases_to_json(path, &sample_cases)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_save_json() {
        let temp_dir = TempDir::new().unwrap();
        let json_path = temp_dir.path().join("test_cases.json");
        
        // 生成示例
        let sample_cases = vec![
            TestCaseJson {
                name: "测试1".to_string(),
                refno: "1/2".to_string(),
                file_path: "test.ams".to_string(),
                fixture_path: None,
                attr: "ATTR1".to_string(),
                expected: "EXPR1".to_string(),
                enabled: true,
            },
        ];
        
        save_test_cases_to_json(json_path.to_str().unwrap(), &sample_cases).unwrap();
        
        // 加载验证
        let loaded = load_test_cases_from_json(json_path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "测试1");
        assert_eq!(loaded[0].refno, "1/2");
    }
    
    #[test]
    fn test_load_csv() {
        let csv_content = r#"
# 测试用例 CSV 格式
# 格式: name, refno, file_path, attr, expected
测试1, 1/2, test.ams, ATTR1, EXPR1
测试2, 3/4, test2.ams, ATTR2, EXPR2
"#;
        
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("test_cases.csv");
        fs::write(&csv_path, csv_content).unwrap();
        
        let loaded = load_test_cases_from_csv(csv_path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "测试1");
        assert_eq!(loaded[1].name, "测试2");
    }
}
