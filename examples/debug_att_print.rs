//! PDMS 元素调试工具
//!
//! 用法: cargo run --example debug_ams1112_272310 -- <db_path> <refno>
//! 示例: cargo run --example debug_ams1112_272310 -- "D:\AVEVA\Projects\...\ams1112_0001" "17496/272310"
//!

use anyhow::{Result, bail};
use pdms_io::io::PdmsIO;
use aios_core::RefU64;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!("用法: {} <db_path> <refno>", args[0]);
        println!("示例: {} \"D:\\AVEVA\\...\\ams1112_0001\" \"17496/272310\"", args[0]);
        bail!("参数不足");
    }

    let db_path = &args[1];
    let refno_str = &args[2];
    let refno: RefU64 = refno_str.as_str().into();

    println!("========================================");
    println!("PDMS 元素调试工具");
    println!("文件: {}", db_path);
    println!("RefNo 字符串: {}", refno_str);
    println!("RefNo 解析后: {}", refno);
    println!("RefNo 内部值: db_idx={}, ele_idx={}", refno.get_0(), refno.get_1());
    println!("========================================\n");

    let mut io = PdmsIO::new("ams", db_path, true);
    io.open()?;

    println!("数据库打开成功，dbnum = {}\n", io.dbnum);

    // 先获取数据库基本信息
    if let Ok(basic_info) = io.get_page_basic_info() {
        println!("=== 数据库基本信息 ===");
        println!("最新会话: sesno={}", basic_info.latest_ses_data.sesno);
        println!("索引根页: {:#X}", basic_info.latest_ses_data.index_root_pageno);
    }

    // 尝试定位元素
    println!("\n=== 尝试定位元素 {} ===", refno);
    if let Some((sesno, offset)) = io.search_latest_refno(refno, None) {
        println!("元素定位成功:");
        println!("  sesno: {}", sesno);
        println!("  offset: {:#X}", offset);
        println!("  page: {:#X}", offset / io.page_size as u64);

        // 获取完整解析结果
        println!("\n=== 完整解析结果 ===");
        match io.auto_get_element(refno).await {
            Ok(ele_data) => {
                let att_map = ele_data.att_map();
                println!("TYPE: {}", att_map.get_type());
                println!("REFNO: {}", att_map.get_refno_or_default());

                println!("\n=== 所有属性 ===");
                for (key, value) in att_map.map.iter() {
                    println!("  {}: {:?}", key, value);
                }
            }
            Err(e) => {
                println!("错误: 无法获取元素数据 - {}", e);
            }
        }
    } else {
        println!("无法定位元素 {}", refno);

        // 尝试获取一些已知存在的元素来验证数据库是否正常
        println!("\n=== 尝试其他已知 refno ===");
        let test_refnos = [
            "17496/171603",
            "17496/171606",
            "17496/269393",
            "17496/497129",
        ];

        for test_refno_str in test_refnos {
            let test_refno: RefU64 = test_refno_str.into();
            if let Some((sesno, offset)) = io.search_latest_refno(test_refno, None) {
                println!("  {} => 存在 (sesno={}, offset={:#X})", test_refno_str, sesno, offset);
            } else {
                println!("  {} => 不存在", test_refno_str);
            }
        }
    }

    println!("\n========================================");
    println!("调试完成");
    println!("========================================");

    Ok(())
}
