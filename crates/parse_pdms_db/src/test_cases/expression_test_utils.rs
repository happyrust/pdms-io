//! 表达式解析验证工具
//!
//! 方案2：通过 pdms-io 定位 refno 的二进制数据，然后定位属性数据
//!
//! 使用流程：
//! 1. 使用 PdmsIO 打开 AMS 文件
//! 2. 通过 search_latest_refno 定位 refno 的物理偏移
//! 3. 读取属性区域的原始字节数据
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
//!     assert!(test_expression_via_pdmsio(&case).await.is_ok());
//! }
//! ```

use crate::parse::parse_ele_data;
use crate::test_cases::convert_str_to_bytes;
use pdms_io::io::PdmsIO;
use aios_core::RefU64;
use std::fs::File;
use std::io::Read;

/// 测试用例定义
pub struct TestCase {
    /// 参考号字符串 (如 "13246/514326")
    pub refno: &'static str,
    /// AMS 文件完整路径
    pub file_path: &'static str,
    /// 属性名 (如 "PHEI")
    pub attr: &'static str,
    /// 期望的表达式输出
    pub expected: &'static str,
}

/// 解析 refno 字符串为 RefU64
fn parse_refno(refno_str: &str) -> Result<RefU64, String> {
    let parts: Vec<&str> = refno_str.split('/').collect();
    if parts.len() != 2 {
        return Err(format!("无效的 refno 格式: {}", refno_str));
    }
    let db_idx: u32 = parts[0].parse().map_err(|_| "无法解析 db_idx")?;
    let ele_idx: u32 = parts[1].parse().map_err(|_| "无法解析 ele_idx")?;
    Ok(RefU64::from_two_nums(db_idx as i32, ele_idx as i32))
}

/// 通过 pdms-io 定位 refno 并提取属性二进制数据
///
/// 流程：
/// 1. 打开 AMS 文件
/// 2. search_latest_refno 定位 refno 的物理偏移
/// 3. 读取元素头部获取 impl_len
/// 4. 读取完整的元素数据
/// 5. 定位到指定属性的数据区域
pub async fn extract_attr_bytes_via_pdmsio(
    file_path: &str,
    refno_str: &str,
    attr_name: &str,
) -> Result<Vec<u8>, String> {
    // 1. 解析 refno
    let refno = parse_refno(refno_str)?;
    
    // 2. 打开 PDMS 文件
    let mut io = PdmsIO::new(file_path.to_string(), None)
        .map_err(|e| format!("打开文件失败: {}", e))?;
    
    // 3. 定位 refno 的物理偏移
    let (sesno, offset) = io
        .search_latest_refno(refno, None)
        .ok_or_else(|| format!("找不到 refno: {}", refno_str))?;
    
    println!("[DEBUG] Refno {} => sesno={}, offset={:#X}", refno_str, sesno, offset);
    
    // 4. 读取元素头部 (至少 24 字节)
    let head = io.read_data_cached(offset, 24)
        .map_err(|e| format!("读取头部失败: {}", e))?;
    
    // 解析头部获取 impl_len
    let header_start = if head[..4] == [0, 0, 0, 0x7] { &head[4..] } else { &head[..] };
    let impl_len = i32::from_be_bytes(header_start[20..24].try_into().unwrap());
    let total_len = 24 + impl_len * 4;
    
    println!("[DEBUG] impl_len={}, total_len={}", impl_len, total_len);
    
    // 5. 读取完整的元素数据
    let full_data = io.read_data_cached(offset, total_len as usize)
        .map_err(|e| format!("读取元素数据失败: {}", e))?;
    
    // 6. 定位属性区域
    // 属性区域从 offset + 24 开始
    let attr_data = &full_data[24..];
    
    // 7. 查找指定属性的数据
    // 属性以 4 字节 hash 开头，后面是属性数据
    let attr_bytes = locate_attr_in_data(attr_data, attr_name)
        .ok_or_else(|| format!("找不到属性: {}", attr_name))?;
    
    Ok(attr_bytes.to_vec())
}

