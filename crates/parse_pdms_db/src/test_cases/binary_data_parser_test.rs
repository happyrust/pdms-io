use crate::parse::{parse_ele_data, parse_raw_ele_data};
use crate::test_cases::convert_str_to_bytes;
use aios_core::{NamedAttrValue, get_default_pdms_db_info};

#[tokio::test]
async fn test_parse_binary_data_a5_35_30_hex_data() {
    let test_data = "\
00 00 00 21 00 00 44 AA 00 00 09 16
00 0B 0D 89 00 00 44 AA 00 00 09 08 00 00 01 94
00 3E 20 01 00 00 01 94 00 3D 40 01 20 02 C0 02
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 03
00 00 00 00 C0 96 08 00 00 00 00 00 C0 E0 F9 00
00 00 00 00 C0 96 A8 00 00 00 00 03 00 00 00 00
00 00 00 00 00 00 00 01 40 46 80 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 02 00 07 00 00 44 AA
00 00 09 16 00 00 00 00 00 00 00 00 00 00 44 AA
00 00 09 17 00 01 00 0F 00 00 44 AA 00 00 09 16
00 00 01 95 00 00 20 01 00 CC 6B 3F 38 00 00 02
00 00 00 01 00 0B 0D 89 00 09 C1 8E 3C 00 00 05
00 00 00 0F 2F 31 52 48 52 30 30 37 4D 54 2D 4B
00 00 00 07 00 01 00 07 00 00 44 AA 00 00 09 16
00 00 00 00 00 00 00 00 32 30 34 00 00 00 00 00";

    let data = convert_str_to_bytes(test_data);

    // 测试数据基本信息
    println!("Test data length: {} bytes", data.len());
    println!("First 16 bytes: {:02X?}", &data[0..16]);

    // 尝试解析数据
    match parse_raw_ele_data(&data) {
        Ok(ele_data) => {
            println!("Successfully parsed element data:");
            println!("  RefNo: {:?}", ele_data.refno);
            println!("  Owner: {:?}", ele_data.owner);
            println!("  Noun: 0x{:X}", ele_data.noun);
            println!("  Name: {}", ele_data.name);
            println!("  Children count: {}", ele_data.children.len());
            println!("  Attributes count: {}", ele_data.att_map().len());
        }
        Err(e) => {
            println!("Failed to parse element data: {:?}", e);

            // 尝试分析数据结构
            println!("Attempting to analyze data structure...");

            // 检查是否包含常见的标识符
            if data.len() >= 4 {
                let first_u32 = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                println!("First u32 (BE): 0x{:08X}", first_u32);

                let first_u32_le = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                println!("First u32 (LE): 0x{:08X}", first_u32_le);
            }

            // 查找可能的引用号模式 (44 AA)
            let pattern = [0x44, 0xAA];
            let mut positions = Vec::new();
            for i in 0..data.len().saturating_sub(1) {
                if data[i..i + 2] == pattern {
                    positions.push(i);
                }
            }
            println!("Found 0x44AA pattern at positions: {:?}", positions);
        }
    }

    // 测试特定的数据段
    test_parse_specific_segments(&data);
}

fn test_parse_specific_segments(data: &[u8]) {
    println!("\n--- Testing specific data segments ---");

    // 测试从不同位置开始的解析
    let test_positions = [0, 16, 32, 48, 64];

    for &pos in &test_positions {
        if pos < data.len() {
            println!(
                "Testing from position {}: {:02X?}",
                pos,
                &data[pos..std::cmp::min(pos + 16, data.len())]
            );

            // 尝试解析为不同的数据类型
            if pos + 4 <= data.len() {
                let val_be =
                    u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
                let val_le =
                    u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
                println!("  As u32 BE: 0x{:08X} ({})", val_be, val_be);
                println!("  As u32 LE: 0x{:08X} ({})", val_le, val_le);
            }

            if pos + 8 <= data.len() {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&data[pos..pos + 8]);
                let val_be = u64::from_be_bytes(bytes);
                let val_le = u64::from_le_bytes(bytes);
                println!("  As u64 BE: 0x{:016X}", val_be);
                println!("  As u64 LE: 0x{:016X}", val_le);
            }
            println!();
        }
    }
}

#[tokio::test]
async fn test_new_case_00_00_00_2f() {
    let data_str = "
00 00 00 19 00 00 5C 98 00 19 37 BC 00 A6 DF DD 00 00 5C 98
00 19 37 BA 00 00 02 21 00 0E 80 01 00 00 00 00
00 00 00 00 20 0B 00 00 00 00 00 02 00 00 00 04
00 00 00 00 00 00 00 01 00 00 00 00 00 00 00 00
00 00 00 04 00 00 00 00 00 00 00 01 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 01 00 31 00 00 5C 98 00 19 37 BC 00 00 00 00
00 00 00 00 10 C6 95 E8 1C 00 00 0A 00 00 00 09
00 00 00 09 00 00 00 05 00 00 00 01 00 00 00 22
00 00 00 51 00 00 00 04 00 00 00 02 00 00 00 00
00 00 00 07 00 09 5A 34 1C 00 00 19 00 00 00 18
00 00 00 18 00 00 00 05 00 00 00 02 00 00 00 16
00 00 00 0B 00 00 00 11 00 00 00 01 00 00 00 65
00 00 00 06 00 10 00 00 00 00 00 00 40 00 04 01
00 00 00 00 00 00 00 00 00 00 00 6A 00 00 00 02
00 0D 20 C7 FF FF FF FF FF FF FF FF 00 00 00 00
00 00 06 41 00 00 06 A5 00 00 00 0F 00 00 00 3D
00 58 52 59 1C 00 00 03 00 00 00 02 00 00 00 01
00 00 00 02
";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    let mut result = "".to_string();
    let merged = ele_data.whole_attmap.merge();
    if let Some(r) = merged.get_val("PZAXI") {
        dbg!(r);
        match r {
            NamedAttrValue::StringType(v) => {
                result = v.to_string();
            }
            _ => {}
        }
    }
    // 您可以根据期望的结果调整断言
    // assert_eq!("期望值", result);
    dbg!(&merged);
}
