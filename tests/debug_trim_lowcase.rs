//! 调试测试：分析 15194/4553 元件库中的 TRIM/LOWCASE 表达式问题
//!
//! 运行方式：
//! ```bash
//! cargo test --test debug_trim_lowcase -- --nocapture
//! ```

use aios_core::RefU64;
use parse_pdms_db::parse::parse_ele_data;
use pdms_io::io::PdmsIO;

fn parse_refno(refno_str: &str) -> Result<RefU64, String> {
    let parts: Vec<&str> = refno_str.split('/').collect();
    if parts.len() != 2 {
        return Err(format!("无效的 refno 格式: {}", refno_str));
    }
    let db_idx: u32 = parts[0].parse().map_err(|_| "无法解析 db_idx")?;
    let ele_idx: u32 = parts[1].parse().map_err(|_| "无法解析 ele_idx")?;
    Ok(RefU64::from_two_nums(db_idx, ele_idx))
}

#[tokio::test]
async fn analyze_scom_15194_4553() {
    println!("\n=== 分析元件库 15194/4553 ===");
    println!("问题表达式: (TRIM(0.001)*LOWCASE(TRUE))");

    let ams_path = "test-file/acp7002_0001";
    let refno_str = "15194/4553";

    // 检查文件是否存在
    if !std::path::Path::new(ams_path).exists() {
        panic!("AMS 文件不存在: {}", ams_path);
    }

    let refno = parse_refno(refno_str).expect("解析 refno 失败");

    // 打开 PDMS 文件
    let mut io = PdmsIO::new(ams_path.to_string(), ams_path, false);
    io.open().expect("打开文件失败");

    // 定位 refno
    let (sesno, offset) = io
        .search_latest_refno(refno, None)
        .expect(&format!("找不到 refno: {}", refno_str));

    println!("Sesno: {}, Offset: {:#X}", sesno, offset);

    // 读取元素数据
    let record = io
        .read_element_record_cached(offset)
        .expect("读取元素 record 失败");

    let mut input = record.as_slice();
    while input.len() >= 4 && (input[..4] == [0, 0, 0, 0] || input[..4] == [0, 0, 0, 7]) {
        input = &input[4..];
    }

    println!("Record 长度: {} 字节", input.len());

    // 解析元素数据
    let ele_data = parse_ele_data(input).await.expect("解析元素数据失败");

    let map = &ele_data.whole_attmap.attmap;

    // 打印元素类型
    let noun = map
        .get_as_string("TYPE")
        .unwrap_or_else(|| "未知".to_string());
    println!("元素类型 (NOUN): {}", noun);

    // 搜索包含 TRIM 或 LOWCASE 的属性
    println!("\n=== 搜索 TRIM/LOWCASE 表达式 ===");
    let mut found_count = 0;

    for (key, val) in map.iter() {
        let val_str = format!("{:?}", val);
        let val_upper = val_str.to_uppercase();

        if val_upper.contains("TRIM") || val_upper.contains("LOWCASE") {
            found_count += 1;
            println!("\n⚠️ 发现目标表达式!");
            println!("  属性: {}", key);
            println!("  值: {:?}", val);
        }
    }

    if found_count == 0 {
        println!("未在该元素中直接找到 TRIM/LOWCASE 表达式");
        println!("表达式可能来自子元素（几何体）");
    }

    // 打印所有表达式类属性
    println!("\n=== 表达式类属性 ===");
    let expr_attrs = [
        "PRAD", "PHEI", "PWID", "PANG", "PDIS", "PDIA", "POFF", "DRAD", "DHEI", "DWID", "DANG",
        "DDIS", "DDIA", "PTCD", "XLEN", "YLEN", "ZLEN", "BORE", "OBORE", "IBORE", "FRAD",
    ];

    for attr in &expr_attrs {
        if let Some(val) = map.get_as_string(attr) {
            let val = val.trim();
            if !val.is_empty() && val != "UNSET" && val != "0" && val != "0.0" {
                println!("  {} = '{}'", attr, val);
            }
        }
    }

    // 打印所有非空属性供参考
    println!("\n=== 所有非空属性 (部分) ===");
    let mut count = 0;
    for (key, val) in map.iter() {
        let val_str = format!("{:?}", val);
        // 过滤掉太长或默认值的属性
        if val_str.len() < 100
            && !val_str.contains("0.0")
            && val_str != "\"UNSET\""
            && val_str != "\"0\""
            && !val_str.is_empty()
        {
            println!("  {} = {:?}", key, val);
            count += 1;
            if count > 30 {
                println!("  ... (更多属性省略)");
                break;
            }
        }
    }

    println!("\n=== 分析完成 ===");
}

