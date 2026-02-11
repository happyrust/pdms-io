//! 调试脚本：分析 TRIM(0.001)*LOWCASE(TRUE) 表达式的二进制解码
//!
//! 目标：分析元件库 15194_4553 中包含 TRIM/LOWCASE 的表达式
//!
//! 运行方式：
//! ```bash
//! cargo run --bin debug_trim_lowcase_expr -- --refno 15194/4553
//! ```

use parse_pdms_db::parser::attribute::expression_payload::{
    decode_expression_payload, scan_expression_payload_opcodes,
};
use parse_pdms_db::parser::attribute::opcode::OpcodeCategory;
use std::env;

/// 打印 opcode 信息
fn print_opcode_info(opcode: i32) {
    let category = OpcodeCategory::from(opcode);
    let name = match opcode {
        // 字符串函数
        1301 => "LENGTH",
        1302 => "REAL",
        1303 => "MATCH",
        1304 => "AFTER",
        1305 => "BEFORE",
        1306 => "STRING",
        1307 => "UPCASE",
        1308 => "LOWCASE",  // ⚠️ 关注点
        1309 => "SUBSTRING",
        1311 => "DEFINED",
        1312 => "UNDEFINED",
        1313 => "SIZE",
        1314 => "TRIM",     // ⚠️ 关注点
        1315 => "MATCHWILD",
        1316 => "WIDTH",
        1317 => "PART",
        1321 => "OCCURS",
        1322 => "REPLACE",
        // 算术运算
        801 => "NEGATE",
        802 => "PLUS",
        803 => "MINUS",
        804 => "MUL",       // ⚠️ 关注点
        805 => "DIV",
        // 值类型
        0x65 => "VALUE_EXPR",
        0x66 => "TEXT",
        0x67 => "BOOLEAN",
        0x6A => "ATTRIBUTE",
        // 其他
        _ => "UNKNOWN",
    };
    println!("  opcode={} (0x{:04X}) => {} [{:?}]", opcode, opcode, name, category);
}

/// 分析表达式 payload 的 opcode 序列
fn analyze_payload(payload: &[u8], label: &str) {
    println!("\n=== {} ===", label);
    println!("Payload 长度: {} 字节", payload.len());
    
    // 打印原始十六进制
    println!("\n原始数据 (hex):");
    for (i, chunk) in payload.chunks(16).enumerate() {
        print!("  {:04X}: ", i * 16);
        for byte in chunk {
            print!("{:02X} ", byte);
        }
        println!();
    }
    
    // 尝试解码表达式
    println!("\n解码结果:");
    match decode_expression_payload(payload) {
        Ok((consumed, expr)) => {
            println!("  ✅ 成功解码");
            println!("  消耗字节: {}", consumed);
            println!("  表达式: '{}'", expr);
        }
        Err(e) => {
            println!("  ❌ 解码失败: {:?}", e);
        }
    }
    
    // 扫描 opcode 分布
    println!("\nOpcode 扫描:");
    match scan_expression_payload_opcodes(payload) {
        Ok(report) => {
            println!("  start_words: {}", report.start_words);
            println!("  declared_words: {}", report.declared_words);
            println!("  consumed_bytes: {}", report.consumed_bytes);
            
            println!("\n  Opcode 频次:");
            for (opcode, count) in &report.opcode_counts {
                print!("    ");
                print_opcode_info(*opcode);
                println!("      出现次数: {}", count);
            }
            
            if !report.unknown_opcodes.is_empty() {
                println!("\n  ⚠️ 未知 Opcode:");
                for opcode in &report.unknown_opcodes {
                    print!("    ");
                    print_opcode_info(*opcode);
                }
            }
        }
        Err(e) => {
            println!("  扫描失败: {:?}", e);
        }
    }
}

/// 模拟包含 TRIM(0.001)*LOWCASE(TRUE) 的二进制 payload
/// 用于验证解码器行为
fn create_test_payload() -> Vec<u8> {
    // 构造后缀表达式: 0.001 TRIM TRUE LOWCASE MUL
    // 
    // 期望的 opcode 序列:
    // 1. 0x65 (数值) + 0.001 的编码
    // 2. 1314 (TRIM)
    // 3. 0x67 (布尔值) + 201 (true)
    // 4. 1308 (LOWCASE)
    // 5. 804 (MUL)
    
    let mut payload = Vec::new();
    
    // 表达式长度字段（word 数量，后续计算）
    let len_pos = payload.len();
    payload.extend_from_slice(&[0, 0, 0, 0]); // 占位
    
    // 简化的测试 payload - 仅用于验证解码器
    // 实际需要从 AMS 文件中提取真实数据
    
    payload
}

fn main() {
    println!("===========================================");
    println!("TRIM/LOWCASE 表达式二进制解码调试工具");
    println!("===========================================");
    
    let args: Vec<String> = env::args().collect();
    
    // 检查是否提供了 refno 参数
    let refno = args
        .iter()
        .position(|a| a == "--refno")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("15194/4553");
    
    println!("\n目标元件库: {}", refno);
    println!("问题表达式: (TRIM(0.001)*LOWCASE(TRUE))");
    
    println!("\n分析步骤:");
    println!("1. 需要从 AMS 文件中提取 {} 的二进制数据", refno);
    println!("2. 定位包含 TRIM/LOWCASE 的表达式属性字段");
    println!("3. 分析 opcode 序列是否正确");
    
    println!("\n关键 Opcode 参考:");
    println!("  TRIM    = 1314 (0x0522) - 字符串函数，1个参数");
    println!("  LOWCASE = 1308 (0x051C) - 字符串函数，1个参数");
    println!("  MUL     = 804  (0x0324) - 乘法运算，2个参数");
    println!("  VALUE   = 0x65 (101)    - 数值常量");
    println!("  BOOLEAN = 0x67 (103)    - 布尔值 (201=true, 202=false)");
    
    println!("\n语义问题分析:");
    println!("  ❌ TRIM(0.001) - TRIM 期望字符串，实际传入数值");
    println!("  ❌ LOWCASE(TRUE) - LOWCASE 期望字符串，实际传入布尔值");
    println!("  ❌ 字符串函数结果不能做乘法运算");
    
    println!("\n可能的解码错误场景:");
    println!("  1. opcode 被错误识别（如 0.001 被误解析为 TRIM 参数）");
    println!("  2. 后缀表达式栈操作顺序错误");
    println!("  3. 原始数据本身就是错误的");
    
    println!("\n下一步：需要提供 AMS 文件路径来提取真实二进制数据");
    println!("使用方式: cargo run --bin debug_trim_lowcase_expr -- --ams-file <路径> --refno 15194/4553");
}
