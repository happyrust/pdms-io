//! 表达式解析验证工具
//!
//! 使用流程：
//! 1. 使用 PdmsIO 打开 AMS 文件
//! 2. 通过 search_latest_refno 定位 refno 的物理偏移
//! 3. 读取完整的元素数据
//! 4. 使用 parse_ele_data 解析并验证表达式
//!
//! 使用示例：
//! ```rust
//! #[tokio::test]
//! async fn test_phei_expression() {
//!     let case = TestCase {
//!         refno: "13246/514326",
//!         file_path: "ams5054_0001",
//!         attr: "PHEI",
//!         expected: "-1 TIMES SUM PARAM 4 PARAM 7",
//!     };
//!     assert!(test_expression_from_file(&case).await.is_ok());
//! }
//! ```

use crate::io::PdmsIO;
use crate::test::resolve_test_db_path;
use aios_core::RefU64;
use parse_pdms_db::parse::parse_ele_data;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;

/// 测试用例定义
pub struct TestCase {
    /// 参考号字符串 (如 "13246/514326")
    pub refno: String,
    /// AMS 文件完整路径
    pub file_path: String,
    /// 属性名 (如 "PHEI")
    pub attr: String,
    /// 期望的表达式输出
    pub expected: String,
}

/// 解析 refno 字符串为 RefU64
fn parse_refno(refno_str: &str) -> Result<RefU64, String> {
    let parts: Vec<&str> = refno_str.split('/').collect();
    if parts.len() != 2 {
        return Err(format!("无效的 refno 格式: {}", refno_str));
    }
    let db_idx: u32 = parts[0].parse().map_err(|_| "无法解析 db_idx")?;
    let ele_idx: u32 = parts[1].parse().map_err(|_| "无法解析 ele_idx")?;
    Ok(RefU64::from_two_nums(db_idx, ele_idx))
}

/// 直接从文件读取并解析元素
pub async fn test_expression_from_file(case: &TestCase) -> Result<String, String> {
    match File::open(&case.file_path) {
        Ok(mut file) => {
            let mut data = Vec::new();
            file.read_to_end(&mut data).map_err(|e| e.to_string())?;
            
            let ele_data = parse_ele_data(&data).await.map_err(|e| e.to_string())?;
            let map = &ele_data.whole_attmap.attmap;
            let result = map.get_as_string(&case.attr).unwrap_or_default();
            
            if result.trim() == case.expected.trim() {
                Ok(result)
            } else {
                Err(format!(
                    "表达式不匹配:\n  期望: '{}'\n  实际: '{}'",
                    case.expected, result
                ))
            }
        }
        Err(e) => Err(format!("文件打开失败: {}", e)),
    }
}

/// 通过 pdms-io 测试表达式解析
pub async fn test_expression_via_pdmsio(case: &TestCase) -> Result<String, String> {
    // 1. 解析 refno
    let refno = parse_refno(&case.refno)?;
    
    // 2. 打开 PDMS 文件
    let mut io = PdmsIO::new(case.file_path.clone(), case.file_path.as_str(), false);
    io.open().map_err(|e| format!("打开文件失败: {}", e))?;
    
    // 3. 定位 refno 的物理偏移
    let (sesno, offset) = io
        .search_latest_refno(refno, None)
        .ok_or_else(|| format!("找不到 refno: {}", case.refno))?;
    
    println!("[DEBUG] Refno {} => sesno={}, offset={:#X}", case.refno, sesno, offset);

    // 4. 读取“单条元素 record”（跨页 + 截断），避免把下一条 record 误吞进来
    let record = io
        .read_element_record_cached(offset)
        .map_err(|e| format!("读取元素 record 失败: {}", e))?;
    let mut input = record.as_slice();
    while input.len() >= 4 && (input[..4] == [0, 0, 0, 0] || input[..4] == [0, 0, 0, 7]) {
        input = &input[4..];
    }

    // 5. 解析元素数据
    let ele_data = parse_ele_data(input)
        .await
        .map_err(|e| format!("解析失败: {}", e))?;
    let map = &ele_data.whole_attmap.attmap;
    let result = map.get_as_string(&case.attr).unwrap_or_default();
    
    println!("[DEBUG] {} 解析结果: '{}'", case.attr, result);
    
    if result.trim() == case.expected.trim() {
        Ok(result)
    } else {
        Err(format!(
            "表达式不匹配:\n  期望: '{}'\n  实际: '{}'",
            case.expected, result
        ))
    }
}

