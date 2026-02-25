//! AMS7330 解析测试案例
//!
//! 解析 ams7330_0001 测试文件，保存到 SurrealDB，并添加 profile 耗时分析

use aios_core::{SUL_DB, init_test_surreal, insert_into_table_with_chunks};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::time::Instant;

use crate::parse::parse_ele_data;

/// 解析后的元素数据，用于保存到数据库
#[derive(Debug, Serialize, Deserialize)]
struct ParsedElementRecord {
    /// 元素索引
    index: usize,
    /// 元素类型 (noun)
    noun: String,
    /// 参考号字符串
    refno: String,
    /// 属性数量
    attr_count: usize,
    /// 显式属性数量
    explicit_attr_count: usize,
    /// 解析耗时（微秒）
    parse_time_us: u64,
}

/// Profile 统计结构
#[derive(Debug, Default)]
struct ProfileStats {
    file_read_ms: f64,
    parse_ms: f64,
    db_save_ms: f64,
    total_ms: f64,
    element_count: usize,
    total_attr_count: usize,
}

impl std::fmt::Display for ProfileStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\n╔════════════════════════════════════════╗")?;
        writeln!(f, "║          Profile 耗时分析报告           ║")?;
        writeln!(f, "╠════════════════════════════════════════╣")?;
        writeln!(
            f,
            "║ 📁 文件读取耗时:    {:>10.2} ms      ║",
            self.file_read_ms
        )?;
        writeln!(f, "║ 🔍 解析处理耗时:    {:>10.2} ms      ║", self.parse_ms)?;
        writeln!(
            f,
            "║ 💾 数据库保存耗时:  {:>10.2} ms      ║",
            self.db_save_ms
        )?;
        writeln!(f, "╠════════════════════════════════════════╣")?;
        writeln!(f, "║ ⏱️  总耗时:         {:>10.2} ms      ║", self.total_ms)?;
        writeln!(f, "╠════════════════════════════════════════╣")?;
        writeln!(
            f,
            "║ 📊 解析元素数量:    {:>10}          ║",
            self.element_count
        )?;
        writeln!(
            f,
            "║ 📋 总属性数量:      {:>10}          ║",
            self.total_attr_count
        )?;
        if self.element_count > 0 {
            writeln!(
                f,
                "║ ⚡ 平均解析速度:    {:>10.2} 元素/秒  ║",
                self.element_count as f64 / (self.parse_ms / 1000.0)
            )?;
        }
        writeln!(f, "╚════════════════════════════════════════╝")?;
        Ok(())
    }
}