/// 在属性数据中定位指定属性
///
/// 属性格式：
/// - 4 字节: 属性 hash
/// - 4 字节: 属性类型/标志
/// - 后续: 属性值数据
fn locate_attr_in_data(data: &[u8], attr_name: &str) -> Option<&[u8]> {
    use aios_core::tool::db_tool::{convert_to_hash, db1_dehash};
    
    let target_hash = convert_to_hash(attr_name);
    let mut pos = 0;
    
    while pos + 8 <= data.len() {
        let hash_bytes: [u8; 4] = data[pos..pos+4].try_into().unwrap();
        let hash_val = i32::from_be_bytes(hash_bytes);
        let unsigned_hash = hash_val.unsigned_abs() as u32;
        
        let name = db1_dehash(unsigned_hash);
        if name.eq_ignore_ascii_case(attr_name) {
            // 找到属性，计算其数据范围
            let dtype = i32::from_be_bytes(data[pos+4..pos+8].try_into().unwrap());
            
            // 根据类型确定数据长度
            let data_len = match dtype {
                0x65 | 0x66 | 0x67 | 0x68 | 0x69 | 0x6A | 0x6B | 0x6C | 0x6D | 0x6F | 0x70 | 0x71 | 0x72 | 0x74 | 0x75 | 0x76 => {
                    // 表达式类型，需要读取后续长度字段
                    let len_bytes: [u8; 4] = data[pos+8..pos+12].try_into().unwrap();
                    let len = i32::from_be_bytes(len_bytes) as usize;
                    12 + len * 4
                },
                0x66 => {
                    // 字符串类型
                    let len_bytes: [u8; 4] = data[pos+8..pos+12].try_into().unwrap();
                    let len = i32::from_be_bytes(len_bytes) as usize;
                    12 + len * 4
                },
                _ => 8, // 简单类型只有 hash + dtype
            };
            
            let end = pos + data_len;
            if end <= data.len() {
                return Some(&data[pos..end]);
            }
        }
        
        // 移动到下一个属性
        pos += 8;
    }
    
    None
}

/// 通过 pdms-io 测试表达式解析
pub async fn test_expression_via_pdmsio(case: &TestCase) -> Result<String, String> {
    // 提取属性字节数据
    let attr_bytes = extract_attr_bytes_via_pdmsio(
        &case.file_path,
        &case.refno,
        &case.attr,
    ).await?;
    
    // 构建完整的元素数据用于解析
    // 需要包含元素头部信息
    let refno = parse_refno(&case.refno)?;
    let db_idx = refno.get_0() as u32;
    let ele_idx = refno.get_1() as u32;
    
    // 构建元素数据 (简化版：直接使用属性数据)
    // 实际需要构建完整的元素结构
    let element_data = build_element_for_parse(&attr_bytes, db_idx, ele_idx);
    
    // 解析并验证
    let ele_data = parse_ele_data(&element_data).await
        .map_err(|e| format!("解析失败: {}", e))?;
    
    let map = &ele_data.whole_attmap.attmap;
    let result = map.get_as_string(case.attr).unwrap_or_default();
    
    if result.trim() == case.expected.trim() {
        Ok(result)
    } else {
        Err(format!(
            "表达式不匹配:\n  期望: '{}'\n  实际: '{}'",
            case.expected, result
        ))
    }
}

/// 构建用于解析的元素数据
fn build_element_for_parse(attr_bytes: &[u8], db_idx: u32, ele_idx: u32) -> Vec<u8> {
    let mut result = Vec::new();
    
    // 元素头部 (24 字节)
    // 格式参考 parse.rs 中的 ele_data 结构
    result.extend_from_slice(&[0u8; 4]); // flag
    result.extend_from_slice(&db_idx.to_be_bytes()); // dbidx
    result.extend_from_slice(&ele_idx.to_be_bytes()); // eleidx
    result.extend_from_slice(&[0u8; 4]); // ses_pgno
    result.extend_from_slice(&[0u8; 4]); // sesno
    result.extend_from_slice(&[0u8; 4]); // pre_pgno
    result.extend_from_slice(&[0u8; 4]); // pre_offset
    
    // impl_len (4字节) = (attr_bytes.len() - 8) / 4
    let impl_len = ((attr_bytes.len() as i32 - 8) / 4).max(1);
    result.extend_from_slice(&impl_len.to_be_bytes());
    
    // 属性数据
    result.extend_from_slice(attr_bytes);
    
    result
}