/// 打印元素的二进制结构（用于调试）
pub async fn dump_element_structure(file_path: &str, refno_str: &str) -> Result<(), String> {
    let refno = parse_refno(refno_str)?;
    
    let mut io = PdmsIO::new(file_path.to_string(), file_path, false);
    io.open().map_err(|e| format!("打开文件失败: {}", e))?;
    
    let (sesno, offset) = io.search_latest_refno(refno, None)
        .ok_or_else(|| format!("找不到 refno: {}", refno_str))?;
    
    println!("=== 元素结构 ===");
    println!("Refno: {}", refno_str);
    println!("Sesno: {}", sesno);
    println!("Offset: {:#X}", offset);

    let record = io
        .read_element_record_cached(offset)
        .map_err(|e| format!("读取元素 record 失败: {}", e))?;
    let mut input = record.as_slice();
    while input.len() >= 4 && (input[..4] == [0, 0, 0, 0] || input[..4] == [0, 0, 0, 7]) {
        input = &input[4..];
    }

    if input.len() >= 4 {
        let impl_len = i32::from_be_bytes(input[0..4].try_into().unwrap());
        println!("ImplLen: {}", impl_len);
    }
    println!("RecordBytes: {}", input.len());

    // 经验：隐式区头常为 24B；这里仅用于调试展示
    let attr_data = input.get(24..).unwrap_or(&[]);
    
    println!("\n属性数据 (共 {} 字节):", attr_data.len());
    print_hex_dump(attr_data, 16);
    
    Ok(())
}

/// 导出元素夹具文件
pub fn export_element_fixture_via_pdmsio(
    ams_file_path: &str,
    refno_str: &str,
    out_path: &str,
) -> Result<(), String> {
    let refno = parse_refno(refno_str)?;

    let mut io = PdmsIO::new(ams_file_path.to_string(), ams_file_path, false);
    io.open().map_err(|e| format!("打开文件失败: {}", e))?;

    let (_sesno, offset) = io
        .search_latest_refno(refno, None)
        .ok_or_else(|| format!("找不到 refno: {}", refno_str))?;

    let record = io
        .read_element_record_cached(offset)
        .map_err(|e| format!("读取元素 record 失败: {}", e))?;
    let mut input = record.as_slice();
    while input.len() >= 4 && (input[..4] == [0, 0, 0, 0] || input[..4] == [0, 0, 0, 7]) {
        input = &input[4..];
    }

    let mut f = File::create(out_path).map_err(|e| format!("创建输出文件失败: {}", e))?;
    f.write_all(input)
        .map_err(|e| format!("写入输出文件失败: {}", e))?;
    Ok(())
}

