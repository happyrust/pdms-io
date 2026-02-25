//! 调试 SPVE 元素的 PX/PY 属性解析
//!
//! 用法: cargo run --example debug_spve_px_py
//!
//! 目的：
//! 1. 定位 refno 15193_14688 (SPVE 元素)
//! 2. 检查 PX/PY 属性是如何被解析的
//! 3. 验证是否正确解析为表达式字符串（如 "ATTRIB DESP[1 ]"）而非浮点数
//! 4. 导出原始字节数据用于分析

use aios_core::RefU64;
use anyhow::Result;
use pdms_io::io::PdmsIO;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let db_path = if args.len() > 1 {
        args[1].clone()
    } else if let Ok(path) = env::var("PDMS_ACP_FILE") {
        path
    } else {
        eprintln!("未提供 ACP 文件路径，请设置环境变量 PDMS_ACP_FILE 或传入参数");
        eprintln!("用法: cargo run --example debug_spve_px_py -- <acp_db_path>");
        return Ok(());
    };

    // 目标 SPVE 元素的 refno 列表
    let target_refnos = [
        (15193, 14687, "SPVE - PX=' 0', PY=' 0'"),
        (15193, 14688, "SPVE - PX='ATTRIB DESP[1 ]', PY=' 0'"),
        (
            15193,
            14689,
            "SPVE - PX='ATTRIB DESP[1 ]', PY='ATTRIB DESP[2 ]'",
        ),
        (15193, 14690, "SPVE - PX=' 0', PY='ATTRIB DESP[2 ]'"),
    ];

    println!("========================================");
    println!("调试 SPVE 元素 PX/PY 属性解析");
    println!("文件: {}", db_path);
    println!("========================================\n");

    // 打开数据库
    let mut io = PdmsIO::new("debug", &db_path, true);
    io.open()?;

    println!("数据库打开成功，dbnum = {}\n", io.dbnum);

    for (ref_0, ref_1, expected_desc) in target_refnos {
        let refno = RefU64::from_two_nums(ref_0, ref_1);

        println!("----------------------------------------");
        println!("定位 refno: {}_{} ({})", ref_0, ref_1, expected_desc);
        println!("----------------------------------------");

        // 方法1: 使用 auto_get_element 获取完整解析结果
        match io.auto_get_element(refno).await {
            Ok(ele_data) => {
                let att_map = ele_data.att_map();

                println!("  TYPE: {:?}", att_map.get_type());
                println!("  REFNO: {:?}", att_map.get_refno_or_default());
                println!(
                    "  OWNER: {:?}",
                    att_map.get_refno_by_att_or_default("OWNER")
                );
                println!("  SESNO: {:?}", att_map.sesno());

                // 关键：检查 PX/PY 的解析结果
                println!("\n  === PX/PY 属性解析结果 ===");

                // 尝试获取为字符串
                if let Some(px_str) = att_map.get_string("PX") {
                    println!("  PX (string): '{}'", px_str);
                } else {
                    println!("  PX (string): None");
                }

                if let Some(py_str) = att_map.get_string("PY") {
                    println!("  PY (string): '{}'", py_str);
                } else {
                    println!("  PY (string): None");
                }

                // 尝试获取为浮点数
                if let Some(px_f64) = att_map.get_f64("PX") {
                    println!("  PX (f64): {}", px_f64);
                } else {
                    println!("  PX (f64): None");
                }

                if let Some(py_f64) = att_map.get_f64("PY") {
                    println!("  PY (f64): {}", py_f64);
                } else {
                    println!("  PY (f64): None");
                }

                // 检查其他相关属性
                println!("\n  === 其他相关属性 ===");
                for attr_name in ["PRAD", "DRAD", "DX", "DY"] {
                    if let Some(val) = att_map.get_string(attr_name) {
                        println!("  {} (string): '{}'", attr_name, val);
                    }
                    if let Some(val) = att_map.get_f64(attr_name) {
                        println!("  {} (f64): {}", attr_name, val);
                    }
                }

                // 打印完整的属性映射（用于调试）
                println!("\n  === 完整属性映射 ===");
                for (key, value) in att_map.map.iter() {
                    println!("  {}: {:?}", key, value);
                }
            }
            Err(e) => {
                println!("  错误: 无法获取元素数据 - {}", e);
            }
        }

        // 方法2: 使用 parse_raw_element 获取原始解析结果（不处理 UDA）
        println!("\n  === 原始解析结果 (parse_raw_element) ===");
        match io.auto_get_raw_element(refno) {
            Ok(raw_ele) => {
                let raw_att = raw_ele.att_map();
                if let Some(px) = raw_att.get_string("PX") {
                    println!("  PX (raw): '{}'", px);
                }
                if let Some(py) = raw_att.get_string("PY") {
                    println!("  PY (raw): '{}'", py);
                }
            }
            Err(e) => {
                println!("  错误: 无法获取原始元素数据 - {}", e);
            }
        }

        // 方法3: 读取原始字节数据用于分析
        println!("\n  === 原始字节数据 ===");
        if let Some((_, offset)) = io.search_latest_refno(refno, None) {
            match io.read_data_cached(offset, 512) {
                Ok(data) => {
                    println!("  元素数据偏移: {:#X}", offset);
                    println!("  数据长度: {} 字节", data.len());
                    for (i, chunk) in data.chunks(16).enumerate() {
                        let hex: String = chunk.iter().map(|b| format!("{:02X} ", b)).collect();
                        let ascii: String = chunk
                            .iter()
                            .map(|&b| {
                                if b >= 0x20 && b < 0x7F {
                                    b as char
                                } else {
                                    '.'
                                }
                            })
                            .collect();
                        println!("  {:04X}: {} | {}", i * 16, hex, ascii);
                    }
                }
                Err(e) => {
                    println!("  错误: 无法读取原始数据 - {}", e);
                }
            }
        } else {
            println!("  无法定位元素");
        }

        println!();
    }

    println!("========================================");
    println!("调试完成");
    println!("========================================");

    Ok(())
}