/// 测试解析 ams7330_0001 文件
///
/// 功能：
/// 1. 读取 test-files/ams7330_0001 二进制文件
/// 2. 使用 parse_ele_data 解析
/// 3. 保存解析结果到 SurrealDB (parsed_element 表)
/// 4. 输出各阶段耗时 profile
#[tokio::test]
async fn test_parse_ams7330_0001() {
    let total_start = Instant::now();
    let mut stats = ProfileStats::default();

    // ============ 1. 初始化数据库连接 ============
    println!("\n🚀 初始化数据库连接...");
    if let Err(e) = init_test_surreal().await {
        eprintln!("❌ 数据库连接失败: {:?}", e);
        return;
    }
    println!("✅ 数据库连接成功");

    // ============ 2. 读取测试文件 ============
    let file_start = Instant::now();
    let test_file_path = "test-files/ams7330_0001";
    println!("\n📁 读取测试文件: {}", test_file_path);

    let mut file = match File::open(test_file_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("❌ 无法打开文件 {}: {:?}", test_file_path, e);
            return;
        }
    };

    let mut buffer = Vec::new();
    if let Err(e) = file.read_to_end(&mut buffer) {
        eprintln!("❌ 读取文件失败: {:?}", e);
        return;
    }

    stats.file_read_ms = file_start.elapsed().as_secs_f64() * 1000.0;
    println!("✅ 文件读取完成，大小: {} bytes", buffer.len());

    // ============ 3. 解析文件内容 ============
    let parse_start = Instant::now();
    println!("\n🔍 开始解析文件内容...");

    let mut parsed_records: Vec<ParsedElementRecord> = Vec::new();
    let offset = 0;
    let mut element_index = 0;

    // 逐个解析元素
    while offset < buffer.len() {
        let element_start = Instant::now();

        // 尝试解析一个元素
        match parse_ele_data(&buffer[offset..]).await {
            Ok(ele_data) => {
                let element_parse_time = element_start.elapsed().as_micros() as u64;

                // 获取属性信息
                let attr_count = ele_data.whole_attmap.attmap.len();
                let explicit_attr_count = ele_data.whole_attmap.explicit_attmap.len();
                let noun = ele_data
                    .whole_attmap
                    .attmap
                    .get_as_string("TYPE")
                    .unwrap_or_else(|| "UNKNOWN".to_string());
                let refno = ele_data.refno.to_string();

                stats.total_attr_count += attr_count + explicit_attr_count;

                // 创建记录
                let record = ParsedElementRecord {
                    index: element_index,
                    noun,
                    refno,
                    attr_count,
                    explicit_attr_count,
                    parse_time_us: element_parse_time,
                };

                if element_index < 10 || element_index % 100 == 0 {
                    println!(
                        "  📌 元素 #{}: {} (属性: {}, 显式属性: {})",
                        element_index, record.refno, attr_count, explicit_attr_count
                    );
                }

                parsed_records.push(record);
                element_index += 1;

                // 根据解析消耗的数据量更新 offset
                // 注意：这里假设整个文件是单个元素，如果是多个元素需要调整
                break; // 先只解析第一个元素，后续可以扩展
            }
            Err(e) => {
                eprintln!(
                    "⚠️ 解析元素 #{} 时出错 (offset={}): {:?}",
                    element_index, offset, e
                );
                break;
            }
        }
    }

    stats.parse_ms = parse_start.elapsed().as_secs_f64() * 1000.0;
    stats.element_count = parsed_records.len();
    println!("✅ 解析完成，共解析 {} 个元素", stats.element_count);

    // ============ 4. 保存到数据库 ============
    let db_start = Instant::now();
    println!("\n💾 保存解析结果到数据库...");

    if !parsed_records.is_empty() {
        match insert_into_table_with_chunks(&SUL_DB, "parsed_element", parsed_records).await {
            Ok(_) => println!("✅ 数据保存成功"),
            Err(e) => eprintln!("❌ 数据保存失败: {:?}", e),
        }
    }

    stats.db_save_ms = db_start.elapsed().as_secs_f64() * 1000.0;
    stats.total_ms = total_start.elapsed().as_secs_f64() * 1000.0;

    // ============ 5. 输出 Profile 报告 ============
    println!("{}", stats);
}

/// 测试解析 ams7330_0001 文件并打印详细属性
#[tokio::test]
async fn test_parse_ams7330_0001_detail() {
    // 初始化数据库连接
    if let Err(e) = init_test_surreal().await {
        eprintln!("❌ 数据库连接失败: {:?}", e);
        return;
    }

    // 读取测试文件
    let test_file_path = "test-files/ams7330_0001";
    let mut file = match File::open(test_file_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("❌ 无法打开文件 {}: {:?}", test_file_path, e);
            return;
        }
    };

    let mut buffer = Vec::new();
    if let Err(e) = file.read_to_end(&mut buffer) {
        eprintln!("❌ 读取文件失败: {:?}", e);
        return;
    }

    println!("📁 文件大小: {} bytes", buffer.len());

    // 解析文件
    match parse_ele_data(&buffer).await {
        Ok(ele_data) => {
            println!("\n✅ 解析成功!");
            println!("📌 参考号: {}", ele_data.refno.to_string());

            // 打印普通属性
            println!("\n📋 普通属性 ({} 个):", ele_data.whole_attmap.attmap.len());
            for (key, value) in ele_data.whole_attmap.attmap.iter() {
                println!("  {} = {:?}", key, value);
            }

            // 打印显式属性
            println!(
                "\n📋 显式属性 ({} 个):",
                ele_data.whole_attmap.explicit_attmap.len()
            );
            for (key, value) in ele_data.whole_attmap.explicit_attmap.iter() {
                println!("  {} = {:?}", key, value);
            }
        }
        Err(e) => {
            eprintln!("❌ 解析失败: {:?}", e);
        }
    }
}