/// 十六进制打印辅助函数
fn print_hex_dump(data: &[u8], bytes_per_line: usize) {
    let mut line = String::new();
    for (i, byte) in data.iter().enumerate() {
        if i % bytes_per_line == 0 && i > 0 {
            println!("{}", line);
            line.clear();
        }
        line.push_str(&format!("{:02X} ", byte));
    }
    if !line.is_empty() {
        println!("{}", line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 示例：PHEI 表达式测试
    #[ignore]
    #[tokio::test]
    async fn test_phei_expression() {
        let file_path = std::env::var("PDMS_ELE_FIXTURE")
            .unwrap_or_else(|_| "".to_string());
        assert!(
            !file_path.is_empty() && Path::new(&file_path).exists(),
            "未得元素夹具文件：请先生成并设置环境变量 PDMS_ELE_FIXTURE 指向单元素二进制文件。当前值='{}'",
            file_path
        );
        let case = TestCase {
            refno: "13246/514326".to_string(),
            file_path,
            attr: "PHEI".to_string(),
            expected: "-1 TIMES SUM PARAM 4 PARAM 7".to_string(),
        };

        // 从文件测试
        let result = test_expression_from_file(&case)
            .await
            .expect("PHEI 表达式解析失败（from_file）");
        println!("✓ PHEI 解析结果: '{}'", result);
        assert_eq!(result.trim(), case.expected.trim());
    }

    /// 通过 pdms-io 测试 PHEI
    #[ignore]
    #[tokio::test]
    async fn test_phei_via_pdmsio() {
        let file_path = std::env::var("PDMS_AMS_FILE")
            .ok()
            .filter(|val| !val.trim().is_empty())
            .or_else(|| resolve_test_db_path("ams1112_0001").map(|p| p.display().to_string()))
            .unwrap_or_default();
        assert!(
            !file_path.is_empty() && Path::new(&file_path).exists(),
            "未得 AMS 文件路径：请设置环境变量 PDMS_AMS_FILE，或提供本地 test-file/ams1112_0001。当前值='{}'",
            file_path
        );
        let case = TestCase {
            refno: "13246/514326".to_string(),
            file_path,
            attr: "PHEI".to_string(),
            expected: "-1 TIMES SUM PARAM 4 PARAM 7".to_string(),
        };

        let result = test_expression_via_pdmsio(&case)
            .await
            .expect("PHEI 表达式解析失败（via_pdmsio）");
        println!("✓ PHEI (via pdms-io) 解析结果: '{}'", result);
        assert_eq!(result.trim(), case.expected.trim());
    }

    #[ignore]
    #[test]
    fn dump_phei_fixture() {
        let ams_file_path = std::env::var("PDMS_AMS_FILE").unwrap_or_else(|_| "".to_string());
        assert!(
            !ams_file_path.is_empty() && Path::new(&ams_file_path).exists(),
            "未得 AMS 文件路径：请设置环境变量 PDMS_AMS_FILE。当前值='{}'",
            ams_file_path
        );

        let out_dir = "test_output";
        std::fs::create_dir_all(out_dir).expect("创建 test_output 失败");
        let out_path = format!("{}/phei_13246_514326.element.bin", out_dir);

        export_element_fixture_via_pdmsio(&ams_file_path, "13246/514326", &out_path)
            .expect("导出元素夹具失败");
        println!("夹具已写入: {}", out_path);
    }

     #[ignore]
     #[tokio::test]
     async fn test_expression_cases_from_json() {
         let json_path = std::env::var("PDMS_EXPR_TEST_JSON").unwrap_or_else(|_| "".to_string());
         assert!(
             !json_path.is_empty() && Path::new(&json_path).exists(),
             "未得 JSON 用例路径：请设置环境变量 PDMS_EXPR_TEST_JSON。当前值='{}'",
             json_path
         );

         let cases = crate::test::test_case_loader::load_test_cases_from_json(&json_path)
             .expect("加载 JSON 用例失败");
         assert!(!cases.is_empty(), "JSON 用例为空: '{}'", json_path);

         for c in cases {
             let name = c.name.clone();
             let fixture_path = c
                 .fixture_path
                 .as_ref()
                 .map(|s| s.trim())
                 .filter(|s| !s.is_empty())
                 .map(|s| s.to_string());

             if let Some(fixture_path) = fixture_path {
                 assert!(
                     Path::new(&fixture_path).exists(),
                     "用例 '{}' 夹具文件不存在: '{}'",
                     name,
                     fixture_path
                 );
                 let case = TestCase {
                     refno: c.refno.clone(),
                     file_path: fixture_path,
                     attr: c.attr.clone(),
                     expected: c.expected.clone(),
                 };
                 test_expression_from_file(&case)
                     .await
                     .unwrap_or_else(|e| panic!("用例 '{}' 失败: {}", name, e));
                 continue;
             }

            let ams_path = if !c.file_path.trim().is_empty() {
                c.file_path.clone()
            } else {
                std::env::var("PDMS_AMS_FILE")
                    .ok()
                    .filter(|val| !val.trim().is_empty())
                    .or_else(|| resolve_test_db_path("ams1112_0001").map(|p| p.display().to_string()))
                    .unwrap_or_default()
            };
            assert!(
                !ams_path.is_empty() && Path::new(&ams_path).exists(),
                "用例 '{}' 未得 AMS 路径：请在 JSON 的 file_path 填入，或设置环境变量 PDMS_AMS_FILE，或提供本地 test-file/ams1112_0001。当前值='{}'",
                name,
                ams_path
            );
             let case = TestCase {
                 refno: c.refno.clone(),
                 file_path: ams_path,
                 attr: c.attr.clone(),
                 expected: c.expected.clone(),
             };
             test_expression_via_pdmsio(&case)
                 .await
                 .unwrap_or_else(|e| panic!("用例 '{}' 失败: {}", name, e));
         }
     }
}
