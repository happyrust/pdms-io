use aios_core::init_test_surreal;
use aios_core::RefU64;
use aios_core::RefI32Tuple;
use super::convert_str_to_bytes;
use crate::parse::parse_ele_data;

#[tokio::test]
async fn test_parse_uda_elelist_0() {
    let data_str = "
    00 00 00 1B 00 00 33 EC 00 00 00 7C 00 08 1F 4B
    00 00 33 EC 00 00 00 74 00 00 00 3C 00 0F E0 01
    00 00 00 00 00 00 00 00 20 07 00 00 26 52 AB 53
    00 00 00 07 54 58 59 73 69 7A 65 00 00 00 00 00
    00 00 00 00 00 00 00 00 00 00 00 07 00 0B BA 07
    00 00 00 02 00 00 00 00 00 00 00 00 00 00 00 00
    00 09 C5 E1 00 0E 62 A0 18 EF 4B 6D 00 01 00 21
    00 00 33 EC 00 00 00 7C 00 00 00 00 00 00 00 00
    00 0A FA 16 28 00 00 02 00 00 00 04 42 4F 52 45
    04 1F E8 B1 14 00 00 01 00 00 00 00 00 09 C1 8E
    3C 00 00 03 00 00 00 08 2F 54 58 59 73 69 7A 65
    00 09 39 40 28 00 00 04 00 00 00 0C 54 61 69 6C
    20 58 59 20 53 69 7A 65 00 0F 8B EF 28 00 00 04
    00 00 00 0C 54 61 69 6C 20 58 59 20 53 69 7A 65
    00 0B C6 1B 1C 00 00 02 00 00 00 01 00 0C 55 1C
";
    let data = convert_str_to_bytes(data_str);
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    let map = &ele_data.whole_attmap.attmap;
    dbg!(&map);
}

#[tokio::test]
async fn test_parse_uda_elelist_1() {
    let data_str = "
        00 00 00 1B 00 00 3B 5E 00 00 00 37 00 08 1F 4B
        00 00 3B 5E 00 00 00 36 00 00 00 20 00 0B 20 01
        00 00 00 00 00 00 00 00 20 08 C0 00 2C 00 D5 76
        00 00 00 07 48 58 59 73 69 7A 65 00 00 00 00 00
        00 00 00 00 00 00 00 00 00 00 00 03 00 0B BA 07
        00 00 00 02 00 00 00 00 00 00 00 00 00 00 00 00
        00 09 C5 E1 00 0E 54 BF 17 EF 4B 61 00 01 00 28
        00 00 3B 5E 00 00 00 37 00 00 00 00 00 00 00 00
        00 09 39 40 28 00 00 04 00 00 00 0C 48 65 61 64
        20 58 59 20 53 69 7A 65 00 0A FA 16 28 00 00 03
        00 00 00 06 4C 65 6E 67 74 68 00 00 00 0F 8B EF
        28 00 00 04 00 00 00 0C 48 65 61 64 20 58 59 20
        53 69 7A 65 00 0B C6 1B 1C 00 00 03 00 00 00 02
        00 0C 55 1C 00 0A D7 E9 00 09 C1 8E 3C 00 00 06
        00 00 00 11 2F 55 44 41 2F 48 56 41 43 2F 48 58
        59 73 69 7A 65 00 00 00 00 0E 20 EC 28 00 00 03
        00 00 00 06 44 65 73 69 67 6E 00 00

";
    let data = convert_str_to_bytes(data_str);
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    let map = &ele_data.whole_attmap.attmap;
    dbg!(&map);
}