/// 直接从文件读取并解析元素
pub async fn test_expression_from_file(case: &TestCase) -> Result<String, String> {
    match File::open(&case.file_path) {
        Ok(mut file) => {
            let mut data = Vec::new();
            file.read_to_end(&mut data).map_err(|e| e.to_string())?;
            
            let ele_data = parse_ele_data(&data).await.map_err(|e| e.to_string())?;
            let map = &ele_data.whole_attmap.attmap;
            let result = map.get_as_string(case.attr).unwrap_or_default();
            
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

/// 批量测试运行器
pub async fn run_tests(cases: &[TestCase]) {
    for case in cases {
        match test_expression_from_file(case).await {
            Ok(result) => {
                println!("✓ {} {} => {}", case.refno, case.attr, result);
            }
            Err(e) => {
                println!("✗ {} {} => {}", case.refno, case.attr, e);
                panic!("测试失败");
            }
        }
    }
}

/// 调试模式：打印解析详情
pub async fn debug_expression(data_str: &str, attr: &str) {
    let data = convert_str_to_bytes(data_str);
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    let map = &ele_data.whole_attmap.attmap;
    
    println!("=== {} 解析结果 ===", attr);
    println!("原始值: {:?}", map.get(attr));
    println!("字符串: {:?}", map.get_as_string(attr));
    println!("完整属性: {:?}", map.iter().collect::<Vec<_>>());
}

/// 打印元素的二进制结构（用于调试）
pub async fn dump_element_structure(file_path: &str, refno_str: &str) -> Result<(), String> {
    let refno = parse_refno(refno_str)?;
    
    let mut io = PdmsIO::new(file_path.to_string(), None)
        .map_err(|e| format!("打开文件失败: {}", e))?;
    
    let (sesno, offset) = io.search_latest_refno(refno, None)
        .ok_or_else(|| format!("找不到 refno: {}", refno_str))?;
    
    println!("=== 元素结构 ===");
    println!("Refno: {}", refno_str);
    println!("Sesno: {}", sesno);
    println!("Offset: {:#X}", offset);
    
    // 读取头部
    let head = io.read_data_cached(offset, 24).map_err(|e| e.to_string())?;
    let header_start = if head[..4] == [0, 0, 0, 0x7] { &head[4..] } else { &head[..] };
    let impl_len = i32::from_be_bytes(header_start[20..24].try_into().unwrap());
    let total_len = 24 + impl_len as usize * 4;
    
    println!("ImplLen: {}", impl_len);
    println!("TotalLen: {}", total_len);
    
    // 读取完整数据
    let full_data = io.read_data_cached(offset, total_len).map_err(|e| e.to_string())?;
    let attr_data = &full_data[24..];
    
    println!("\n属性数据 (共 {} 字节):", attr_data.len());
    print_hex_dump(attr_data, 16);
    
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
    #[tokio::test]
    async fn test_phei_expression() {
        let data_str = "
00 00 00 21 00 00 3B 5A 00 00 01 7C 00 0F 56 3E
00 00 3B 5A 00 00 01 7B 00 00 0F 29 00 27 60 01
00 00 00 00 00 00 00 00 20 1B 00 00 00 00 00 01
00 00 00 04 00 00 00 28 00 00 00 07 00 00 03 9F
00 00 01 B9 00 00 00 04 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 04 00 00 00 00
00 00 00 01 00 00 00 00 00 00 00 00 00 00 00 02
00 00 00 02 00 00 00 03 00 00 00 00 00 00 00 00
00 00 00 00 00 01 00 71 00 00 3B 5A 00 00 01 7C
00 00 00 00 00 00 00 00 00 58 52 59 1C 00 00 03
00 00 00 02 00 00 00 01 00 00 00 02 06 76 B4 60
1C 00 00 05 00 00 00 04 00 00 00 28 00 00 00 01
FF FF F8 F8 00 00 00 00 00 09 6B 9B 1C 00 00 05
00 00 00 04 00 00 00 00 00 00 00 01 00 00 00 00
00 00 00 00 00 0A DF 11 1C 00 00 05 00 00 00 04
00 00 00 00 00 00 00 01 00 00 00 00 00 00 00 00
FF F2 51 1C 1C 00 00 22 00 00 00 21 00 00 00 21
00 00 00 01 00 00 00 65 00 00 00 06 00 00 40 00
00 00 00 00 00 00 00 02 00 00 00 00 00 00 00 06
00 00 00 6A 00 00 00 02 00 0D DF 77 FF FF FF FF
FF FF FF FF 00 00 00 00 00 00 06 41 00 00 06 A5
00 00 00 65 00 00 00 06 00 00 60 00 00 00 00 00
00 00 00 02 00 00 00 00 00 00 00 06 00 00 00 6A
00 00 00 02 00 0D DF 77 FF FF FF FF FF FF FF FF
00 00 00 00 00 00 06 41 00 00 06 A5 00 00 03 22
FF F6 94 65 1C 00 00 12 00 00 00 11 00 00 00 11
00 00 00 01 00 00 00 65 00 00 00 06 00 00 40 00
00 00 00 00 00 00 00 02 00 00 00 00 00 00 00 06
00 00 00 6A 00 00 00 02 00 0D DF 77 FF FF FF FF
FF FF FF FF 00 00 00 00 00 00 06 41 00 00 06 A5
FF F5 20 EF 1C 00 00 12 00 00 00 11 00 00 00 11
00 00 00 01 00 00 00 65 00 00 00 06 00 00 60 00
00 00 00 00 00 00 00 02 00 00 00 00 00 00 00 06
00 00 00 6A 00 00 00 02 00 0D DF 77 FF FF FF FF
FF FF FF FF 00 00 00 00 00 00 06 41 00 00 06 A5
00 09 C1 8E 3C 00 00 04 00 00 00 0A 2F 52 53 54
52 41 2D 50 41 31 00 00 
        ";
        
        let result = test_expression_hex(data_str, "PHEI").await;
        println!("PHEI 解析结果: '{}'", result);
    }

    /// 验证 PBOF 复杂表达式
    #[tokio::test]
    async fn test_pbof_expression() {
        let data_str = include_str!("test_parse_expr.rs")
            .lines()
            .skip(49)
            .take(235)
            .collect::<Vec<_>>()
            .join("\n");
        
        let result = test_expression_hex(&data_str, "PBOF").await;
        println!("PBOF 解析结果: {}", result);
    }
}
