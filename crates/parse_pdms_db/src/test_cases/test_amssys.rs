use std::path::PathBuf;

use crate::parse::parse_file;
use aios_core::types::RefU64;

/// 测试 amssys 文件的 DB 元素解析，特别关注 STYP 属性
#[tokio::test]
async fn test_amssys_db_styp_parsing() -> anyhow::Result<()> {
    let path = PathBuf::from("test-files/amssys");
    let file_name = "amssys";
    let project = "ams";

    let pdms = parse_file(&path, &None, file_name, project).await?;

    println!("\n=== amssys DB 元素 STYP 解析测试 ===");
    println!("总元素数量: {}", pdms.total_attr_map.len());

    // 查找所有 DB 类型的元素
    let mut db_elements = vec![];
    for entry in pdms.total_attr_map.iter() {
        let attr_map = entry.value();
        if let Some(type_name) = attr_map.get_as_string("TYPE") {
            if type_name == "DB" {
                db_elements.push((*entry.key(), attr_map.clone()));
            }
        }
    }

    println!("找到 {} 个 DB 类型元素\n", db_elements.len());

    // 打印每个 DB 元素的 STYP 信息
    for (refno, attr_map) in &db_elements {
        println!("--- DB 元素 Refno: {} ---", refno);

        // 打印关键属性
        if let Some(name) = attr_map.get_as_string("NAME") {
            println!("  NAME: {}", name);
        }
        if let Some(dbnum) = attr_map.get_val("DBNO") {
            println!("  DBNO: {:?}", dbnum);
        }
        if let Some(desc) = attr_map.get_as_string("DESC") {
            println!("  DESC: {}", desc);
        }

        // 重点检查 STYP
        if let Some(styp) = attr_map.get_val("STYP") {
            println!("  STYP (raw): {:?}", styp);
            println!(
                "  STYP (string): '{}'",
                attr_map.get_as_string("STYP").unwrap_or_default()
            );
        } else {
            println!("  STYP: 不存在！");
        }

        // 打印 AREA, CRCY, PROJ 等相邻属性以便对比
        if let Some(area) = attr_map.get_val("AREA") {
            println!("  AREA: {:?}", area);
        }
        if let Some(crcy) = attr_map.get_val("CRCY") {
            println!("  CRCY: {:?}", crcy);
        }
        if let Some(proj) = attr_map.get_val("PROJ") {
            println!("  PROJ: {:?}", proj);
        }
        println!();
    }

    // 断言：至少有 DB 元素存在
    assert!(!db_elements.is_empty(), "应该能找到至少一个 DB 元素");

    Ok(())
}