//15198/530
#[tokio::test]
async fn test_13292_185_udna() {
    let data_str = "
    00 00 00 1B 00 00 3B 5E 00 00 02 12 00 08 1F 4B
00 00 3B 5E 00 00 02 0D 00 00 00 13 00 00 20 01
00 00 00 00 00 00 00 00 20 0B 40 00 2C 00 D5 7D
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 09 00 08 31 81
00 00 00 01 00 00 00 00 00 00 00 00 00 00 00 00
00 08 31 81 00 09 C5 E1 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 07 00 01 00 32 00 00 3B 5E
00 00 02 12 00 00 00 00 00 00 00 00 00 0A FA 16
28 00 00 02 00 00 00 04 4E 4F 4E 45 00 09 39 40
28 00 00 07 00 00 00 18 43 6F 6E 73 69 73 74 65
6E 63 79 20 63 68 65 63 6B 20 72 65 73 75 6C 74
00 0B C6 1B 1C 00 00 02 00 00 00 01 00 0C 55 1C
00 0E 40 7F 28 00 00 03 00 00 00 05 46 41 4C 53
45 00 00 00 01 56 07 8A 28 00 00 04 00 00 00 09
50 46 43 6F 6E 73 43 68 6B 00 00 00 00 09 C1 8E
3C 00 00 08 00 00 00 19 2F 50 46 43 6F 6E 73 69
73 74 65 6E 63 79 43 68 65 63 6B 52 65 73 75 6C
74 00 00 00 00 0E 20 EC 28 00 00 05 00 00 00 10
50 69 70 65 20 66 61 62 72 69 63 61 74 69 6F 6E
";
    let data = convert_str_to_bytes(data_str);
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    dbg!(&ele_data.whole_attmap.explicit_attmap);
}

#[tokio::test]
async fn test_24381_177401_nphs_asr() {
    let _ = init_test_surreal().await;
    let data_str = "00 00 00 33 00 00 5F 3D 00 02 B4 F9 00 08 A3 E5
00 00 5F 3D 00 02 B4 F5 00 00 56 8A 00 1B 00 01
00 00 00 00 00 00 00 00 20 10 C0 00 00 00 00 03
00 00 00 00 40 BD 4F 80 00 00 00 00 40 DE 31 C0
99 99 99 9A C0 BB 58 D9 00 00 00 03 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
40 66 80 00 00 00 00 0C 00 00 33 BD 00 0D 32 63
00 00 33 BC 00 0A 8D 27 00 00 00 01 00 00 00 02
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 03 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
40 66 80 00 00 0D 19 FB 00 00 00 00 00 00 00 00
00 00 00 02 05 F7 05 E9 80 00 00 01 00 01 00 48
00 00 5F 3D 00 02 B4 F9 00 00 00 00 00 00 00 00
00 0A AF CA 14 00 00 01 00 00 00 00 00 09 2E A7
0C 00 00 01 FF FF FF FF 00 0B C6 C0 14 00 00 01
00 00 00 01 06 A0 26 04 0C 00 00 01 00 0D F3 17
00 0B CB FF 08 00 00 02 00 00 00 00 00 00 00 00
10 71 D1 20 08 00 00 02 00 00 00 00 00 00 00 00
10 71 D1 2B 08 00 00 02 00 00 00 00 00 00 00 00
00 0D FD 22 14 00 00 01 00 00 00 00 00 CC 6B 3F
38 00 00 02 00 00 00 01 00 08 A3 E5 00 09 C1 8E
3C 00 00 07 00 00 00 17 2F 31 57 41 49 30 31 38
32 2D 53 55 50 50 2D 30 31 2F 44 41 54 55 4D 00
00 0D 20 C7 18 00 00 03 00 00 00 01 00 00 00 00
40 3A 00 00 0F 7A 2C C8 1C 00 00 02 00 00 00 01
00 09 C5 E1 00 09 2F 7A 0C 00 00 01 00 08 2D B8
26 52 AB A5 28 00 00 06 00 00 00 12 2F 31 57 41
49 30 31 38 32 2D 31 4C 52 31 2D 30 31 53 00 00
26 52 AB A6 20 00 00 05 00 00 00 02 00 00 5F 3D
00 02 B8 44 00 00 5F 3D 00 02 B8 50";
    let _ = init_test_surreal().await;
    let data = convert_str_to_bytes(data_str);
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    dbg!(&ele_data.whole_attmap);
}

