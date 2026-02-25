//! 测试 collect_explict_data 函数

use crate::parse::collect_explict_data;
use aios_core::RefU64;
use aios_core::helper::parse_to_u16;

/// 测试从测试用例中获取的实际数据（完整的第一个显式块）
#[test]
fn test_collect_explict_data_format() {
    // 这是从 ams1112_0001 中 refno 17496/171603 提取的完整第一个显式属性块
    // 需要完整的 476 字节数据才能正确解析
    // 为了测试，我们只需要验证头部检查逻辑

    // 创建一个足够大的模拟数据
    let mut explicit_block: Vec<u8> = vec![
        // 0x0000: flag=0x0001, count=119 (0x77)
        0x00, 0x01, 0x00, 0x77, // 0x0004: refno = 17496_171603
        0x00, 0x00, 0x44, 0x58, // refno_0 = 17496
        0x00, 0x02, 0x9E, 0x53, // refno_1 = 171603
    ];

    // 填充足够的数据以满足 declared_bytes = 476
    while explicit_block.len() < 476 {
        explicit_block.push(0x00);
    }

    // 添加结束标记
    explicit_block.extend_from_slice(&[0x00, 0x00, 0x00, 0x07]);

    let refno = RefU64::from_two_nums(17496, 171603);

    println!("输入数据长度: {} 字节", explicit_block.len());
    println!("RefNo: {}", refno);

    // 解析前 4 个字节
    let flag = u16::from_be_bytes([explicit_block[0], explicit_block[1]]);
    let count = u16::from_be_bytes([explicit_block[2], explicit_block[3]]);
    println!("Flag: {:#06X}", flag);
    println!("Count: {} words ({} bytes)", count, count as usize * 4);

    // 解析 refno
    let block_refno = RefU64::from(&explicit_block[4..12]);
    println!("Block RefNo: {}", block_refno);
    println!("RefNo 匹配: {}", block_refno == refno);

    // 调用 collect_explict_data
    let result = collect_explict_data(&explicit_block, refno);

    println!("\ncollect_explict_data 返回 {} 字节", result.len());

    if result.is_empty() {
        println!("警告: 返回空数据！");

        // 调试分析
        let v = i32::from_be_bytes([
            explicit_block[0],
            explicit_block[1],
            explicit_block[2],
            explicit_block[3],
        ]);
        println!("\n调试分析:");
        println!("  parse_to_i32 第一个值: {} (0x{:08X})", v, v);
        println!("  这个值不是 0, 7, 5，所以进入 _ => 分支");

        // 模拟 collect_explict_data 中的检查
        let flag_u8 = (flag & 0xFF) as u8;
        println!("  flag as u8: {} (期望 1)", flag_u8);

        let len_words = count as usize;
        let declared_bytes = len_words * 4;
        println!("  len_words: {}", len_words);
        println!("  declared_bytes: {}", declared_bytes);
        println!("  declared_bytes < 12: {}", declared_bytes < 12);
        println!(
            "  declared_bytes > input.len(): {} ({} > {})",
            declared_bytes > explicit_block.len(),
            declared_bytes,
            explicit_block.len()
        );

        println!("  len_words < 5: {}", len_words < 5);
        println!("  maybe_refno != refno: {}", block_refno != refno);
    } else {
        println!("成功！收集到的数据前 128 字节:");
        for (i, chunk) in result.chunks(16).enumerate().take(8) {
            let hex: String = chunk.iter().map(|b| format!("{:02X} ", b)).collect();
            println!("  {:04X}: {}", i * 16, hex);
        }
    }
}

/// 测试 parse_to_u16 的行为
#[test]
fn test_parse_to_u16_behavior() {
    // 测试 big-endian 解析
    let data = [0x00u8, 0x01];
    let result = parse_to_u16(&data);
    println!("parse_to_u16([0x00, 0x01]) = {} (期望 1)", result);
    assert_eq!(result, 1);

    let data2 = [0x00u8, 0x77];
    let result2 = parse_to_u16(&data2);
    println!("parse_to_u16([0x00, 0x77]) = {} (期望 119)", result2);
    assert_eq!(result2, 119);
}

/// 测试两个连续显式属性块的收集（验证偏移计算）
#[test]
fn test_collect_two_consecutive_blocks() {
    // 模拟两个连续的显式属性块，它们之间没有 0x07 分隔符
    let refno = RefU64::from_two_nums(17496, 171603);

    // 块 #1: flag=0x0001, count=20 (80 bytes total = 4 + 76)
    // 但 count 字段表示的是数据部分大小还是总大小？
    // 根据分析，count 表示的是数据部分大小（不含 flag+len 的 4 字节）

    // 第一个块: count=20 表示 80 bytes 数据（包括 refno）
    let mut data: Vec<u8> = vec![
        0x00, 0x01, 0x00, 0x14, // flag=0x0001, count=20 (80 bytes)
        0x00, 0x00, 0x44, 0x58, // refno_0 = 17496
        0x00, 0x02, 0x9E, 0x53, // refno_1 = 171603
    ];
    // 填充到 4 + 80 = 84 字节
    while data.len() < 84 {
        data.push(0xAA);
    }

    // 第二个块: count=15 (60 bytes)，refno 相同
    data.extend_from_slice(&[
        0x00, 0x01, 0x00, 0x0F, // flag=0x0001, count=15 (60 bytes)
        0x00, 0x00, 0x44, 0x58, // refno_0 = 17496
        0x00, 0x02, 0x9E, 0x53, // refno_1 = 171603
    ]);
    // 填充到 84 + 4 + 60 = 148 字节
    while data.len() < 148 {
        data.push(0xBB);
    }

    // 添加结束标记
    data.extend_from_slice(&[0x00, 0x00, 0x00, 0x07]);

    println!("测试数据总长度: {} 字节", data.len());
    println!("块 #1: offset 0, count=20 (80 bytes), 总大小=84 bytes");
    println!("块 #2: offset 84, count=15 (60 bytes), 总大小=64 bytes");
    println!("预期收集数据: (80-8) + (60-8) = 72 + 52 = 124 bytes");
    println!("目标 RefNo: {}", refno);

    let result = collect_explict_data(&data, refno);
    println!("\ncollect_explict_data 返回 {} 字节", result.len());

    // 检查是否收集了两个块的数据
    if result.len() >= 72 {
        println!("成功收集了第一个块的数据");

        // 检查第一个块的数据 (应该是 0xAA 填充)
        let aa_count = result.iter().take(72).filter(|&&b| b == 0xAA).count();
        println!("第一块 0xAA 数量: {} (期望接近 72)", aa_count);

        if result.len() >= 124 {
            println!("成功收集了第二个块的数据");
            // 检查第二个块的数据 (应该是 0xBB 填充)
            let bb_count = result.iter().skip(72).filter(|&&b| b == 0xBB).count();
            println!("第二块 0xBB 数量: {} (期望接近 52)", bb_count);
        } else {
            println!("未能收集第二个块 - 可能存在偏移计算问题！");
        }
    } else {
        println!("未能收集完整数据！");
    }

    // 打印实际收集的数据
    println!("\n收集到的数据:");
    for (i, chunk) in result.chunks(16).enumerate().take(10) {
        let hex: String = chunk.iter().map(|b| format!("{:02X} ", b)).collect();
        println!("  {:04X}: {}", i * 16, hex);
    }
}