/// 测试 amssys 特定 refno 的 DB 元素
#[tokio::test]
async fn test_amssys_specific_db_element() -> anyhow::Result<()> {
    let path = PathBuf::from("test-files/amssys");
    let file_name = "amssys";
    let project = "ams";

    let pdms = parse_file(&path, &None, file_name, project).await?;

    // 用户提供的数据: db:⟨24575_2195⟩
    let target_refno = RefU64::from_two_nums(24575, 2195);

    println!("\n=== 检查特定 DB 元素 {} ===", target_refno);

    if let Some(attr_map) = pdms.total_attr_map.get(&target_refno) {
        println!("\n找到元素 {}，属性列表：", target_refno);

        for (key, value) in attr_map.iter() {
            println!("  {} = {:?}", key, value);
        }

        // 详细检查 STYP
        println!("\n=== STYP 详细分析 ===");
        match attr_map.get_val("STYP") {
            Some(val) => {
                println!("STYP 原始值: {:?}", val);
                println!(
                    "STYP 字符串值: '{}'",
                    attr_map.get_as_string("STYP").unwrap_or_default()
                );
            }
            None => {
                println!("STYP 属性在 attr_map 中不存在！");
                println!("这可能是因为:");
                println!("  1. 隐式属性解析时 offset 计算错误");
                println!("  2. STYP (STRING类型) 的隐式解析逻辑有问题");
                println!("  3. DB 元素的隐式数据区域不包含 STYP 数据");
            }
        }
    } else {
        println!("未找到 refno {} 的元素", target_refno);

        // 尝试查找所有 DBNO=1112 的 DB 元素
        println!("\n尝试查找 DBNO=1112 的 DB 元素...");
        for entry in pdms.total_attr_map.iter() {
            let attr_map = entry.value();
            if let Some(type_name) = attr_map.get_as_string("TYPE") {
                if type_name == "DB" {
                    if let Some(dbnum) = attr_map.get_u32("DBNO") {
                        if dbnum == 1112 {
                            println!("\n找到 DBNO=1112 的 DB 元素:");
                            println!("  Refno: {}", entry.key());
                            for (key, value) in attr_map.iter() {
                                println!("    {} = {:?}", key, value);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// 调试测试：打印 DB 类型元素的隐式数据
#[tokio::test]
async fn test_amssys_db_implicit_data_debug() -> anyhow::Result<()> {
    use std::fs::File;
    use std::io::Read;

    let path = PathBuf::from("test-files/amssys");

    let mut file = File::open(&path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    println!("\n=== amssys 文件基本信息 ===");
    println!("文件大小: {} bytes", buf.len());

    // 检查文件头
    if buf.len() >= 60 {
        let db_basic_info = crate::parse::parse_file_basic_info(&buf[..60]);
        println!("数据库类型: {}", db_basic_info.db_type);
        println!("数据库编号: {}", db_basic_info.dbnum);
        println!("SES PGNO: {}", db_basic_info.ses_pgno);
    }

    Ok(())
}

/// 打印 DB 元素的十六进制隐式数据，分析 STYP
/// STYP 是数据库子类型枚举 (INTEGER 类型)，映射关系:
/// 1 => "DESI" (设计数据库)
/// 2 => "CATA" (目录数据库)
/// 4 => "PROP" (属性数据库)
/// 6 => "ISOD" (ISO图纸数据库)
/// 7 => "PADD" (填充数据库)
/// 8 => "DICT" (字典数据库)
/// 9 => "ENGI" (工程数据库)
/// 10 => "MANU" (制造数据库)
/// 14 => "SCHE" (示意图数据库)
#[tokio::test]
async fn test_amssys_db_hex_dump() -> anyhow::Result<()> {
    use aios_core::helper::parse_to_i32;
    use aios_core::tool::db_tool::db1_dehash;
    use std::fs::File;
    use std::io::Read;

    let path = PathBuf::from("test-files/amssys");

    let mut file = File::open(&path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    // 解析基本数据获取 refno 位置表
    let (refno_table_map, _) = crate::parse::gen_ref_type_pos_table(&buf);

    // 目标 DB 元素
    let target_refno = RefU64::from_two_nums(24575, 2195);

    println!("\n=== DB 元素 {} 十六进制分析 ===", target_refno);

    if let Some(pos_info) = refno_table_map.get(&target_refno) {
        let pos = pos_info.pos;
        println!("元素数据位置: 0x{:X} ({})", pos, pos);

        // 元素数据从 pos-4 开始（包含 impl_len）
        let data_start = pos - 4;
        if data_start + 100 <= buf.len() {
            let data = &buf[data_start..data_start + 100];

            // 打印前 100 字节的十六进制
            println!("\n元素原始数据 (前 100 字节):");
            for (i, chunk) in data.chunks(16).enumerate() {
                let hex: String = chunk
                    .iter()
                    .map(|b| format!("{:02X}", b))
                    .collect::<Vec<_>>()
                    .join(" ");
                let offset = i * 16;
                println!("  [{:3}] ({:2}w) {}", offset, offset / 4, hex);
            }

            // 解析关键位置
            println!("\n=== 属性偏移分析 ===");

            // impl_len at word 0
            let impl_len = parse_to_i32(&data[0..4]);
            println!("Word  0: impl_len = {} (0x{:08X})", impl_len, impl_len);

            // refno at word 1-2 (offset 4..12)
            let ref0 = parse_to_i32(&data[4..8]);
            let ref1 = parse_to_i32(&data[8..12]);
            println!(
                "Word 1-2: refno = {}_{} (0x{:08X} 0x{:08X})",
                ref0, ref1, ref0, ref1
            );

            // type_hash at word 3 (offset 12..16)
            let type_hash = parse_to_i32(&data[12..16]);
            let type_name = db1_dehash(type_hash as u32);
            println!(
                "Word  3: type_hash = {} => '{}' (0x{:08X})",
                type_hash, type_name, type_hash
            );

            // owner at word 4-5 (offset 16..24)
            let own0 = parse_to_i32(&data[16..20]);
            let own1 = parse_to_i32(&data[20..24]);
            println!(
                "Word 4-5: owner = {}_{} (0x{:08X} 0x{:08X})",
                own0, own1, own0, own1
            );

            // DBNO at offset 11 (word 11 = byte 44..48)
            if data.len() >= 48 {
                let dbnum = parse_to_i32(&data[44..48]);
                println!("Word 11: DBNO = {} (0x{:08X})", dbnum, dbnum);
            }

            // STYP at offset 12 (word 12 = byte 48..52) - 应该是 WORD 类型!
            if data.len() >= 52 {
                let styp_raw = parse_to_i32(&data[48..52]);
                let styp_word = if styp_raw >= 0x81BF1 {
                    db1_dehash(styp_raw as u32)
                } else {
                    // 可能是枚举值
                    match styp_raw {
                        1 => "DESI".to_string(),
                        2 => "CATA".to_string(),
                        4 => "PROP".to_string(),
                        6 => "ISOD".to_string(),
                        7 => "PADD".to_string(),
                        8 => "DICT".to_string(),
                        9 => "ENGI".to_string(),
                        10 => "MANU".to_string(),
                        14 => "SCHE".to_string(),
                        _ => format!("UNKNOWN({})", styp_raw),
                    }
                };
                println!(
                    "Word 12: STYP = {} => '{}' (0x{:08X})",
                    styp_raw, styp_word, styp_raw
                );
            }

            // FINO at offset 13 (word 13 = byte 52..56)
            if data.len() >= 56 {
                let fino = parse_to_i32(&data[52..56]);
                println!("Word 13: FINO = {} (0x{:08X})", fino, fino);
            }

            // AREA at offset 14 (word 14 = byte 56..60)
            if data.len() >= 60 {
                let area = parse_to_i32(&data[56..60]);
                println!("Word 14: AREA = {} (0x{:08X})", area, area);
            }

            // CRCY at offset 15 (word 15 = byte 60..64)
            if data.len() >= 64 {
                let crcy = parse_to_i32(&data[60..64]);
                println!("Word 15: CRCY = {} (0x{:08X})", crcy, crcy);
            }

            // PROJ at offset 16 (word 16 = byte 64..68)
            if data.len() >= 68 {
                let proj = parse_to_i32(&data[64..68]);
                println!("Word 16: PROJ = {} (0x{:08X})", proj, proj);
            }
        }
    } else {
        println!("未找到 refno {} 的位置信息", target_refno);
    }

    Ok(())
}