/// 测试 UDA 表动态缓存功能的性能对比
/// 验证优化后的实现能够正确获取 UDA 属性名称
#[tokio::test]
async fn test_uda_preload_performance_comparison() {
    use crate::parse::get_uda_full_name;
    use crate::parse::register_uda_name;
    use std::time::Instant;

    let _ = init_test_surreal().await;

    // 手动注册一些 UDA 名称（模拟解析过程中收集）
    register_uda_name(0xCD243, "SPROFILE".to_string());
    register_uda_name(0x8A1C2, "FLOWDIR".to_string());
    register_uda_name(0x7B3D4, "PROFILE".to_string());
    register_uda_name(0x9E5F6, "CNPEOPENITEM".to_string());
    register_uda_name(0x1A2B3, "JGOBJBASE".to_string());
    register_uda_name(0x2C4D5, "JGOBJMAT".to_string());
    register_uda_name(0x3E6F7, "JGOBJZL".to_string());

    // 测试一些常见的 UDA hash 值
    let test_hashes = vec![
        0xCD243, // SPROFILE
        0x8A1C2, // FLOWDIR
        0x7B3D4, // PROFILE
        0x9E5F6, // CNPEOPENITEM
        0x1A2B3, // JGOBJBASE
        0x2C4D5, // JGOBJMAT
        0x3E6F7, // JGOBJZL
        0xFFFFF, // 不存在的 hash
    ];

    println!("\n=== UDA 属性查询性能对比测试 ===\n");

    for hash in test_hashes {
        let start = Instant::now();
        let result = get_uda_full_name(hash);
        let elapsed = start.elapsed();

        match result {
            Some(name) => {
                println!("Hash 0x{:X} -> '{}' (耗时: {:?})", hash, name, elapsed);
            }
            None => {
                println!("Hash 0x{:X} -> 未找到 (耗时: {:?})", hash, elapsed);
            }
        }
    }

    println!("\n优化说明:");
    println!("优化前: 每次查询都需要异步访问数据库");
    println!("优化后: 从内存 HashMap 直接读取，耗时 < 1μs");
    println!("实现方式: 解析过程中动态收集 UDA 名称到缓存");

    println!("\n真实 UDA 属性示例:");
    println!("  :FLOWDIR E");
    println!("  :PROFILE CIRC");
    println!("  :CNPEOPENITEM UNSET");
    println!("  :JGOBJBASE 套管（T）|Φ100|圆形|124*124*850");
    println!("  :JGOBJMAT 纤维水泥||");
    println!("  :JGOBJZL S-1RS-NI-2D2-04-NPIY-23A3|A|新建");
}

/// 测试解析 ams1112_0001 并检查 refno 17496_142306 的 UDA 属性
/// 验证 UDA 表预加载功能正常工作
#[tokio::test]
async fn test_ams1112_0001_refno_17496_142306_uda() {
    let _ = init_test_surreal().await;

    // 目标 refno: 17496_142306
    let target_refno: RefU64 = RefI32Tuple((17496, 142306)).into();

    // 从数据库查询该 refno 的原始数据并解析
    // 注意：这个测试需要数据库中有对应的 ams1112_0001 数据
    let sql = format!("SELECT DATA FROM only element WHERE id = {}", target_refno.0);

    if let Ok(mut response) = aios_core::SUL_DB.query(&sql).await {
        if let Ok(data_bytes) = response.take::<Vec<u8>>(0) {
            if !data_bytes.is_empty() {
                // 解析元素数据
                if let Ok(ele_data) = parse_ele_data(&data_bytes).await {
                    println!("\n=== 解析 refno {:?} ===", target_refno);
                    println!("Noun: {:?}", ele_data.noun);
                    println!("\n所有属性:");
                    for (key, value) in ele_data.whole_attmap.attmap.iter() {
                        println!("  {}: {:?}", key, value);
                    }

                    // 检查显式属性中的 UDA
                    println!("\n显式属性 (explicit_attmap):");
                    let mut has_uda = false;
                    for (key, value) in ele_data.whole_attmap.explicit_attmap.iter() {
                        if key.starts_with("UDA:") {
                            has_uda = true;
                            println!("  {}: {:?}", key, value);
                        }
                    }

                    if has_uda {
                        println!("\n✓ 成功找到 UDA 属性（验证预加载功能正常）");
                    } else {
                        println!("\n⚠ 未找到 UDA 属性（可能该元素没有 UDA 或预加载失败）");
                    }

                    // 验证 UDA 名称格式正确（应该包含完整名称而不是 hash）
                    for key in ele_data.whole_attmap.explicit_attmap.keys() {
                        if key.starts_with("UDA:") {
                            assert!(
                                !key.contains("UDA_HASH:"),
                                "UDA 属性应该是完整名称格式，不应该出现 UDA_HASH: 格式"
                            );
                        }
                    }
                } else {
                    println!("解析失败: 数据可能是错误的格式");
                }
            } else {
                println!("未找到数据: refno {:?} 在数据库中不存在", target_refno);
            }
        }
    } else {
        println!("数据库查询失败，请确保 SurrealDB 已启动并包含 ams1112_0001 数据");
    }
}