#[tokio::test]
async fn analyze_gmse_children() {
    println!("\n=== 分析 GMSE 及其子元素 ===");

    let ams_path = "test-file/acp7002_0001";

    let mut io = PdmsIO::new(ams_path.to_string(), ams_path, false);
    io.open().expect("打开文件失败");

    // GMRE = 15194/4544 是几何体集合的入口
    let gmse_refno = parse_refno("15194/4544").unwrap();

    let (_, offset) = io
        .search_latest_refno(gmse_refno, None)
        .expect("找不到 GMSE");
    let record = io.read_element_record_cached(offset).expect("读取失败");
    let mut input = record.as_slice();
    while input.len() >= 4 && (input[..4] == [0, 0, 0, 0] || input[..4] == [0, 0, 0, 7]) {
        input = &input[4..];
    }

    let ele_data = parse_ele_data(input).await.expect("解析失败");
    let map = &ele_data.whole_attmap.attmap;

    println!("GMSE 元素类型: {:?}", map.get_as_string("TYPE"));

    // 打印所有属性来了解结构
    println!("\nGMSE 所有属性:");
    for (key, val) in map.iter() {
        println!("  {} = {:?}", key, val);
    }

    // 递归遍历所有子孙元素
    async fn traverse_children(
        io: &mut PdmsIO,
        parent_refno: RefU64,
        depth: usize,
        max_depth: usize,
    ) {
        if depth > max_depth {
            return;
        }

        let indent = "  ".repeat(depth);

        if let Some((_, offset)) = io.search_latest_refno(parent_refno, None) {
            let record = io.read_element_record_cached(offset).expect("读取失败");
            let mut input = record.as_slice();
            while input.len() >= 4 && (input[..4] == [0, 0, 0, 0] || input[..4] == [0, 0, 0, 7]) {
                input = &input[4..];
            }

            if let Ok(ele_data) = parse_ele_data(input).await {
                let map = &ele_data.whole_attmap.attmap;
                let ele_type = map.get_as_string("TYPE").unwrap_or_default();

                // 搜索 TRIM/LOWCASE
                for (key, val) in map.iter() {
                    let val_str = format!("{:?}", val);
                    let val_upper = val_str.to_uppercase();
                    if val_upper.contains("TRIM") || val_upper.contains("LOWCASE") {
                        println!("\n{}⚠️ 发现! {} ({}):", indent, parent_refno, ele_type);
                        println!("{}   {} = {:?}", indent, key, val);
                    }
                }

                // 遍历子元素
                if let Some(fele_refno) = map.get_foreign_refno("FELE") {
                    let mut current: Option<aios_core::RefnoEnum> = Some(fele_refno);

                    while let Some(refno) = current {
                        if refno.is_none() {
                            break;
                        }
                        let refno_u64: RefU64 = refno.into();

                        // 递归
                        Box::pin(traverse_children(io, refno_u64, depth + 1, max_depth)).await;

                        // 获取下一个兄弟
                        if let Some((_, child_offset)) = io.search_latest_refno(refno_u64, None) {
                            let child_record = io
                                .read_element_record_cached(child_offset)
                                .unwrap_or_default();
                            let mut child_input = child_record.as_slice();
                            while child_input.len() >= 4
                                && (child_input[..4] == [0, 0, 0, 0]
                                    || child_input[..4] == [0, 0, 0, 7])
                            {
                                child_input = &child_input[4..];
                            }
                            if let Ok(child_data) = parse_ele_data(child_input).await {
                                current = child_data.whole_attmap.attmap.get_foreign_refno("NELE");
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }

    println!("\n=== 遍历 GMSE 及子元素 ===");
    Box::pin(traverse_children(&mut io, gmse_refno, 0, 5)).await;

    println!("\n=== 分析完成 ===");
}

#[tokio::test]
async fn check_ptre_dtre() {
    println!("\n=== 检查 PTRE/DTRE 及相关引用 ===");

    let ams_path = "test-file/acp7002_0001";
    let mut io = PdmsIO::new(ams_path.to_string(), ams_path, false);
    io.open().expect("打开文件失败");

    // 从 SCOM 获取的引用
    let refs_to_check = [
        ("PTRE", "15194/4534"),
        ("DTRE", "15194/13015"),
        ("GMRE", "15194/4544"),
    ];

    for (name, refno_str) in refs_to_check {
        println!("\n=== {} = {} ===", name, refno_str);

        let refno = parse_refno(refno_str).unwrap();
        if let Some((_, offset)) = io.search_latest_refno(refno, None) {
            let record = io.read_element_record_cached(offset).expect("读取失败");
            let mut input = record.as_slice();
            while input.len() >= 4 && (input[..4] == [0, 0, 0, 0] || input[..4] == [0, 0, 0, 7]) {
                input = &input[4..];
            }

            if let Ok(ele_data) = parse_ele_data(input).await {
                let map = &ele_data.whole_attmap.attmap;
                let ele_type = map.get_as_string("TYPE").unwrap_or_default();
                println!("类型: {}", ele_type);

                // 搜索 TRIM/LOWCASE
                for (key, val) in map.iter() {
                    let val_str = format!("{:?}", val);
                    let val_upper = val_str.to_uppercase();
                    if val_upper.contains("TRIM") || val_upper.contains("LOWCASE") {
                        println!("⚠️ 发现! {} = {:?}", key, val);
                    }
                }

                // 检查是否有子元素
                if let Some(fele) = map.get_foreign_refno("FELE") {
                    println!("FELE: {}", fele);

                    // 遍历子元素
                    let mut current: Option<aios_core::RefnoEnum> = Some(fele);
                    let mut count = 0;

                    while let Some(child_refno) = current {
                        if child_refno.is_none() || count >= 100 {
                            break;
                        }
                        count += 1;

                        let child_u64: RefU64 = child_refno.into();
                        if let Some((_, child_offset)) = io.search_latest_refno(child_u64, None) {
                            let child_record = io
                                .read_element_record_cached(child_offset)
                                .unwrap_or_default();
                            let mut child_input = child_record.as_slice();
                            while child_input.len() >= 4
                                && (child_input[..4] == [0, 0, 0, 0]
                                    || child_input[..4] == [0, 0, 0, 7])
                            {
                                child_input = &child_input[4..];
                            }

                            if let Ok(child_data) = parse_ele_data(child_input).await {
                                let child_map = &child_data.whole_attmap.attmap;
                                let child_type =
                                    child_map.get_as_string("TYPE").unwrap_or_default();

                                // 搜索 TRIM/LOWCASE
                                for (key, val) in child_map.iter() {
                                    let val_str = format!("{:?}", val);
                                    let val_upper = val_str.to_uppercase();
                                    if val_upper.contains("TRIM") || val_upper.contains("LOWCASE") {
                                        println!(
                                            "\n⚠️ 发现目标! 元素 {} ({}):",
                                            child_refno, child_type
                                        );
                                        println!("   {} = {:?}", key, val);
                                    }
                                }

                                current = child_map.get_foreign_refno("NELE");
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    println!("遍历了 {} 个子元素", count);
                }
            }
        } else {
            println!("未找到");
        }
    }
}

#[tokio::test]
async fn search_trim_lowcase_in_db() {
    println!("\n=== 在整个数据库中搜索 TRIM/LOWCASE ===");

    let ams_path = "test-file/acp7002_0001";

    if !std::path::Path::new(ams_path).exists() {
        panic!("AMS 文件不存在: {}", ams_path);
    }

    let mut io = PdmsIO::new(ams_path.to_string(), ams_path, false);
    io.open().expect("打开文件失败");

    // 获取数据库中的所有元素
    // 注意：这可能需要遍历整个数据库，可能比较慢
    println!("正在扫描数据库...");

    // 尝试直接解析 15194/4553 的子元素
    // 元件库通常包含多个几何体 (GM) 子元素

    // 先检查 GMSE（正向几何体集合）相关数据
    let refno = parse_refno("15194/4553").unwrap();
    let (_, offset) = io.search_latest_refno(refno, None).expect("找不到元素");

    let record = io.read_element_record_cached(offset).expect("读取失败");
    let mut input = record.as_slice();
    while input.len() >= 4 && (input[..4] == [0, 0, 0, 0] || input[..4] == [0, 0, 0, 7]) {
        input = &input[4..];
    }

    let ele_data = parse_ele_data(input).await.expect("解析失败");
    let map = &ele_data.whole_attmap.attmap;

    // 检查 GMSE 引用
    if let Some(gmse) = map.get_as_string("GMSE") {
        println!("GMSE 引用: {}", gmse);
    }

    // 检查 NGMR 引用
    if let Some(ngmr) = map.get_as_string("NGMR") {
        println!("NGMR 引用: {}", ngmr);
    }

    println!("\n提示: TRIM/LOWCASE 表达式可能在子元素（几何体）中");
    println!("需要遍历 GMSE 下的几何体来定位具体来源");
}
