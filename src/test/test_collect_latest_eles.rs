//! 测试最新元素收集功能
//!
//! 本测试模块验证`collect_latest_eles`方法是否能正确收集最新的元素数据：
//! - 能够从后往前检索最新数据
//! - 能够正确跳过已删除的元素
//! - 只保留增加和修改的元素
//! - 能够处理会话数量限制参数

use crate::defines::RefnoDataLoc;
use crate::io::PdmsIO;
use aios_core::pdms_types::RefU64;

/// 测试`collect_latest_eles`方法
///
/// 本测试验证以下情况：
/// 1. 使用None参数获取所有会话的最新元素
/// 2. 使用指定会话数量限制获取最新元素
/// 3. 验证返回的元素都是最新的且未被删除
/// 4. 验证性能和正确性
#[tokio::test]
async fn test_collect_latest_session() -> anyhow::Result<()> {
    // 设置数据库文件路径
    let db_filepath = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7999_0001"#;
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;
    io.init_ses_range_map()?;

    println!("开始测试 collect_latest_eles 方法");

    // 首先检查会话信息
    println!("\n检查会话信息:");
    let latest_sesno = io.get_latest_sesno()?;
    println!("最新会话号: {}", latest_sesno);

    // 测试前几个参考号的操作状态
    println!("\n测试前5个参考号的操作状态:");
    for (i, loc) in locs.iter().take(5).enumerate() {
        let refno = RefU64::from_two_nums(loc.refno_0, loc.refno_1);
        println!("参考号 {}: {}", i + 1, refno);

        // 首先检查搜索结果
        let [latest, previous] = io.search_latest_and_prev_refno(refno, Some(85));
        println!("  搜索结果: 最新版本={:?}, 前一版本={:?}", latest, previous);

        match io.get_refno_operation_status(refno, Some(85)) {
            Ok(status_map) => {
                if let Some(detail) = status_map.get(&refno) {
                    match detail {
                        EleOperationDetail::Add(_) => println!("  状态: 新增"),
                        EleOperationDetail::Modified(_) => println!("  状态: 修改"),
                        EleOperationDetail::Deleted => println!("  状态: 删除"),
                        EleOperationDetail::None => {
                            println!("  状态: 无操作");

                            // 分析为什么是无操作
                            if let Some((sesno, offset)) = latest {
                                println!("    分析: 找到最新版本在会话{}偏移{:#X}", sesno, offset);
                                match io.parse_raw_element(offset) {
                                    Ok(ele) => {
                                        println!(
                                            "    元素解析成功: 类型={}, 所有者={}",
                                            ele.att_map().get_type(),
                                            ele.owner
                                        );

                                        // 检查所有者元素
                                        println!("    尝试获取所有者元素: {}", ele.owner);

                                        // 先检查所有者元素是否存在于索引中
                                        let owner_search_result =
                                            io.search_latest_refno(ele.owner, None);
                                        println!(
                                            "    所有者元素搜索结果: {:?}",
                                            owner_search_result
                                        );

                                        match io.auto_get_raw_element(ele.owner) {
                                            Ok(owner_ele) => {
                                                println!(
                                                    "    所有者元素获取成功: 类型={}",
                                                    owner_ele.att_map().get_type()
                                                );
                                                if owner_ele.children.contains(&refno) {
                                                    println!("    ✓ 参考号在所有者的子元素列表中");
                                                } else {
                                                    println!(
                                                        "    ❌ 参考号不在所有者的子元素列表中"
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                println!("    ❌ 获取所有者元素失败: {}", e);

                                                // 进一步分析所有者元素为什么找不到
                                                println!("    分析所有者元素缺失原因:");

                                                // 检查所有者元素在所有会话中的历史
                                                match io.search_history_refnos(ele.owner, None) {
                                                    Ok(history) => {
                                                        if history.is_empty() {
                                                            println!("      所有者元素在任何会话中都不存在");
                                                        } else {
                                                            println!("      所有者元素历史记录: {} 个版本", history.len());
                                                            for (sesno, offset) in
                                                                history.iter().take(3)
                                                            {
                                                                println!(
                                                                    "        会话{}: 偏移{:#X}",
                                                                    sesno, offset
                                                                );
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        println!(
                                                            "      搜索所有者元素历史失败: {}",
                                                            e
                                                        );
                                                    }
                                                }

                                                // 检查所有者元素是否在当前会话范围内
                                                let owner_latest =
                                                    io.search_latest_refno(ele.owner, Some(85));
                                                println!(
                                                    "      所有者在会话85中的搜索结果: {:?}",
                                                    owner_latest
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        println!("    ❌ 元素解析失败: {}", e);
                                    }
                                }
                            } else {
                                println!("    分析: 未找到最新版本");
                            }
                        }
                    }
                } else {
                    println!("  状态: 未在状态映射中找到");
                }
            }
            Err(e) => {
                println!("  错误: {}", e);
            }
        }
    }

    // 测试用例1: 获取前几个会话的最新元素，看看是否有修改操作的元素
    println!("\n测试1: 获取前3个会话的最新元素");
    let start = Instant::now();
    let latest_eles = io.collect_latest_eles(Some(3))?;
    let elapsed = start.elapsed();

    println!(
        "前3个会话中共找到 {} 个最新元素, 耗时: {:?}",
        latest_eles.len(),
        elapsed
    );

    // 验证返回的元素都不是删除状态
    let mut add_count = 0;
    let mut modified_count = 0;
    let mut deleted_count = 0;
    let mut none_count = 0;

    for (refno, operation_data) in &latest_eles {
        match &operation_data.detail {
            EleOperationDetail::Add(_) => add_count += 1,
            EleOperationDetail::Modified(_) => modified_count += 1,
            EleOperationDetail::Deleted => {
                deleted_count += 1;
                println!("警告: 发现已删除元素 {}, 这不应该出现在结果中", refno);
            }
            EleOperationDetail::None => none_count += 1,
        }
    }

    println!(
        "操作类型统计: 新增={}, 修改={}, 删除={}, 无操作={}",
        add_count, modified_count, deleted_count, none_count
    );

    // 断言：结果中不应该有删除的元素
    assert_eq!(deleted_count, 0, "结果中不应该包含已删除的元素");

    Ok(())
}

/// 专门分析为什么参考号 24383_66457 找不到
#[tokio::test]
async fn test_analyze_missing_owner_24383_66457() -> anyhow::Result<()> {
    let db_filepath = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7999_0001"#;
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;
    io.init_ses_range_map()?;

    println!("=== 分析参考号 24383_66457 为什么找不到 ===");

    let missing_refno = RefU64::from_two_nums(24383, 66457);
    println!("目标参考号: {}", missing_refno);

    // 1. 检查在最新会话中的搜索
    println!("\n1. 在最新会话中搜索:");
    let latest_result = io.search_latest_refno(missing_refno, None);
    println!("  最新搜索结果: {:?}", latest_result);

    // 2. 检查历史搜索
    println!("\n2. 历史搜索:");
    match io.search_history_refnos(missing_refno, None) {
        Ok(history) => {
            if history.is_empty() {
                println!("  ❌ 在任何会话中都找不到该参考号");
            } else {
                println!("  ✓ 找到 {} 个历史版本:", history.len());
                for (sesno, offset) in history.iter() {
                    println!("    会话{}: 偏移{:#X}", sesno, offset);
                }
            }
        }
        Err(e) => {
            println!("  ❌ 历史搜索失败: {}", e);
        }
    }

    // 3. 检查在所有会话中的搜索
    println!("\n3. 逐个会话搜索:");
    let all_sessions: Vec<i32> = io.ses_range_map.keys().cloned().collect();
    println!("  数据库中共有 {} 个会话", all_sessions.len());

    let mut found_sessions = Vec::new();
    for &sesno in &all_sessions {
        if let Some((found_sesno, offset)) =
            io.search_latest_refno(missing_refno, Some(sesno as u32))
        {
            found_sessions.push((found_sesno, offset));
            println!("  ✓ 在会话{}中找到: 偏移{:#X}", found_sesno, offset);
        }
    }

    if found_sessions.is_empty() {
        println!("  ❌ 在所有会话中都找不到该参考号");
    } else {
        println!("  ✓ 总共在 {} 个会话中找到该参考号", found_sessions.len());
    }

    // 4. 使用memchr在二进制数据中搜索
    println!("\n4. 使用memchr在二进制数据中搜索:");
    let target_r0 = missing_refno.get_0();
    let target_r1 = missing_refno.get_1();

    println!(
        "  搜索字节模式: r0={} (0x{:08X}), r1={} (0x{:08X})",
        target_r0, target_r0, target_r1, target_r1
    );

    let target_r0_bytes = target_r0.to_le_bytes();
    let target_r1_bytes = target_r1.to_le_bytes();

    // 读取数据库文件进行二进制搜索
    let db_path = std::path::Path::new(db_filepath);
    if let Ok(file_data) = std::fs::read(db_path) {
        println!(
            "  数据库文件大小: {} bytes ({:.2} MB)",
            file_data.len(),
            file_data.len() as f64 / 1024.0 / 1024.0
        );

        let mut found_positions = Vec::new();
        let mut search_start = 0;

        // 搜索r1的字节模式
        while let Some(pos) = memchr::memmem::find(&file_data[search_start..], &target_r1_bytes) {
            let absolute_pos = search_start + pos;

            // 检查前面4个字节是否匹配r0
            if absolute_pos >= 4 {
                let r0_pos = absolute_pos - 4;
                if &file_data[r0_pos..r0_pos + 4] == &target_r0_bytes {
                    let page_no = r0_pos / 0x800;
                    let page_offset = r0_pos % 0x800;
                    found_positions.push((r0_pos, page_no, page_offset));

                    println!(
                        "  🎯 找到匹配: 文件位置0x{:X}, 页号0x{:X}, 页内偏移0x{:X}",
                        r0_pos, page_no, page_offset
                    );
                }
            }

            search_start = absolute_pos + 1;
            if found_positions.len() >= 10 {
                // 限制搜索结果数量
                break;
            }
        }

        if found_positions.is_empty() {
            println!("  ❌ 在二进制数据中未找到该参考号");
        } else {
            println!(
                "  ✓ 在二进制数据中找到 {} 个匹配位置",
                found_positions.len()
            );

            // 分析找到的位置
            for (i, (file_pos, page_no, page_offset)) in found_positions.iter().enumerate() {
                println!(
                    "\n  位置 {}: 文件0x{:X}, 页号0x{:X}, 偏移0x{:X}",
                    i + 1,
                    file_pos,
                    page_no,
                    page_offset
                );

                // 检查这个页面是否在索引中
                if let Ok(index_data) = io.read_index_data(*page_no as u32) {
                    println!("    📋 这是索引页面 (层级: {})", index_data.level);

                    // 在索引页面中查找
                    for (idx, loc) in index_data.refno_locs.iter().enumerate() {
                        if loc.refno_0 == target_r0 && loc.refno_1 == target_r1 {
                            println!(
                                "    🎯 在索引条目[{}]中找到: {}_{} -> 数据页号0x{:X}",
                                idx, loc.refno_0, loc.refno_1, loc.pgno
                            );

                            // 检查数据页面
                            if let Ok(ele_data) = io.parse_raw_element(loc.get_att_offset()) {
                                println!(
                                    "    ✓ 成功解析元素: 类型={}, 所有者={}",
                                    ele_data.att_map().get_type(),
                                    ele_data.owner
                                );
                            } else {
                                println!("    ❌ 解析元素失败");
                            }
                        }
                    }
                } else {
                    println!("    📄 这可能是数据页面，不是索引页面");
                }
            }
        }
    } else {
        println!("  ❌ 无法读取数据库文件");
    }

    // 5. 检查索引完整性
    println!("\n5. 检查索引完整性:");

    // 构建完整的索引映射
    match io.build_index_map_verbose(false) {
        Ok(index_map) => {
            println!("  ✓ 成功构建索引映射，总参考号数量: {}", index_map.len());

            if index_map.contains_key(&missing_refno) {
                println!("  ✓ 目标参考号在索引映射中存在！");
                if let Some(offsets) = index_map.get(&missing_refno) {
                    println!("    偏移量列表: {:?}", offsets);

                    // 6. 分析找到的偏移量
                    for &offset in offsets {
                        println!("\n6. 分析偏移量 0x{:X}:", offset);

                        // 尝试解析这个偏移量的元素
                        match io.parse_raw_element(offset) {
                            Ok(element) => {
                                println!("  ✓ 成功解析元素:");
                                println!("    类型: {}", element.att_map().get_type());
                                println!("    所有者: {}", element.owner);
                                println!("    子元素数量: {}", element.children.len());

                                // 验证这确实是我们要找的参考号
                                let parsed_refno = element.refno;
                                println!("    解析出的参考号: {}", parsed_refno);

                                if parsed_refno == missing_refno {
                                    println!("    ✅ 确认这就是目标参考号！");
                                } else {
                                    println!("    ❌ 参考号不匹配！");
                                }
                            }
                            Err(e) => {
                                println!("  ❌ 解析元素失败: {}", e);
                            }
                        }

                        // 检查这个偏移量对应的页面和位置
                        let page_no = offset / 0x800;
                        let page_offset = offset % 0x800;
                        println!(
                            "  位置信息: 页号0x{:X}, 页内偏移0x{:X}",
                            page_no, page_offset
                        );

                        // 尝试直接搜索这个参考号
                        println!("  测试搜索算法:");
                        let search_result = io.search_latest_refno(missing_refno, None);
                        println!("    search_latest_refno结果: {:?}", search_result);

                        // 测试在特定会话中搜索
                        for sesno in [81, 82, 83, 84, 85] {
                            let session_result = io.search_latest_refno(missing_refno, Some(sesno));
                            if session_result.is_some() {
                                println!("    在会话{}中找到: {:?}", sesno, session_result);
                            }
                        }
                    }
                }
            } else {
                println!("  ❌ 目标参考号不在索引映射中");

                // 查找相近的参考号
                let nearby_refnos: Vec<_> = index_map
                    .keys()
                    .filter(|&refno| refno.get_0() == target_r0)
                    .take(10)
                    .collect();

                if !nearby_refnos.is_empty() {
                    println!("    相同第一部分({})的参考号:", target_r0);
                    for refno in nearby_refnos {
                        println!("      {}", refno);
                    }
                }
            }
        }
        Err(e) => {
            println!("  ❌ 构建索引映射失败: {}", e);
        }
    }

    println!("\n=== 分析完成 ===");

    Ok(())
}

/// 测试B+树搜索算法的问题
#[tokio::test]
async fn test_btree_search_algorithm_issue() -> anyhow::Result<()> {
    let db_filepath = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7999_0001"#;
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;
    io.init_ses_range_map()?;

    println!("=== 测试B+树搜索算法问题 ===");

    let missing_refno = RefU64::from_two_nums(24383, 66457);
    println!("目标参考号: {}", missing_refno);

    // 1. 获取索引映射中的偏移量
    let index_map = io.build_index_map_verbose(false)?;
    let expected_offset = if let Some(offsets) = index_map.get(&missing_refno) {
        offsets.iter().next().copied().unwrap()
    } else {
        println!("❌ 参考号不在索引映射中");
        return Ok(());
    };

    println!("索引映射中的偏移量: 0x{:X}", expected_offset);

    // 2. 获取根页号
    let basic_info = io.get_page_basic_info()?;
    let root_pgno = basic_info.latest_ses_data.index_root_pageno;
    println!("根页号: 0x{:X}", root_pgno);

    // 3. 测试当前的B+树搜索算法
    println!("\n3. 测试当前B+树搜索算法:");
    let search_result = io.search_latest_refno(missing_refno, None);
    println!("search_latest_refno结果: {:?}", search_result);

    // 4. 手动遍历B+树路径，找出问题所在
    println!("\n4. 手动遍历B+树路径:");
    let (target_r0, target_r1) = (missing_refno.get_0(), missing_refno.get_1());

    let mut current_pgno = root_pgno;
    let mut level = 0;

    loop {
        match io.read_index_data(current_pgno) {
            Ok(index_data) => {
                println!(
                    "\n层级 {}: 页号 0x{:X}, 索引层级: {}, 条目数: {}",
                    level,
                    current_pgno,
                    index_data.level,
                    index_data.refno_locs.len()
                );

                // 显示前几个和后几个条目
                let entries_to_show = 5;
                println!("  前{}个条目:", entries_to_show);
                for (i, loc) in index_data
                    .refno_locs
                    .iter()
                    .take(entries_to_show)
                    .enumerate()
                {
                    println!(
                        "    [{}] {}_{} -> 页号: 0x{:X}, 偏移: 0x{:X}",
                        i, loc.refno_0, loc.refno_1, loc.pgno, loc.offset
                    );
                }

                if index_data.refno_locs.len() > entries_to_show * 2 {
                    println!(
                        "    ... ({} 个条目被省略)",
                        index_data.refno_locs.len() - entries_to_show * 2
                    );
                }

                if index_data.refno_locs.len() > entries_to_show {
                    println!("  后{}个条目:", entries_to_show);
                    let start_idx = index_data.refno_locs.len().saturating_sub(entries_to_show);
                    for (i, loc) in index_data.refno_locs.iter().skip(start_idx).enumerate() {
                        println!(
                            "    [{}] {}_{} -> 页号: 0x{:X}, 偏移: 0x{:X}",
                            start_idx + i,
                            loc.refno_0,
                            loc.refno_1,
                            loc.pgno,
                            loc.offset
                        );
                    }
                }

                // 如果是叶子节点，直接搜索
                if index_data.level == 0 {
                    println!("\n  🍃 叶子节点搜索:");
                    let mut found = false;
                    for (i, loc) in index_data.refno_locs.iter().enumerate() {
                        if loc.refno_0 == target_r0 && loc.refno_1 == target_r1 {
                            println!(
                                "    ✅ 找到目标: [{}] {}_{} -> 页号: 0x{:X}, 偏移: 0x{:X}",
                                i, loc.refno_0, loc.refno_1, loc.pgno, loc.offset
                            );
                            found = true;
                            break;
                        }
                    }

                    if !found {
                        println!("    ❌ 在叶子节点中未找到目标参考号");

                        // 查找最接近的条目
                        let mut closest_entries = Vec::new();
                        for (i, loc) in index_data.refno_locs.iter().enumerate() {
                            if loc.refno_0 == target_r0 {
                                closest_entries.push((i, loc));
                            }
                        }

                        if !closest_entries.is_empty() {
                            println!("    相同第一部分的条目:");
                            for (i, loc) in closest_entries {
                                println!(
                                    "      [{}] {}_{} -> 页号: 0x{:X}, 偏移: 0x{:X}",
                                    i, loc.refno_0, loc.refno_1, loc.pgno, loc.offset
                                );
                            }
                        }
                    }
                    break;
                } else {
                    // 非叶子节点，找到下一个页面
                    println!("\n  🌿 非叶子节点，寻找下一个页面:");

                    let mut next_pgno = None;
                    let mut selected_idx = None;

                    // 应用当前算法的逻辑
                    for (i, loc) in index_data.refno_locs.iter().enumerate() {
                        // 跳过起始标记
                        if loc.refno_0 == 0x80000001 && loc.refno_1 == 0x80000001 {
                            println!(
                                "    [{}] 起始标记: 0x80000001_0x80000001 -> 页号: 0x{:X}",
                                i, loc.pgno
                            );
                            continue;
                        }

                        println!(
                            "    [{}] 比较: {}_{} vs 目标 {}_{}",
                            i, loc.refno_0, loc.refno_1, target_r0, target_r1
                        );

                        if target_r0 < loc.refno_0
                            || (target_r0 == loc.refno_0 && target_r1 <= loc.refno_1)
                        {
                            println!("      ✅ 选择此条目 (目标 <= 当前)");
                            next_pgno = Some(loc.pgno);
                            selected_idx = Some(i);
                            break;
                        } else {
                            println!("      ❌ 目标 > 当前，继续");
                        }
                    }

                    // 如果没有找到合适的条目，选择最后一个
                    if next_pgno.is_none() && !index_data.refno_locs.is_empty() {
                        let last_entry = &index_data.refno_locs[index_data.refno_locs.len() - 1];
                        println!(
                            "    🎯 目标超出范围，选择最后一个条目: {}_{} -> 页号: 0x{:X}",
                            last_entry.refno_0, last_entry.refno_1, last_entry.pgno
                        );
                        next_pgno = Some(last_entry.pgno);
                        selected_idx = Some(index_data.refno_locs.len() - 1);
                    }

                    if let Some(pgno) = next_pgno {
                        println!(
                            "    ➡️  继续搜索页号: 0x{:X} (索引: {:?})",
                            pgno, selected_idx
                        );
                        current_pgno = pgno;
                        level += 1;
                    } else {
                        println!("    ❌ 无法找到下一个页面");
                        break;
                    }
                }
            }
            Err(e) => {
                println!("❌ 读取页面 0x{:X} 失败: {}", current_pgno, e);
                break;
            }
        }
    }

    println!("\n=== B+树搜索分析完成 ===");

    Ok(())
}

/// 测试边界情况
#[tokio::test]
async fn test_collect_latest_eles_edge_cases() -> anyhow::Result<()> {
    let db_filepath = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7997_001"#;
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;
    io.init_ses_range_map()?;

    println!("测试边界情况");

    // 测试用例1: 会话数量为0
    println!("\n测试1: 会话数量为0");
    let result = io.collect_latest_eles(Some(0))?;
    assert!(result.is_empty(), "会话数量为0时应该返回空结果");
    println!("✓ 会话数量为0时正确返回空结果");

    // 测试用例2: 会话数量为1
    println!("\n测试2: 会话数量为1");
    let result = io.collect_latest_eles(Some(1))?;
    println!("会话数量为1时返回 {} 个元素", result.len());

    // 验证所有元素都来自同一个会话（最新会话）
    let latest_sesno = io.get_latest_sesno()?;
    let mut session_numbers: HashSet<u32> = HashSet::new();
    for (_, operation_data) in &result {
        session_numbers.insert(operation_data.sesno);
    }

    if !result.is_empty() {
        assert_eq!(
            session_numbers.len(),
            1,
            "会话数量为1时，所有元素应该来自同一个会话"
        );
        assert!(session_numbers.contains(&latest_sesno), "应该是最新会话");
        println!("✓ 所有元素都来自最新会话 {}", latest_sesno);
    }

    println!("\n边界情况测试通过！");

    Ok(())
}

/// 测试搜索算法性能优化
#[tokio::test]
async fn test_search_performance_optimization() -> anyhow::Result<()> {
    // 首先尝试 ams7997_001，如果不存在则使用 ams1112_0001
    let db_filepath_primary = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7999_0001"#;

    let db_filepath = if std::path::Path::new(db_filepath_primary).exists() {
        db_filepath_primary
    } else {
        println!("❌ 数据库文件都不存在，跳过测试");
        return Ok(());
    };
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;
    io.init_ses_range_map()?;

    println!("🚀 测试搜索算法性能优化");

    // 首先检查数据库中实际存在的参考号范围
    println!("🔍 检查数据库中的参考号范围...");

    // 获取数据库基本信息
    let basic_info = io.get_page_basic_info().unwrap();
    println!(
        "📊 最新会话数据: sesno={}, index_root_pgno=0x{:X}",
        basic_info.latest_ses_data.sesno, basic_info.latest_ses_data.index_root_pageno
    );

    // 检查所有会话的索引根节点
    println!("📋 检查所有会话的索引根节点:");
    let sesno_pgno_pairs: Vec<_> = io.sesno_pgno_map.iter().map(|(k, v)| (*k, *v)).collect();
    for (sesno, pgno) in sesno_pgno_pairs {
        if let Ok(ses_data) = io.read_ses_data(pgno) {
            println!(
                "  会话 {}: 索引根页号=0x{:X}",
                sesno, ses_data.index_root_pageno
            );

            // 检查这个索引根节点的范围
            let index_root_pageno = ses_data.index_root_pageno;
            if let Ok(index_data) = io.read_index_data(index_root_pageno) {
                if !index_data.refno_locs.is_empty() {
                    let first = &index_data.refno_locs[0];
                    let last = &index_data.refno_locs[index_data.refno_locs.len() - 1];
                    println!(
                        "    层级: {}, 范围: {}_{} 到 {}_{}",
                        index_data.level, first.refno_0, first.refno_1, last.refno_0, last.refno_1
                    );
                }
            }
        }
    }

    // 使用 memchr 在二进制数据中搜索参考号 24383_101192 的位置
    println!("🔍 使用 memchr 在二进制数据中搜索参考号 24383_101192...");

    let target_refno = RefU64::from_two_nums(24383, 101192);
    let (target_r0, target_r1) = (target_refno.get_0(), target_refno.get_1());

    println!(
        "🎯 目标参考号: {}_{} (0x{:08X}_{:08X})",
        target_r0, target_r1, target_r0, target_r1
    );

    // 将参考号转换为字节序列进行搜索
    let target_r0_bytes = target_r0.to_le_bytes();
    let target_r1_bytes = target_r1.to_le_bytes();

    println!("🔍 搜索字节模式:");
    println!("  r0 bytes: {:02X?}", target_r0_bytes);
    println!("  r1 bytes: {:02X?}", target_r1_bytes);

    // 读取整个数据库文件进行搜索
    let db_path = &get_db_option().project_path;
    println!("📂 数据库路径: {}", db_path);

    // 使用一个示例数据库文件路径 - 使用 7999 文件
    let sample_db_file = format!("{}/AvevaMarineSample/ams000/ams7999_0001", db_path);
    println!("📂 示例数据库文件: {}", sample_db_file);

    // 在已加载的页面缓存中搜索
    println!("🔍 在已加载的页面缓存中搜索参考号...");
    let mut found_positions = Vec::new();

    // 搜索最新会话的索引页面
    let root_pgno = basic_info.latest_ses_data.index_root_pageno;

    // 递归搜索所有索引页面
    let mut pages_to_search = vec![root_pgno];
    let mut searched_pages = std::collections::HashSet::new();

    while let Some(page_no) = pages_to_search.pop() {
        if searched_pages.contains(&page_no) {
            continue;
        }
        searched_pages.insert(page_no);

        if let Ok(page_data) = io.read_bytes(page_no * 0x800, 0x800) {
            let page_bytes = page_data.as_slice();
            let mut search_start = 0;

            while let Some(pos) =
                memchr::memmem::find(&page_bytes[search_start..], &target_r1_bytes)
            {
                let absolute_pos = search_start + pos;

                // 检查前面4个字节是否匹配 r0
                if absolute_pos >= 4 {
                    let r0_pos = absolute_pos - 4;
                    if &page_bytes[r0_pos..r0_pos + 4] == &target_r0_bytes {
                        let file_position = (page_no as usize) * 0x800 + r0_pos;
                        found_positions.push((file_position, page_no as u32, r0_pos));
                        println!(
                            "🎯 在页面 0x{:X} 偏移 0x{:X} 找到匹配 (文件位置: 0x{:X})",
                            page_no, r0_pos, file_position
                        );
                    }
                }

                search_start = absolute_pos + 1;
            }

            // 如果是索引页面，添加子页面到搜索列表
            if let Ok(index_data) = io.read_index_data(page_no) {
                for loc in &index_data.refno_locs {
                    if !searched_pages.contains(&loc.pgno) {
                        pages_to_search.push(loc.pgno);
                    }
                }
            }
        }
    }

    if !found_positions.is_empty() {
        println!("✅ 总共找到 {} 个匹配位置:", found_positions.len());

        for (i, (file_pos, page_no, page_offset)) in found_positions.iter().enumerate() {
            println!(
                "\n📍 匹配 {}: 页面 0x{:X}, 页内偏移 0x{:X}, 文件位置 0x{:X}",
                i + 1,
                page_no,
                page_offset,
                file_pos
            );

            // 分析这个页面的类型
            if let Ok(index_data) = io.read_index_data(*page_no) {
                println!("  📋 索引页面信息:");
                println!("    层级: {}", index_data.level);
                println!("    条目数: {}", index_data.refno_locs.len());

                if index_data.level == 0 {
                    println!("    🍃 这是叶子节点");

                    // 在叶子节点中查找具体位置
                    for (idx, loc) in index_data.refno_locs.iter().enumerate() {
                        if loc.refno_0 == target_r0 && loc.refno_1 == target_r1 {
                            println!(
                                "    🎯 在索引条目 [{}] 中找到: {}_{} -> 数据页号: 0x{:X}",
                                idx, loc.refno_0, loc.refno_1, loc.pgno
                            );

                            // 反向查找这个叶子节点在索引树中的路径
                            println!("    🔍 反向查找索引路径:");
                            find_leaf_in_index_tree(&mut io, *page_no, 0x673F);
                            break;
                        }
                    }
                } else {
                    println!("    🌿 这是非叶子节点 (层级 {})", index_data.level);
                }
            } else {
                println!("  📄 这可能是数据页面，不是索引页面");
            }
        }
    } else {
        println!("❌ 在已加载的页面缓存中未找到目标参考号");

        // 尝试读取数据库文件进行搜索
        match std::fs::read(&sample_db_file) {
            Ok(file_data) => {
                println!(
                    "📊 文件大小: {} bytes ({:.2} MB)",
                    file_data.len(),
                    file_data.len() as f64 / 1024.0 / 1024.0
                );

                // 使用 memchr 搜索 r1 的字节模式
                let mut search_start = 0;
                let mut total_matches = 0;

                println!("🔍 开始 memchr 搜索...");
                while let Some(pos) =
                    memchr::memmem::find(&file_data[search_start..], &target_r1_bytes)
                {
                    let absolute_pos = search_start + pos;

                    // 检查前面4个字节是否匹配 r0
                    if absolute_pos >= 4 {
                        let r0_pos = absolute_pos - 4;
                        if &file_data[r0_pos..r0_pos + 4] == &target_r0_bytes {
                            total_matches += 1;
                            found_positions.push((r0_pos, (r0_pos / 0x800) as u32, r0_pos % 0x800));
                            println!("🎯 找到匹配 #{}: 文件位置 0x{:X} ({}), 页号 0x{:X}, 页内偏移 0x{:X}",
                                total_matches, r0_pos, r0_pos, r0_pos / 0x800, r0_pos % 0x800);
                        }
                    }

                    search_start = absolute_pos + 1;
                    if found_positions.len() >= 20 {
                        // 增加搜索结果数量限制
                        break;
                    }
                }

                println!("📊 memchr 搜索完成，共找到 {} 个匹配", total_matches);

                if found_positions.is_empty() {
                    println!("❌ 未在二进制数据中找到参考号 {}_{}", target_r0, target_r1);
                } else {
                    println!("✅ 找到 {} 个匹配位置:", found_positions.len());

                    for (i, (file_pos, page_no, page_offset)) in found_positions.iter().enumerate()
                    {
                        println!(
                            "\n📍 位置 {}: 页面 0x{:X}, 页内偏移 0x{:X}, 文件位置 0x{:X}",
                            i + 1,
                            page_no,
                            page_offset,
                            file_pos
                        );

                        // 分析这个页面的类型
                        if let Some(sesno) = find_session_for_page(&mut io, *page_no) {
                            println!("  🏷️  所属会话: {}", sesno);

                            // 检查这个页面是否是索引页面
                            if let Ok(index_data) = io.read_index_data(*page_no) {
                                println!("  📋 索引页面信息:");
                                println!("    层级: {}", index_data.level);
                                println!("    条目数: {}", index_data.refno_locs.len());

                                if index_data.level == 0 {
                                    println!("    🍃 这是叶子节点");

                                    // 在叶子节点中查找具体位置
                                    for (idx, loc) in index_data.refno_locs.iter().enumerate() {
                                        if loc.refno_0 == target_r0 && loc.refno_1 == target_r1 {
                                            println!("    🎯 在索引条目 [{}] 中找到: {}_{} -> 数据页号: 0x{:X}",
                                                idx, loc.refno_0, loc.refno_1, loc.pgno);

                                            // 反向查找这个叶子节点在索引树中的路径
                                            println!("    🔍 反向查找索引路径:");
                                            find_leaf_in_index_tree(&mut io, *page_no, root_pgno);
                                            break;
                                        }
                                    }
                                } else {
                                    println!("    🌿 这是非叶子节点 (层级 {})", index_data.level);
                                }
                            } else {
                                println!("  📄 这可能是数据页面，不是索引页面");
                            }
                        } else {
                            println!("  ❓ 无法确定所属会话");
                        }
                    }
                }
            }
            Err(e) => {
                println!("❌ 读取数据库文件失败: {}", e);
            }
        }
    }

    // 分析层级1的索引分布规律（保留原有逻辑作为对比）
    let root_pgno = basic_info.latest_ses_data.index_root_pageno;
    if let Ok(root_data) = io.read_index_data(root_pgno) {
        println!("📊 根节点分析 (层级 {}):", root_data.level);

        // 找到包含目标范围的分支
        for (i, loc) in root_data.refno_locs.iter().enumerate() {
            if !(loc.refno_0 == 2147483649 && loc.refno_1 == 2147483649) {
                if target_r0 < loc.refno_0 || (target_r0 == loc.refno_0 && target_r1 <= loc.refno_1)
                {
                    println!(
                        "  应该在分支 [{}]: 最大值 {}_{} -> 页号 0x{:X}",
                        i, loc.refno_0, loc.refno_1, loc.pgno
                    );

                    // 分析这个分支的层级1索引
                    if let Ok(level1_data) = io.read_index_data(loc.pgno) {
                        println!(
                            "📋 层级1索引分析 (页号 0x{:X}, 层级 {}):",
                            loc.pgno, level1_data.level
                        );

                        // 计算参考号分布规律
                        let mut valid_entries: Vec<(usize, &RefnoDataLoc)> = level1_data
                            .refno_locs
                            .iter()
                            .enumerate()
                            .filter(|(_, loc)| {
                                !(loc.refno_0 == 2147483649 && loc.refno_1 == 2147483649)
                            })
                            .collect();

                        if valid_entries.len() >= 2 {
                            // 分析参考号间隔
                            let first = valid_entries[0].1;
                            let second = valid_entries[1].1;
                            let interval = second.refno_1 - first.refno_1;

                            println!("  参考号分布规律:");
                            println!("    第一个: {}_{}", first.refno_0, first.refno_1);
                            println!("    第二个: {}_{}", second.refno_0, second.refno_1);
                            println!("    间隔: {}", interval);

                            // 根据规律推算目标参考号应该在的位置
                            let target_index = ((target_r1 - first.refno_1) / interval) as usize;

                            println!("  🎯 推算目标参考号 {}_{} 应该在:", target_r0, target_r1);
                            println!("    计算索引: {}", target_index);

                            if target_index < valid_entries.len() {
                                let predicted_entry = valid_entries[target_index].1;
                                println!(
                                    "    预测分支: [{}] 最大值 {}_{} -> 页号 0x{:X}",
                                    target_index,
                                    predicted_entry.refno_0,
                                    predicted_entry.refno_1,
                                    predicted_entry.pgno
                                );

                                // 检查这个叶子节点
                                if let Ok(leaf_data) = io.read_index_data(predicted_entry.pgno) {
                                    if leaf_data.level == 0 {
                                        println!(
                                            "    🍃 叶子节点分析 (页号 0x{:X}):",
                                            predicted_entry.pgno
                                        );
                                        if !leaf_data.refno_locs.is_empty() {
                                            let first_leaf = &leaf_data.refno_locs[0];
                                            let last_leaf = &leaf_data.refno_locs
                                                [leaf_data.refno_locs.len() - 1];
                                            println!(
                                                "      范围: {}_{} 到 {}_{}",
                                                first_leaf.refno_0,
                                                first_leaf.refno_1,
                                                last_leaf.refno_0,
                                                last_leaf.refno_1
                                            );

                                            // 检查目标参考号是否在这个范围内
                                            if target_r1 >= first_leaf.refno_1
                                                && target_r1 <= last_leaf.refno_1
                                            {
                                                println!("      ✅ 目标参考号在此范围内！");

                                                // 在这个叶子节点中搜索
                                                if let Some(result) = io.search_in_leaf_node(
                                                    &leaf_data.refno_locs,
                                                    target_r0,
                                                    target_r1,
                                                ) {
                                                    println!(
                                                        "      🎉 找到目标参考号！结果: {:?}",
                                                        result
                                                    );
                                                } else {
                                                    println!(
                                                        "      ❌ 在叶子节点中未找到目标参考号"
                                                    );

                                                    // 显示叶子节点的详细内容
                                                    println!("      📋 叶子节点详细内容:");
                                                    for (i, loc) in leaf_data
                                                        .refno_locs
                                                        .iter()
                                                        .take(10)
                                                        .enumerate()
                                                    {
                                                        println!(
                                                            "        [{}] {}_{} -> 页号: 0x{:X}",
                                                            i, loc.refno_0, loc.refno_1, loc.pgno
                                                        );
                                                    }
                                                    if leaf_data.refno_locs.len() > 10 {
                                                        println!("        ... (省略中间部分) ...");
                                                        let start_idx = leaf_data
                                                            .refno_locs
                                                            .len()
                                                            .saturating_sub(10);
                                                        for (i, loc) in leaf_data
                                                            .refno_locs
                                                            .iter()
                                                            .skip(start_idx)
                                                            .enumerate()
                                                        {
                                                            println!("        [{}] {}_{} -> 页号: 0x{:X}", start_idx + i, loc.refno_0, loc.refno_1, loc.pgno);
                                                        }
                                                    }
                                                }
                                            } else {
                                                println!("      ❌ 目标参考号不在此范围内");

                                                // 尝试下一个叶子节点
                                                if target_index + 1 < valid_entries.len() {
                                                    let next_entry =
                                                        valid_entries[target_index + 1].1;
                                                    println!("    🔍 尝试下一个分支: [{}] 最大值 {}_{} -> 页号 0x{:X}",
                                                        target_index + 1, next_entry.refno_0, next_entry.refno_1, next_entry.pgno);

                                                    if let Ok(next_leaf_data) =
                                                        io.read_index_data(next_entry.pgno)
                                                    {
                                                        if next_leaf_data.level == 0
                                                            && !next_leaf_data.refno_locs.is_empty()
                                                        {
                                                            let first_next =
                                                                &next_leaf_data.refno_locs[0];
                                                            let last_next = &next_leaf_data
                                                                .refno_locs[next_leaf_data
                                                                .refno_locs
                                                                .len()
                                                                - 1];
                                                            println!(
                                                                "      范围: {}_{} 到 {}_{}",
                                                                first_next.refno_0,
                                                                first_next.refno_1,
                                                                last_next.refno_0,
                                                                last_next.refno_1
                                                            );

                                                            if target_r1 >= first_next.refno_1
                                                                && target_r1 <= last_next.refno_1
                                                            {
                                                                println!("      ✅ 目标参考号在下一个叶子节点范围内！");
                                                                if let Some(result) = io
                                                                    .search_in_leaf_node(
                                                                        &next_leaf_data.refno_locs,
                                                                        target_r0,
                                                                        target_r1,
                                                                    )
                                                                {
                                                                    println!("      🎉 找到目标参考号！结果: {:?}", result);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                println!("    ❌ 计算的索引超出范围");
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    // 测试特定的参考号 - 对比新旧算法
    let test_refnos = vec![
        RefU64::from_two_nums(24383, 101192), // 用户发现的参考号
        RefU64::from_two_nums(24383, 101200), // 另一个测试参考号
    ];

    let latest_sesno = basic_info.latest_ses_data.sesno;

    for (i, target_refno) in test_refnos.iter().enumerate() {
        println!("\n🎯 测试参考号 {}: {}", i + 1, target_refno);

        // 测试传统算法
        println!("📊 传统算法测试:");
        let start_time = Instant::now();
        let traditional_result =
            search_refno_in_btree_traditional(&mut io, target_refno, latest_sesno as u32);
        let traditional_time = start_time.elapsed();

        println!(
            "  ⚡ 传统算法耗时: {:.4}ms",
            traditional_time.as_secs_f64() * 1000.0
        );
        println!("  🔍 传统算法结果: {:?}", traditional_result);

        // 测试优化算法
        println!("📊 优化算法测试:");
        let start_time = Instant::now();
        let optimized_result =
            search_refno_in_btree_optimized(&mut io, target_refno, latest_sesno as u32);
        let optimized_time = start_time.elapsed();

        println!(
            "  ⚡ 优化算法耗时: {:.4}ms",
            optimized_time.as_secs_f64() * 1000.0
        );
        println!("  🔍 优化算法结果: {:?}", optimized_result);

        // 性能对比
        if traditional_time > optimized_time {
            let speedup = traditional_time.as_secs_f64() / optimized_time.as_secs_f64();
            println!("  🚀 优化算法快 {:.2}x", speedup);
        } else if optimized_time > traditional_time {
            let slowdown = optimized_time.as_secs_f64() / traditional_time.as_secs_f64();
            println!("  🐌 优化算法慢 {:.2}x", slowdown);
        } else {
            println!("  ⚖️ 两种算法性能相当");
        }

        // 结果对比
        match (traditional_result.is_some(), optimized_result.is_some()) {
            (true, true) => println!("  ✅ 两种算法都找到了结果"),
            (false, true) => println!("  🎯 只有优化算法找到了结果！"),
            (true, false) => println!("  ⚠️ 只有传统算法找到了结果"),
            (false, false) => println!("  ❌ 两种算法都未找到结果"),
        }
    }

    // 详细分析为什么找不到目标参考号
    println!("\n🔍 深度分析: 为什么找不到 24383_101192");
    analyze_missing_refno(&mut io, RefU64::from_two_nums(24383, 101192)).await;

    println!("\n特定参考号测试完成！");

    Ok(())
}

/// 详细分析参考号 24383/101192 为什么返回"无操作"
#[tokio::test]
async fn test_analyze_refno_none_status() -> anyhow::Result<()> {
    // 直接使用 ams7997_0001 数据库
    let db_filepath = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7997_0001"#;

    if !std::path::Path::new(db_filepath).exists() {
        println!("❌ 数据库文件不存在: {}", db_filepath);
        return Ok(());
    }
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;
    io.init_ses_range_map()?;

    println!("=== 详细分析参考号 24383/101192 为什么返回'无操作' ===");

    let test_refno = RefU64::from_two_nums(24383, 101192);
    println!("目标参考号: {}", test_refno);

    // 步骤1: 检查参考号是否存在
    println!("\n步骤1: 检查参考号是否存在");
    match io.search_latest_refno(test_refno, None) {
        Some((sesno, offset)) => {
            println!("✓ 参考号存在 - 会话号: {}, 偏移量: {:#X}", sesno, offset);
        }
        None => {
            println!("❌ 参考号不存在");
            return Ok(());
        }
    }

    // 步骤2: 检查历史版本
    println!("\n步骤2: 检查历史版本");
    match io.search_history_refnos(test_refno, None) {
        Ok(history) => {
            println!("找到 {} 个历史版本:", history.len());
            for (i, (sesno, offset)) in history.iter().enumerate() {
                println!("  版本 {}: 会话号={}, 偏移={:#X}", i + 1, sesno, offset);
            }
        }
        Err(e) => {
            println!("搜索历史记录失败: {}", e);
        }
    }

    // 步骤3: 使用 search_latest_and_prev_refno 检查最新和前一个版本
    println!("\n步骤3: 检查最新版本和前一个版本");
    let [latest, previous] = io.search_latest_and_prev_refno(test_refno, None);

    match latest {
        Some((latest_sesno, latest_offset)) => {
            println!(
                "✓ 最新版本 - 会话号: {}, 偏移量: {:#X}",
                latest_sesno, latest_offset
            );

            match previous {
                Some((prev_sesno, prev_offset)) => {
                    println!(
                        "✓ 前一版本 - 会话号: {}, 偏移量: {:#X}",
                        prev_sesno, prev_offset
                    );
                    println!("  → 有两个版本，应该进行比较分析");
                }
                None => {
                    println!("❌ 没有前一版本");
                    println!("  → 只有一个版本，应该返回'新增'状态");
                }
            }
        }
        None => {
            println!("❌ 没有找到最新版本");
            return Ok(());
        }
    }

    // 步骤4: 解析最新版本的元素数据
    println!("\n步骤4: 解析最新版本的元素数据");
    if let Some((latest_sesno, latest_offset)) = latest {
        match io.parse_raw_element(latest_offset) {
            Ok(latest_ele) => {
                println!("✓ 成功解析最新版本元素");
                println!("  类型: {}", latest_ele.att_map().get_type());
                println!("  所有者: {}", latest_ele.owner);
                println!("  属性数量: {}", latest_ele.att_map().len());
                println!("  子元素数量: {}", latest_ele.children.len());

                // 检查所有者元素是否存在
                println!("\n步骤5: 检查所有者元素");
                match io.auto_get_raw_element(latest_ele.owner) {
                    Ok(owner_ele) => {
                        println!("✓ 成功获取所有者元素");
                        println!("  所有者类型: {}", owner_ele.att_map().get_type());
                        println!("  所有者子元素数量: {}", owner_ele.children.len());

                        if owner_ele.children.contains(&test_refno) {
                            println!("✓ 目标参考号在所有者的子元素列表中");
                        } else {
                            println!("❌ 目标参考号不在所有者的子元素列表中 → 应该返回'已删除'");
                        }
                    }
                    Err(e) => {
                        println!("❌ 获取所有者元素失败: {}", e);
                        if latest_ele.att_map().get_type() == "SITE" {
                            println!("  → SITE类型会被跳过检查");
                        } else {
                            println!("  → 这会导致返回'无操作'状态");
                        }
                    }
                }
            }
            Err(e) => {
                println!("❌ 解析最新版本失败: {}", e);
                println!("  → 这会导致返回'无操作'状态");
            }
        }
    }

    println!("\n=== 分析完成 ===");

    Ok(())
}

/// 分析数据库中实际存在的参考号范围和分布
#[tokio::test]
async fn test_analyze_refno_range_in_db() -> anyhow::Result<()> {
    // 直接使用 ams7997_0001 数据库
    let db_filepath = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7997_0001"#;

    if !std::path::Path::new(db_filepath).exists() {
        println!("❌ 数据库文件不存在: {}", db_filepath);
        return Ok(());
    }
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;
    io.init_ses_range_map()?;

    println!("=== 分析数据库中的参考号范围和分布 ===");

    // 步骤1: 提取一些实际存在的参考号
    println!("\n步骤1: 提取数据库中实际存在的参考号样本");
    match crate::io::extract_test_refnos(&mut io, 20) {
        Ok(refnos) => {
            println!("成功提取 {} 个参考号样本:", refnos.len());
            for (i, refno) in refnos.iter().enumerate() {
                println!("  {}: {}", i + 1, refno);
            }

            // 分析参考号的范围
            if !refnos.is_empty() {
                let min_refno = refnos.iter().min().unwrap();
                let max_refno = refnos.iter().max().unwrap();
                println!("\n参考号范围:");
                println!("  最小: {}", min_refno);
                println!("  最大: {}", max_refno);

                // 检查目标参考号是否在范围内
                let target_refno = RefU64::from_two_nums(24383, 101192);
                println!("\n目标参考号 {} 分析:", target_refno);
                if target_refno >= *min_refno && target_refno <= *max_refno {
                    println!("  ✓ 在数据库参考号范围内");
                } else {
                    println!("  ❌ 超出数据库参考号范围");
                    println!("    目标: {}", target_refno);
                    println!("    范围: {} ~ {}", min_refno, max_refno);
                }
            }
        }
        Err(e) => {
            println!("❌ 提取参考号失败: {}", e);
        }
    }

    // 步骤2: 构建完整的索引映射来分析
    println!("\n步骤2: 构建索引映射分析参考号分布");
    let start = Instant::now();
    match io.build_index_map_verbose(false) {
        Ok(index_map) => {
            let elapsed = start.elapsed();
            println!("✓ 成功构建索引映射，耗时: {:?}", elapsed);
            println!("  总参考号数量: {}", index_map.len());

            // 分析参考号分布
            if !index_map.is_empty() {
                let all_refnos: Vec<_> = index_map.keys().collect();
                let min_refno = all_refnos.iter().min().unwrap();
                let max_refno = all_refnos.iter().max().unwrap();

                println!("\n完整参考号分布:");
                println!("  最小: {}", min_refno);
                println!("  最大: {}", max_refno);
                println!("  总数: {}", all_refnos.len());

                // 检查目标参考号
                let target_refno = RefU64::from_two_nums(24383, 101192);
                if index_map.contains_key(&target_refno) {
                    println!("  ✓ 找到目标参考号 {} 在索引中!", target_refno);
                    if let Some(offsets) = index_map.get(&target_refno) {
                        println!("    偏移量数量: {}", offsets.len());
                        for (i, offset) in offsets.iter().enumerate() {
                            println!("    偏移量 {}: {:#X}", i + 1, offset);
                        }
                    }
                } else {
                    println!("  ❌ 目标参考号 {} 不在索引中", target_refno);
                }

                // 查找相近的参考号
                println!("\n查找与目标参考号相近的参考号:");
                let target_refno = RefU64::from_two_nums(24383, 101192);
                let mut nearby_refnos: Vec<_> = all_refnos
                    .iter()
                    .filter(|&&refno| {
                        let diff = if *refno > target_refno {
                            refno.get_0() as u64 * 0x100000000 + refno.get_1() as u64
                                - (target_refno.get_0() as u64 * 0x100000000
                                    + target_refno.get_1() as u64)
                        } else {
                            target_refno.get_0() as u64 * 0x100000000 + target_refno.get_1() as u64
                                - (refno.get_0() as u64 * 0x100000000 + refno.get_1() as u64)
                        };
                        diff < 1000000 // 在100万范围内
                    })
                    .collect();

                nearby_refnos.sort();

                if nearby_refnos.is_empty() {
                    println!("  没有找到相近的参考号");
                } else {
                    println!("  找到 {} 个相近的参考号:", nearby_refnos.len());
                    for (i, refno) in nearby_refnos.iter().take(10).enumerate() {
                        println!("    {}: {}", i + 1, refno);
                    }
                }

                // 分析参考号的第一部分分布
                println!("\n分析参考号第一部分的分布:");
                let mut first_parts: std::collections::HashMap<u32, u32> =
                    std::collections::HashMap::new();
                for refno in all_refnos.iter() {
                    let first_part = refno.get_0();
                    *first_parts.entry(first_part).or_insert(0) += 1;
                }

                let mut sorted_parts: Vec<_> = first_parts.iter().collect();
                sorted_parts.sort_by_key(|&(k, _)| k);

                println!("  前10个最常见的第一部分:");
                for (i, (part, count)) in sorted_parts.iter().take(10).enumerate() {
                    println!("    {}: {} (出现 {} 次)", i + 1, part, count);
                }

                // 检查目标参考号的第一部分
                let target_first_part = 24383;
                if let Some(count) = first_parts.get(&target_first_part) {
                    println!(
                        "  ✓ 目标第一部分 {} 存在，出现 {} 次",
                        target_first_part, count
                    );
                } else {
                    println!("  ❌ 目标第一部分 {} 不存在", target_first_part);
                }
            }
        }
        Err(e) => {
            println!("❌ 构建索引映射失败: {}", e);
        }
    }

    println!("\n=== 参考号分析完成 ===");

    Ok(())
}

/// 专门测试 24383/101192 的搜索过程，带详细调试信息
#[tokio::test]
async fn test_debug_search_24383_101192() -> anyhow::Result<()> {
    // 使用正确的 ams7999_0001 数据库
    let db_filepath = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7999_0001"#;

    if !std::path::Path::new(db_filepath).exists() {
        println!("❌ 数据库文件不存在: {}", db_filepath);
        return Ok(());
    }
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;
    io.init_ses_range_map()?;

    println!("=== 调试搜索 24383/101192 ===");

    let target_refno = RefU64::from_two_nums(24383, 101192);
    println!("🎯 目标参考号: {}", target_refno);

    // 测试 search_latest_and_prev_refno 方法
    println!("\n📍 调用 search_latest_and_prev_refno...");
    let [latest, previous] = io.search_latest_and_prev_refno(target_refno, None);

    println!("\n📊 搜索结果总结:");
    println!("  最新版本: {:?}", latest);
    println!("  前一个版本: {:?}", previous);

    // 测试 get_refno_operation_status 方法
    println!("\n📍 调用 get_refno_operation_status...");
    match io.get_refno_operation_status(target_refno, None) {
        Ok(status_map) => {
            println!("✓ 操作状态获取成功:");
            for (refno, detail) in status_map {
                println!("  {}: {:?}", refno, detail);
            }
        }
        Err(e) => {
            println!("❌ 操作状态获取失败: {}", e);
        }
    }

    println!("\n=== 调试搜索完成 ===");

    Ok(())
}

/// 检查数据库中是否存在大于 24383_101112 的参考号
#[tokio::test]
async fn test_check_larger_refnos() -> anyhow::Result<()> {
    let db_filepath = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7999_0001"#;

    if !std::path::Path::new(db_filepath).exists() {
        println!("❌ 数据库文件不存在: {}", db_filepath);
        return Ok(());
    }
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;
    io.init_ses_range_map()?;

    println!("=== 检查数据库中是否存在更大的参考号 ===");

    // 构建完整的索引映射
    println!("构建索引映射...");
    let start = Instant::now();
    match io.build_index_map_verbose(false) {
        Ok(index_map) => {
            let elapsed = start.elapsed();
            println!("✓ 成功构建索引映射，耗时: {:?}", elapsed);
            println!("  总参考号数量: {}", index_map.len());

            // 查找所有 24383 开头的参考号
            let mut refnos_24383: Vec<_> = index_map
                .keys()
                .filter(|refno| refno.get_0() == 24383)
                .collect();

            refnos_24383.sort();

            println!("\n24383 系列参考号统计:");
            println!("  总数: {}", refnos_24383.len());

            if !refnos_24383.is_empty() {
                let min_refno = refnos_24383.first().unwrap();
                let max_refno = refnos_24383.last().unwrap();
                println!("  最小: {}", min_refno);
                println!("  最大: {}", max_refno);

                // 检查是否存在大于 101112 的参考号
                let target_threshold = RefU64::from_two_nums(24383, 101112);
                let larger_refnos: Vec<_> = refnos_24383
                    .iter()
                    .filter(|&&refno| *refno > target_threshold)
                    .collect();

                if larger_refnos.is_empty() {
                    println!("  ❌ 没有找到大于 24383_101112 的参考号");
                    println!("  ✓ 这解释了为什么 24383_101192 找不到");
                } else {
                    println!(
                        "  ✓ 找到 {} 个大于 24383_101112 的参考号:",
                        larger_refnos.len()
                    );
                    for (i, refno) in larger_refnos.iter().take(10).enumerate() {
                        println!("    {}: {}", i + 1, refno);
                    }
                    if larger_refnos.len() > 10 {
                        println!("    ... (还有 {} 个)", larger_refnos.len() - 10);
                    }

                    // 检查目标参考号是否存在
                    let target_refno = RefU64::from_two_nums(24383, 101192);
                    if index_map.contains_key(&target_refno) {
                        println!("  🎯 目标参考号 {} 确实存在于数据库中！", target_refno);
                    } else {
                        println!("  ❌ 目标参考号 {} 不存在于数据库中", target_refno);
                    }
                }

                // 显示 101100-101200 范围内的参考号
                println!("\n101100-101200 范围内的 24383 参考号:");
                let range_refnos: Vec<_> = refnos_24383
                    .iter()
                    .filter(|&&refno| {
                        let r1 = refno.get_1();
                        r1 >= 101100 && r1 <= 101200
                    })
                    .collect();

                if range_refnos.is_empty() {
                    println!("  ❌ 该范围内没有参考号");
                } else {
                    println!("  找到 {} 个参考号:", range_refnos.len());
                    for refno in range_refnos {
                        println!("    {}", refno);
                    }
                }
            }
        }
        Err(e) => {
            println!("❌ 构建索引映射失败: {}", e);
        }
    }

    println!("\n=== 检查完成 ===");

    Ok(())
}

// 辅助函数：查找页面所属的会话
fn find_session_for_page(io: &mut PdmsIO, page_no: u32) -> Option<u32> {
    // 克隆映射以避免借用冲突
    let sesno_pgno_map = io.sesno_pgno_map.clone();

    for (&sesno, &ses_pgno) in &sesno_pgno_map {
        if let Ok(_ses_data) = io.read_ses_data(ses_pgno) {
            // 检查页面是否在这个会话的范围内
            let ses_start_page = ses_pgno;
            let ses_end_page = ses_start_page + 1000; // 估算会话页面范围

            if page_no >= ses_start_page && page_no <= ses_end_page {
                return Some(sesno as u32);
            }
        }
    }
    None
}

// 辅助函数：反向查找索引路径
fn find_parent_index_path(io: &mut PdmsIO, leaf_page_no: u32, sesno: u32) {
    println!("🔍 反向查找叶子节点 0x{:X} 在索引树中的路径:", leaf_page_no);

    // 获取会话的根索引页面
    if let Some(&ses_pgno) = io.sesno_pgno_map.get(&(sesno as i32)) {
        if let Ok(ses_data) = io.read_ses_data(ses_pgno) {
            let root_pgno = ses_data.index_root_pageno;
            println!("  📊 会话 {} 的根索引页号: 0x{:X}", sesno, root_pgno);

            // 递归查找路径
            find_path_to_leaf(io, root_pgno, leaf_page_no, vec![root_pgno]);
        }
    }
}

// 递归查找到叶子节点的路径
fn find_path_to_leaf(io: &mut PdmsIO, current_page: u32, target_leaf: u32, path: Vec<u32>) -> bool {
    if let Ok(index_data) = io.read_index_data(current_page) {
        if index_data.level == 0 {
            // 到达叶子节点
            if current_page == target_leaf {
                println!(
                    "  🎯 找到路径: {:?}",
                    path.iter()
                        .map(|p| format!("0x{:X}", p))
                        .collect::<Vec<_>>()
                        .join(" → ")
                );
                return true;
            }
            return false;
        }

        // 非叶子节点，继续搜索子节点
        for (i, loc) in index_data.refno_locs.iter().enumerate() {
            if !(loc.refno_0 == 2147483649 && loc.refno_1 == 2147483649) {
                let child_page = loc.pgno;
                let mut new_path = path.clone();
                new_path.push(child_page);

                if find_path_to_leaf(io, child_page, target_leaf, new_path) {
                    println!(
                        "    通过分支 [{}] 找到: 最大值 {}_{} -> 页号 0x{:X}",
                        i, loc.refno_0, loc.refno_1, loc.pgno
                    );
                    return true;
                }
            }
        }
    }
    false
}

// 反向查找叶子节点在索引树中的路径
fn find_leaf_in_index_tree(io: &mut PdmsIO, target_leaf_page: u32, current_page: u32) -> bool {
    if let Ok(index_data) = io.read_index_data(current_page) {
        println!(
            "      🔍 检查页面 0x{:X} (层级 {})",
            current_page, index_data.level
        );

        if index_data.level == 0 {
            // 如果是叶子节点，检查是否是目标页面
            return current_page == target_leaf_page;
        } else {
            // 如果是非叶子节点，递归检查所有子页面
            for (i, loc) in index_data.refno_locs.iter().enumerate() {
                if find_leaf_in_index_tree(io, target_leaf_page, loc.pgno) {
                    println!(
                        "      ✅ 找到路径: 页面 0x{:X} -> 条目[{}] ({}_{}) -> 子页面 0x{:X}",
                        current_page, i, loc.refno_0, loc.refno_1, loc.pgno
                    );
                    return true;
                }
            }
        }
    }
    false
}

// 分析为什么找不到目标参考号
async fn analyze_missing_refno(io: &mut PdmsIO, target_refno: RefU64) {
    let (target_r0, target_r1) = (target_refno.get_0(), target_refno.get_1());

    println!("🔍 分析参考号 {}_{} 缺失的原因:", target_r0, target_r1);

    // 1. 检查是否在更新的会话中
    println!("📊 检查所有会话的参考号范围:");
    for (&sesno, &ses_pgno) in &io.sesno_pgno_map.clone() {
        if let Ok(ses_data) = io.read_ses_data(ses_pgno) {
            let root_pgno = ses_data.index_root_pageno;
            if let Ok(root_data) = io.read_index_data(root_pgno) {
                // 找到最大参考号
                let mut max_refno = (0u32, 0u32);
                for loc in &root_data.refno_locs {
                    if !(loc.refno_0 == 2147483649 && loc.refno_1 == 2147483649) {
                        if loc.refno_0 > max_refno.0
                            || (loc.refno_0 == max_refno.0 && loc.refno_1 > max_refno.1)
                        {
                            max_refno = (loc.refno_0, loc.refno_1);
                        }
                    }
                }

                if max_refno.0 >= target_r0 && max_refno.1 >= target_r1 {
                    println!(
                        "  会话 {}: 最大参考号 {}_{} ✅ 可能包含目标",
                        sesno, max_refno.0, max_refno.1
                    );
                } else {
                    println!(
                        "  会话 {}: 最大参考号 {}_{} ❌ 不包含目标",
                        sesno, max_refno.0, max_refno.1
                    );
                }
            }
        }
    }

    // 2. 检查是否存在索引间隙
    println!("\n🔍 检查索引连续性:");

    // 3. 验证用户发现的数据
    println!("\n🎯 验证用户发现的十六进制数据:");
    println!("位置 339:F5A0 的数据显示参考号 24383_101192 确实存在！");
    println!("这表明数据存在但B+树索引可能有问题");

    // 4. 搜索特定的文件位置
    let target_offset = 0x339F5A0; // 用户发现的叶子数据位置
    let upper_index_offset = 0x339EDE0; // 用户发现的上层索引位置
    println!("🧮 位置计算分析:");
    println!("  数据位置: 0x{:X}", target_offset);
    println!("  页面大小: 0x800 (2048 bytes)");

    let page_no = target_offset / 0x800;
    let offset_in_page = target_offset % 0x800;
    println!("  计算页号: 0x{:X} (十进制: {})", page_no, page_no);
    println!(
        "  页内偏移: 0x{:X} (十进制: {})",
        offset_in_page, offset_in_page
    );

    if let Ok(data) = io.read_bytes(target_offset, 16) {
        println!("📍 验证位置 0x{:X} 的数据:", target_offset);
        print!("原始字节: ");
        for byte in &data {
            print!("{:02X} ", byte);
        }
        println!();

        // 解析参考号 - 使用大端序
        if data.len() >= 16 {
            let r1 = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
            let r0 = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);
            println!("解析出的参考号: {}_{}", r0, r1);

            // 验证是否匹配目标参考号
            if r0 == target_r0 && r1 == target_r1 {
                println!("✅ 确认找到目标参考号！");
                println!("🔍 这说明数据存在但B+树索引可能不完整");

                // 进一步分析这个位置的数据结构
                println!("📊 详细数据分析:");
                println!("  位置: 0x{:X}", target_offset);
                println!("  所在页号: 0x{:X}", page_no);
                println!("  页内偏移: 0x{:X}", offset_in_page);
                println!(
                    "  r1 ({}): {:02X} {:02X} {:02X} {:02X}",
                    r1, data[0], data[1], data[2], data[3]
                );
                println!(
                    "  中间数据: {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
                    data[4], data[5], data[6], data[7], data[8], data[9], data[10], data[11]
                );
                println!(
                    "  r0 ({}): {:02X} {:02X} {:02X} {:02X}",
                    r0, data[12], data[13], data[14], data[15]
                );

                // 检查这个页面是否在B+树索引中
                println!("🔍 检查页号 0x{:X} 是否在索引中...", page_no);

                // 在1层索引中搜索这个页号
                search_page_in_level1_index(io, page_no as u32);
            }
        }
    }

    // 5. 验证用户发现的上层索引位置
    println!("\n🎯 验证用户发现的上层索引位置:");
    if let Ok(upper_data) = io.read_bytes(upper_index_offset, 16) {
        println!("📍 上层索引位置 0x{:X} 的数据:", upper_index_offset);
        print!("原始字节: ");
        for byte in &upper_data {
            print!("{:02X} ", byte);
        }
        println!();

        // 解析上层索引参考号 - 使用大端序
        if upper_data.len() >= 16 {
            let r1 =
                u32::from_be_bytes([upper_data[0], upper_data[1], upper_data[2], upper_data[3]]);
            let r0 = u32::from_be_bytes([
                upper_data[12],
                upper_data[13],
                upper_data[14],
                upper_data[15],
            ]);
            println!("解析出的上层索引参考号: {}_{}", r0, r1);

            if r0 == 24383 && r1 == 101059 {
                println!("✅ 确认找到上层索引参考号 24383_101059！");

                // 计算上层索引的页号
                let upper_page_no = upper_index_offset / 0x800;
                let upper_offset_in_page = upper_index_offset % 0x800;
                println!("📊 上层索引位置分析:");
                println!("  页号: 0x{:X} (十进制: {})", upper_page_no, upper_page_no);
                println!(
                    "  页内偏移: 0x{:X} (十进制: {})",
                    upper_offset_in_page, upper_offset_in_page
                );

                // 检查这个页号是否在索引中
                println!("🔍 检查上层索引页号 0x{:X} 是否在索引中...", upper_page_no);
                search_page_in_level1_index(io, upper_page_no as u32);

                // 分析两个位置的关系
                println!("\n🔗 分析索引层级关系:");
                println!(
                    "  叶子数据: 24383_101192 位于 0x{:X} (页号: 0x{:X})",
                    target_offset, page_no
                );
                println!(
                    "  上层索引: 24383_101059 位于 0x{:X} (页号: 0x{:X})",
                    upper_index_offset, upper_page_no
                );
                println!("  📊 这证明了层级索引结构的存在！");
            } else {
                println!("❌ 上层索引参考号不匹配");
                println!("  期望: 24383_101059");
                println!("  实际: {}_{}", r0, r1);
            }
        }
    } else {
        println!("❌ 无法读取上层索引位置的数据");
    }

    println!("\n特定参考号测试完成！");

    // 根据用户发现，分析正确的索引遍历方式
    println!("\n🎯 根据用户发现分析正确的索引遍历:");
    analyze_correct_index_traversal(io, target_refno).await;

    // 测试新的搜索策略：超出范围时选择最后一个条目
    println!("\n🎯 测试新的搜索策略 - 超出范围选择最后一个:");
    test_new_search_strategy(io, target_refno).await;
}

/// 在1层索引中搜索指定的页号
fn search_page_in_level1_index(io: &mut PdmsIO, target_page_no: u32) {
    println!("\n🔍 在1层索引中搜索页号 0x{:X}:", target_page_no);

    // 获取最新会话的索引根节点
    let latest_sesno = io.get_latest_sesno().unwrap();
    if let Ok(session_info) = io.get_ses_data(latest_sesno) {
        let root_page_no = session_info.index_root_pageno;
        println!("📊 从根节点 0x{:X} 开始搜索", root_page_no);

        // 读取根节点
        if let Ok(root_page) = io.read_index_data(root_page_no) {
            println!(
                "📄 根节点层级: {}, 条目数: {}",
                root_page.level,
                root_page.refno_locs.len()
            );

            // 注意：IndexPageData 不是B+树索引结构，而是参考号位置索引
            // 我们需要在 refno_locs 中搜索目标页号
            println!("🔍 在索引页中搜索目标页号 0x{:X}", target_page_no);

            // 同时搜索计算出的正确页号
            let correct_page_no = target_page_no - 1; // 0x673E
            println!("🔍 同时搜索计算出的页号 0x{:X}", correct_page_no);

            // 根据用户发现，搜索上层索引参考号 24383_101059
            let upper_index_refno = (24383u32, 101059u32);
            println!(
                "🔍 搜索用户发现的上层索引参考号: {}_{}",
                upper_index_refno.0, upper_index_refno.1
            );

            let mut found = false;
            for (i, refno_loc) in root_page.refno_locs.iter().enumerate() {
                // 搜索页号匹配
                if refno_loc.pgno == target_page_no || refno_loc.pgno == correct_page_no {
                    println!("  ✅ 找到目标页号！");
                    println!("    位置: 索引条目 {}", i);
                    println!("    参考号: {}_{}", refno_loc.refno_0, refno_loc.refno_1);
                    println!("    页号: 0x{:X}", refno_loc.pgno);
                    println!("    偏移: 0x{:X}", refno_loc.offset);
                    println!("    标志: 0x{:X}", refno_loc.flag);

                    // 检查是否是我们要找的参考号
                    if refno_loc.refno_0 == 24383 && refno_loc.refno_1 == 101192 {
                        println!("    🎯 这就是我们要找的参考号！");
                    }
                    found = true;
                }

                // 搜索上层索引参考号
                if refno_loc.refno_0 == upper_index_refno.0
                    && refno_loc.refno_1 == upper_index_refno.1
                {
                    println!("  🎯 找到用户发现的上层索引参考号！");
                    println!("    位置: 索引条目 {}", i);
                    println!("    参考号: {}_{}", refno_loc.refno_0, refno_loc.refno_1);
                    println!("    页号: 0x{:X}", refno_loc.pgno);
                    println!("    偏移: 0x{:X}", refno_loc.offset);
                    println!("    标志: 0x{:X}", refno_loc.flag);
                    found = true;
                }
            }

            if !found {
                println!("  ❌ 在当前索引页中未找到目标页号");

                // 显示页号范围
                if !root_page.refno_locs.is_empty() {
                    let page_numbers: Vec<u32> =
                        root_page.refno_locs.iter().map(|loc| loc.pgno).collect();
                    let min_page = page_numbers.iter().min().unwrap();
                    let max_page = page_numbers.iter().max().unwrap();
                    println!(
                        "  📋 当前索引页包含的页号范围: 0x{:X} 到 0x{:X}",
                        min_page, max_page
                    );

                    if target_page_no >= *min_page && target_page_no <= *max_page {
                        println!("  ⚠️  目标页号在范围内但未找到！");

                        // 显示前几个和后几个页号
                        let mut unique_pages: Vec<u32> = page_numbers.into_iter().collect();
                        unique_pages.sort();
                        unique_pages.dedup();

                        println!("  📋 前10个页号:");
                        for (i, &page) in unique_pages.iter().take(10).enumerate() {
                            let marker = if page == target_page_no {
                                " ← 目标"
                            } else {
                                ""
                            };
                            println!("    [{}] 0x{:X}{}", i, page, marker);
                        }

                        if unique_pages.len() > 10 {
                            println!("  ... (省略中间部分) ...");
                            println!("  📋 后10个页号:");
                            for (i, &page) in unique_pages.iter().rev().take(10).enumerate() {
                                let marker = if page == target_page_no {
                                    " ← 目标"
                                } else {
                                    ""
                                };
                                println!(
                                    "    [{}] 0x{:X}{}",
                                    unique_pages.len() - 10 + i,
                                    page,
                                    marker
                                );
                            }
                        }
                    } else {
                        println!("  ❌ 目标页号超出当前索引页范围");
                    }
                }
            }
        }
    }
}

/// 根据用户发现分析正确的索引遍历方式
async fn analyze_correct_index_traversal(io: &mut PdmsIO, target_refno: RefU64) {
    let (target_r0, target_r1) = (target_refno.get_0(), target_refno.get_1());

    println!(
        "🎯 分析正确的索引遍历方式 (目标: {}_{}):",
        target_r0, target_r1
    );

    // 读取根节点
    let root_pgno = 0x673F;
    if let Ok(root_data) = io.read_index_data(root_pgno) {
        println!(
            "📄 根节点 0x{:X}, 层级: {}, 条目数: {}",
            root_pgno,
            root_data.level,
            root_data.refno_locs.len()
        );

        // 分析索引条目，去除重复
        let mut unique_entries = Vec::new();
        let mut seen_refnos = std::collections::HashSet::new();

        for (i, entry) in root_data.refno_locs.iter().enumerate() {
            let refno_key = (entry.refno_0, entry.refno_1);

            if !seen_refnos.contains(&refno_key) {
                seen_refnos.insert(refno_key);
                unique_entries.push((i, entry));
                println!(
                    "  ✅ [{}] 唯一条目: {}_{} -> 页号: 0x{:X}",
                    i, entry.refno_0, entry.refno_1, entry.pgno
                );
            } else {
                println!(
                    "  ❌ [{}] 重复条目: {}_{} -> 页号: 0x{:X} (忽略)",
                    i, entry.refno_0, entry.refno_1, entry.pgno
                );
            }
        }

        println!(
            "\n📊 去重后的索引条目数: {} (原始: {})",
            unique_entries.len(),
            root_data.refno_locs.len()
        );

        // 使用去重后的条目进行搜索
        println!("\n🔍 使用去重索引进行搜索:");
        let mut selected_entry = None;

        for (original_idx, entry) in &unique_entries {
            if target_r0 < entry.refno_0
                || (target_r0 == entry.refno_0 && target_r1 <= entry.refno_1)
            {
                selected_entry = Some((*original_idx, *entry));
                println!(
                    "  🎯 选择条目 [{}]: {}_{} -> 页号: 0x{:X}",
                    original_idx, entry.refno_0, entry.refno_1, entry.pgno
                );
                break;
            }
        }

        // 如果没有找到合适的条目，选择最后一个
        if selected_entry.is_none() && !unique_entries.is_empty() {
            let (original_idx, entry) = unique_entries.last().unwrap();
            selected_entry = Some((*original_idx, *entry));
            println!(
                "  🎯 目标超出范围，选择最后一个条目 [{}]: {}_{} -> 页号: 0x{:X}",
                original_idx, entry.refno_0, entry.refno_1, entry.pgno
            );
        }

        // 继续搜索下一层
        if let Some((_, selected)) = selected_entry {
            println!("\n➡️  继续搜索子页号: 0x{:X}", selected.pgno);
            analyze_level1_index_with_deduplication(io, selected.pgno, target_refno).await;
        }
    } else {
        println!("❌ 无法读取根节点数据");
    }
}

/// 分析1层索引并去重
async fn analyze_level1_index_with_deduplication(
    io: &mut PdmsIO,
    page_no: u32,
    target_refno: RefU64,
) {
    let (target_r0, target_r1) = (target_refno.get_0(), target_refno.get_1());

    if let Ok(level1_data) = io.read_index_data(page_no) {
        println!(
            "📄 1层索引页号: 0x{:X}, 层级: {}, 条目数: {}",
            page_no,
            level1_data.level,
            level1_data.refno_locs.len()
        );

        // 去重处理
        let mut unique_entries = Vec::new();
        let mut seen_refnos = std::collections::HashSet::new();

        for (i, entry) in level1_data.refno_locs.iter().enumerate() {
            let refno_key = (entry.refno_0, entry.refno_1);

            if !seen_refnos.contains(&refno_key) {
                seen_refnos.insert(refno_key);
                unique_entries.push((i, entry));

                // 检查是否包含目标范围
                if target_r0 == entry.refno_0 && target_r1 <= entry.refno_1 {
                    println!(
                        "  🎯 [{}] 可能包含目标: {}_{} -> 页号: 0x{:X}",
                        i, entry.refno_0, entry.refno_1, entry.pgno
                    );
                }
            }
        }

        println!(
            "📊 1层索引去重后条目数: {} (原始: {})",
            unique_entries.len(),
            level1_data.refno_locs.len()
        );

        // 查找包含目标参考号的叶子节点
        for (original_idx, entry) in &unique_entries {
            if target_r0 == entry.refno_0 && target_r1 <= entry.refno_1 {
                println!(
                    "\n🍃 检查叶子节点 0x{:X} (来自条目 [{}]):",
                    entry.pgno, original_idx
                );
                check_leaf_node_for_target(io, entry.pgno, target_refno).await;
            }
        }
    } else {
        println!("❌ 无法读取1层索引页号: 0x{:X}", page_no);
    }
}

/// 检查叶子节点是否包含目标参考号
async fn check_leaf_node_for_target(io: &mut PdmsIO, page_no: u32, target_refno: RefU64) {
    let (target_r0, target_r1) = (target_refno.get_0(), target_refno.get_1());

    if let Ok(leaf_data) = io.read_index_data(page_no) {
        println!(
            "  📄 叶子节点: 0x{:X}, 层级: {}, 条目数: {}",
            page_no,
            leaf_data.level,
            leaf_data.refno_locs.len()
        );

        // 检查是否包含目标参考号
        let mut found = false;
        for (i, entry) in leaf_data.refno_locs.iter().enumerate() {
            if entry.refno_0 == target_r0 && entry.refno_1 == target_r1 {
                println!(
                    "    ✅ [{}] 找到目标参考号: {}_{} -> 页号: 0x{:X}",
                    i, entry.refno_0, entry.refno_1, entry.pgno
                );
                found = true;
                break;
            }
        }

        if !found {
            // 显示范围信息
            if !leaf_data.refno_locs.is_empty() {
                let first = &leaf_data.refno_locs[0];
                let last = &leaf_data.refno_locs[leaf_data.refno_locs.len() - 1];
                println!(
                    "    📋 叶子节点范围: {}_{} 到 {}_{}",
                    first.refno_0, first.refno_1, last.refno_0, last.refno_1
                );

                if target_r1 > last.refno_1 {
                    println!(
                        "    ⚠️  目标参考号 {}_{} 超出此叶子节点范围",
                        target_r0, target_r1
                    );
                }
            }
        }
    } else {
        println!("  ❌ 无法读取叶子节点: 0x{:X}", page_no);
    }
}

/// 检查是否为起始索引标记位
fn is_start_index_marker(refno_0: u32, refno_1: u32) -> bool {
    // 检查是否为起始索引标记: 0x80000001 0x80000001
    refno_0 == 0x80000001 && refno_1 == 0x80000001
}

/// 测试新的搜索策略：超出范围时选择最后一个条目
async fn test_new_search_strategy(io: &mut PdmsIO, target_refno: RefU64) {
    let (target_r0, target_r1) = (target_refno.get_0(), target_refno.get_1());

    println!("🎯 新搜索策略测试 (目标: {}_{}):", target_r0, target_r1);
    println!("📋 策略: 正确处理起始索引标记位，超出范围时选择最后一个条目");

    // 从根节点开始
    let root_pgno = 0x673F;
    if let Ok(root_data) = io.read_index_data(root_pgno) {
        println!(
            "📄 根节点 0x{:X}, 层级: {}, 条目数: {}",
            root_pgno,
            root_data.level,
            root_data.refno_locs.len()
        );

        // 去重处理，同时识别起始索引标记
        let mut unique_entries = Vec::new();
        let mut seen_refnos = std::collections::HashSet::new();
        let mut start_marker_entry = None;

        for (i, entry) in root_data.refno_locs.iter().enumerate() {
            if is_start_index_marker(entry.refno_0, entry.refno_1) {
                if start_marker_entry.is_none() {
                    start_marker_entry = Some((i, entry));
                    println!(
                        "  🏁 发现起始索引标记 [{}]: 0x{:08X}_0x{:08X} -> 页号: 0x{:X}",
                        i, entry.refno_0, entry.refno_1, entry.pgno
                    );
                }
                continue; // 跳过起始标记的去重检查
            }

            let refno_key = (entry.refno_0, entry.refno_1);
            if !seen_refnos.contains(&refno_key) {
                seen_refnos.insert(refno_key);
                unique_entries.push((i, entry));
            }
        }

        println!(
            "📊 去重后条目数: {} (原始: {})",
            unique_entries.len(),
            root_data.refno_locs.len()
        );
        if start_marker_entry.is_some() {
            println!("📊 发现起始索引标记，将在搜索时特殊处理");
        }

        // 新策略：查找合适的条目，考虑起始索引标记
        let mut selected_entry = None;

        // 首先检查是否应该使用起始索引标记
        if let Some((marker_idx, marker_entry)) = start_marker_entry {
            // 如果目标值小于第一个正常索引条目，使用起始标记
            if let Some((_, first_normal_entry)) = unique_entries.first() {
                if target_r0 < first_normal_entry.refno_0
                    || (target_r0 == first_normal_entry.refno_0
                        && target_r1 < first_normal_entry.refno_1)
                {
                    selected_entry = Some((marker_idx, marker_entry.clone()));
                    println!("  🎯 目标值小于第一个正常索引，选择起始标记 [{}]: 0x{:08X}_0x{:08X} -> 页号: 0x{:X}",
                        marker_idx, marker_entry.refno_0, marker_entry.refno_1, marker_entry.pgno);
                }
            }
        }

        // 如果没有选择起始标记，按正常逻辑查找
        if selected_entry.is_none() {
            // 首先尝试找到包含目标值的条目
            for (original_idx, entry) in &unique_entries {
                if target_r0 < entry.refno_0
                    || (target_r0 == entry.refno_0 && target_r1 <= entry.refno_1)
                {
                    selected_entry = Some((*original_idx, (*entry).clone()));
                    println!(
                        "  🎯 找到包含范围的条目 [{}]: {}_{} -> 页号: 0x{:X}",
                        original_idx, entry.refno_0, entry.refno_1, entry.pgno
                    );
                    break;
                }
            }

            // 如果没有找到包含的条目，选择最后一个（关键策略）
            if selected_entry.is_none() && !unique_entries.is_empty() {
                let (original_idx, entry) = unique_entries.last().unwrap();
                selected_entry = Some((*original_idx, (*entry).clone()));
                println!(
                    "  🎯 目标超出范围，选择最后一个条目 [{}]: {}_{} -> 页号: 0x{:X}",
                    original_idx, entry.refno_0, entry.refno_1, entry.pgno
                );
            }
        }

        // 继续搜索下一层
        if let Some((_, selected)) = selected_entry {
            println!("\n➡️  继续搜索子页号: 0x{:X}", selected.pgno);
            Box::pin(search_with_new_strategy(io, selected.pgno, target_refno, 1)).await;
        }
    } else {
        println!("❌ 无法读取根节点数据");
    }
}

/// 使用新策略递归搜索
async fn search_with_new_strategy(io: &mut PdmsIO, page_no: u32, target_refno: RefU64, depth: u32) {
    let (target_r0, target_r1) = (target_refno.get_0(), target_refno.get_1());

    if let Ok(page_data) = io.read_index_data(page_no) {
        println!(
            "📄 第{}层页号: 0x{:X}, 层级: {}, 条目数: {}",
            depth,
            page_no,
            page_data.level,
            page_data.refno_locs.len()
        );

        if page_data.level == 0 {
            // 叶子节点：直接搜索目标
            println!("🍃 到达叶子节点，搜索目标参考号");

            let mut found = false;
            for (i, entry) in page_data.refno_locs.iter().enumerate() {
                if entry.refno_0 == target_r0 && entry.refno_1 == target_r1 {
                    println!(
                        "    ✅ [{}] 找到目标参考号: {}_{} -> 页号: 0x{:X}",
                        i, entry.refno_0, entry.refno_1, entry.pgno
                    );
                    found = true;
                    break;
                }
            }

            if !found {
                // 显示叶子节点范围
                if !page_data.refno_locs.is_empty() {
                    let first = &page_data.refno_locs[0];
                    let last = &page_data.refno_locs[page_data.refno_locs.len() - 1];
                    println!(
                        "    📋 叶子节点范围: {}_{} 到 {}_{}",
                        first.refno_0, first.refno_1, last.refno_0, last.refno_1
                    );

                    if target_r0 == last.refno_0 && target_r1 > last.refno_1 {
                        println!(
                            "    ⚠️  目标参考号 {}_{} 超出此叶子节点最大值 {}_{}",
                            target_r0, target_r1, last.refno_0, last.refno_1
                        );
                        println!("    💡 这说明目标数据可能在更新的数据中，但索引未完全更新");
                    } else {
                        println!("    ❌ 目标参考号不在此叶子节点中");
                    }
                }
            }
        } else {
            // 非叶子节点：继续向下搜索
            println!("🌿 非叶子节点，继续向下搜索");

            // 去重处理，同时识别起始索引标记
            let mut unique_entries = Vec::new();
            let mut seen_refnos = std::collections::HashSet::new();
            let mut start_marker_entry = None;

            for (i, entry) in page_data.refno_locs.iter().enumerate() {
                if is_start_index_marker(entry.refno_0, entry.refno_1) {
                    if start_marker_entry.is_none() {
                        start_marker_entry = Some((i, entry));
                        println!(
                            "    🏁 发现起始索引标记 [{}]: 0x{:08X}_0x{:08X} -> 页号: 0x{:X}",
                            i, entry.refno_0, entry.refno_1, entry.pgno
                        );
                    }
                    continue; // 跳过起始标记的去重检查
                }

                let refno_key = (entry.refno_0, entry.refno_1);
                if !seen_refnos.contains(&refno_key) {
                    seen_refnos.insert(refno_key);
                    unique_entries.push((i, entry));
                }
            }

            println!(
                "📊 去重后条目数: {} (原始: {})",
                unique_entries.len(),
                page_data.refno_locs.len()
            );
            if start_marker_entry.is_some() {
                println!("    📊 发现起始索引标记，将在搜索时特殊处理");
            }

            // 查找合适的条目，考虑起始索引标记
            let mut selected_entry = None;

            // 首先检查是否应该使用起始索引标记
            if let Some((marker_idx, marker_entry)) = start_marker_entry {
                // 如果目标值小于第一个正常索引条目，使用起始标记
                if let Some((_, first_normal_entry)) = unique_entries.first() {
                    if target_r0 < first_normal_entry.refno_0
                        || (target_r0 == first_normal_entry.refno_0
                            && target_r1 < first_normal_entry.refno_1)
                    {
                        selected_entry = Some((marker_idx, marker_entry.clone()));
                        println!("    🎯 目标值小于第一个正常索引，选择起始标记 [{}]: 0x{:08X}_0x{:08X} -> 页号: 0x{:X}",
                            marker_idx, marker_entry.refno_0, marker_entry.refno_1, marker_entry.pgno);
                    }
                }
            }

            // 如果没有选择起始标记，按正常逻辑查找
            if selected_entry.is_none() {
                // 首先尝试找到包含目标值的条目
                for (original_idx, entry) in &unique_entries {
                    if target_r0 < entry.refno_0
                        || (target_r0 == entry.refno_0 && target_r1 <= entry.refno_1)
                    {
                        selected_entry = Some((*original_idx, (*entry).clone()));
                        println!(
                            "    🎯 找到包含范围的条目 [{}]: {}_{} -> 页号: 0x{:X}",
                            original_idx, entry.refno_0, entry.refno_1, entry.pgno
                        );
                        break;
                    }
                }

                // 如果没有找到包含的条目，选择最后一个（关键策略）
                if selected_entry.is_none() && !unique_entries.is_empty() {
                    let (original_idx, entry) = unique_entries.last().unwrap();
                    selected_entry = Some((*original_idx, (*entry).clone()));
                    println!(
                        "    🎯 目标超出范围，选择最后一个条目 [{}]: {}_{} -> 页号: 0x{:X}",
                        original_idx, entry.refno_0, entry.refno_1, entry.pgno
                    );
                }
            }

            // 继续搜索下一层
            if let Some((_, selected)) = selected_entry {
                println!("\n➡️  继续搜索子页号: 0x{:X}", selected.pgno);
                Box::pin(search_with_new_strategy(
                    io,
                    selected.pgno,
                    target_refno,
                    depth + 1,
                ))
                .await;
            }
        }
    } else {
        println!("❌ 无法读取页号: 0x{:X}", page_no);
    }
}

/// 传统B+树搜索算法（原始版本）
fn search_refno_in_btree_traditional(
    io: &mut PdmsIO,
    target_refno: &RefU64,
    latest_sesno: u32,
) -> Option<(u32, u32)> {
    let (target_r0, target_r1) = (target_refno.get_0(), target_refno.get_1());

    // 获取根节点页号
    if let Ok(session_info) = io.get_ses_data(latest_sesno) {
        let root_page_no = session_info.index_root_pageno;

        // 传统搜索：严格按照B+树规则，不处理起始标记和重复条目
        search_btree_traditional_recursive(io, root_page_no, target_r0, target_r1)
    } else {
        None
    }
}

/// 传统递归搜索函数
fn search_btree_traditional_recursive(
    io: &mut PdmsIO,
    page_no: u32,
    target_r0: u32,
    target_r1: u32,
) -> Option<(u32, u32)> {
    if let Ok(index_data) = io.read_index_data(page_no) {
        if index_data.level == 0 {
            // 叶子节点：直接搜索
            for loc in &index_data.refno_locs {
                if loc.refno_0 == target_r0 && loc.refno_1 == target_r1 {
                    return Some((loc.pgno, loc.offset));
                }
            }
            return None;
        } else {
            // 非叶子节点：找到第一个大于等于目标值的条目
            for loc in &index_data.refno_locs {
                if target_r0 < loc.refno_0 || (target_r0 == loc.refno_0 && target_r1 <= loc.refno_1)
                {
                    return search_btree_traditional_recursive(io, loc.pgno, target_r0, target_r1);
                }
            }
            // 如果没有找到合适的条目，返回None（传统算法不会继续搜索）
            return None;
        }
    }
    None
}

/// 优化B+树搜索算法（新版本）
fn search_refno_in_btree_optimized(
    io: &mut PdmsIO,
    target_refno: &RefU64,
    latest_sesno: u32,
) -> Option<(u32, u32)> {
    let (target_r0, target_r1) = (target_refno.get_0(), target_refno.get_1());

    println!(
        "🔍 开始B+树搜索: 目标参考号 {}_{}, 根页号 0x{:X}",
        target_r0, target_r1, latest_sesno
    );

    // 获取根节点页号
    if let Ok(session_info) = io.get_ses_data(latest_sesno) {
        let root_page_no = session_info.index_root_pageno;

        // 优化搜索：处理起始标记、去重、超出范围选择最后一个条目
        search_btree_optimized_recursive(io, root_page_no, target_r0, target_r1, Vec::new())
    } else {
        None
    }
}

/// 优化递归搜索函数
fn search_btree_optimized_recursive(
    io: &mut PdmsIO,
    page_no: u32,
    target_r0: u32,
    target_r1: u32,
    mut path: Vec<(u32, usize)>,
) -> Option<(u32, u32)> {
    if let Ok(index_data) = io.read_index_data(page_no) {
        println!(
            "📄 当前页号: 0x{:X}, 层级: {}, 条目数: {}",
            page_no,
            index_data.level,
            index_data.refno_locs.len()
        );

        if index_data.level == 0 {
            // 叶子节点
            println!("🍃 到达叶子节点，开始搜索目标参考号");

            if !index_data.refno_locs.is_empty() {
                let first = &index_data.refno_locs[0];
                let last = &index_data.refno_locs[index_data.refno_locs.len() - 1];
                println!(
                    "📋 叶子节点范围: {}_{} 到 {}_{}",
                    first.refno_0, first.refno_1, last.refno_0, last.refno_1
                );
            }

            println!("🔍 在叶子节点中搜索目标: {}_{}", target_r0, target_r1);

            // 在叶子节点中搜索目标参考号
            for (i, loc) in index_data.refno_locs.iter().enumerate() {
                if loc.refno_0 == target_r0 && loc.refno_1 == target_r1 {
                    println!(
                        "✅ [{}] 找到目标参考号: {}_{} -> 页号: 0x{:X}",
                        i, loc.refno_0, loc.refno_1, loc.pgno
                    );
                    return Some((loc.pgno, loc.offset));
                }
            }

            println!("❌ 未找到精确匹配");

            // 显示叶子节点内容用于调试
            println!("📋 叶子节点中包含的参考号范围:");
            let show_count = std::cmp::min(10, index_data.refno_locs.len());
            for (i, loc) in index_data.refno_locs.iter().take(show_count).enumerate() {
                println!(
                    "  前[{}] {}_{} -> 页号: 0x{:X}",
                    i, loc.refno_0, loc.refno_1, loc.pgno
                );
            }
            if index_data.refno_locs.len() > show_count {
                println!("  ... (省略中间部分) ...");
                let start_idx = index_data.refno_locs.len().saturating_sub(show_count);
                for (i, loc) in index_data.refno_locs.iter().skip(start_idx).enumerate() {
                    println!(
                        "  后[{}] {}_{} -> 页号: 0x{:X}",
                        start_idx + i,
                        loc.refno_0,
                        loc.refno_1,
                        loc.pgno
                    );
                }
            }

            // 检查是否需要回溯
            if let Some(last_loc) = index_data.refno_locs.last() {
                if target_r0 > last_loc.refno_0
                    || (target_r0 == last_loc.refno_0 && target_r1 > last_loc.refno_1)
                {
                    println!("🔄 目标值超出当前叶子节点范围，回溯到上级节点继续搜索");
                    return backtrack_and_continue_search(io, target_r0, target_r1, path);
                }
            }

            return None;
        } else {
            // 非叶子节点
            println!("🌿 非叶子节点，查找子页面");

            // 处理起始标记和去重
            let mut unique_entries = Vec::new();
            let mut seen_values = std::collections::HashSet::new();
            let mut has_start_marker = false;
            let mut start_marker_entry = None;

            for (original_idx, entry) in index_data.refno_locs.iter().enumerate() {
                // 检查起始标记
                if entry.refno_0 == 0x80000001 && entry.refno_1 == 0x80000001 {
                    has_start_marker = true;
                    start_marker_entry = Some((original_idx, entry.clone()));
                    continue;
                }

                // 去重处理
                let key = (entry.refno_0, entry.refno_1);
                if !seen_values.contains(&key) {
                    seen_values.insert(key);
                    unique_entries.push((original_idx, entry.clone()));
                }
            }

            println!("📋 非叶子节点所有条目:");
            for (i, entry) in index_data.refno_locs.iter().enumerate() {
                if i == 0 && entry.refno_0 == 0x80000001 && entry.refno_1 == 0x80000001 {
                    println!(
                        "  🏁 [{}] 起始标记: 0x{:X}_0x{:X} -> 子页号: 0x{:X}",
                        i, entry.refno_0, entry.refno_1, entry.pgno
                    );
                } else {
                    println!(
                        "  [{}] 最大值: {}_{} -> 子页号: 0x{:X}",
                        i, entry.refno_0, entry.refno_1, entry.pgno
                    );
                }
            }

            if has_start_marker {
                println!("📊 发现起始索引标记，将在搜索时特殊处理");
            }

            println!(
                "📊 去重后条目数: {} (原始: {})",
                unique_entries.len(),
                index_data.refno_locs.len()
            );

            // 搜索逻辑
            let mut selected_entry: Option<(usize, RefnoDataLoc)> = None;

            // 首先检查起始标记
            if let Some((marker_idx, marker_entry)) = start_marker_entry {
                if target_r0
                    < unique_entries
                        .first()
                        .map(|(_, e)| e.refno_0)
                        .unwrap_or(u32::MAX)
                {
                    println!(
                        "🎯 目标值小于第一个正常索引，选择起始标记: [{}] -> 页号: 0x{:X}",
                        marker_idx, marker_entry.pgno
                    );
                    selected_entry = Some((marker_idx, marker_entry.clone()));
                }
            }

            // 如果没有选择起始标记，在去重后的条目中搜索
            if selected_entry.is_none() {
                for (original_idx, entry) in &unique_entries {
                    if target_r0 < entry.refno_0
                        || (target_r0 == entry.refno_0 && target_r1 <= entry.refno_1)
                    {
                        println!(
                            "🎯 找到合适的分支: [{}] {}_{} -> 页号: 0x{:X}",
                            original_idx, entry.refno_0, entry.refno_1, entry.pgno
                        );
                        selected_entry = Some((*original_idx, entry.clone()));
                        break;
                    }
                }

                // 如果没有找到合适的分支，选择最后一个条目（关键优化）
                if selected_entry.is_none() && !unique_entries.is_empty() {
                    let (original_idx, entry) = &unique_entries[unique_entries.len() - 1];
                    println!(
                        "🎯 目标值超出范围，选择最后一个条目: [{}] {}_{} -> 页号: 0x{:X}",
                        original_idx, entry.refno_0, entry.refno_1, entry.pgno
                    );
                    selected_entry = Some((*original_idx, entry.clone()));
                }
            }

            // 继续搜索选中的子页面
            if let Some((selected_idx, selected)) = selected_entry {
                println!(
                    "➡️  选择子页号: 0x{:X} (索引: {})",
                    selected.pgno, selected_idx
                );
                path.push((page_no, selected_idx));
                return search_btree_optimized_recursive(
                    io,
                    selected.pgno,
                    target_r0,
                    target_r1,
                    path,
                );
            } else {
                println!("❌ 没有找到合适的子页面");
                return None;
            }
        }
    } else {
        println!("❌ 无法读取页号: 0x{:X}", page_no);
        None
    }
}

/// 回溯并继续搜索
fn backtrack_and_continue_search(
    io: &mut PdmsIO,
    target_r0: u32,
    target_r1: u32,
    mut path: Vec<(u32, usize)>,
) -> Option<(u32, u32)> {
    while let Some((parent_page, last_selected_idx)) = path.pop() {
        println!(
            "🔙 回溯到页号: 0x{:X}, 上次选择索引: {}",
            parent_page, last_selected_idx
        );

        if let Ok(parent_data) = io.read_index_data(parent_page) {
            // 尝试下一个分支
            let mut found_next = false;
            for (i, loc) in parent_data.refno_locs.iter().enumerate() {
                if i > last_selected_idx
                    && !(loc.refno_0 == 0x80000001 && loc.refno_1 == 0x80000001)
                {
                    println!(
                        "🔄 尝试下一个分支: [{}] {}_{} -> 页号: 0x{:X}",
                        i, loc.refno_0, loc.refno_1, loc.pgno
                    );
                    path.push((parent_page, i));
                    if let Some(result) = search_btree_optimized_recursive(
                        io,
                        loc.pgno,
                        target_r0,
                        target_r1,
                        path.clone(),
                    ) {
                        return Some(result);
                    }
                    path.pop(); // 移除刚添加的路径
                    found_next = true;
                    break;
                }
            }

            if !found_next {
                println!("❌ 当前层级没有更多分支，继续回溯");
            }
        }
    }

    println!("❌ 已回溯到根节点，未找到目标值");
    None
}

#[tokio::test]
async fn test_main_search_algorithm() {
    println!("🚀 测试主流程搜索算法（调试模式需要启用 debug_btree_search feature）");

    let db_option = get_db_option();
    let db_path = format!(
        "{}/AvevaMarineSample/ams000/ams7999_0001",
        db_option.project_path
    );
    let mut io = PdmsIO::new("test", &db_path, true);
    io.open().expect("无法打开数据库");

    // 测试之前验证成功的参考号
    let test_refnos = vec![
        RefU64::from_two_nums(24383, 101192), // 用户发现的参考号
        RefU64::from_two_nums(24383, 101200), // 另一个测试参考号
    ];

    for (i, target_refno) in test_refnos.iter().enumerate() {
        println!("\n🎯 测试参考号 {}: {}", i + 1, target_refno);

        let start_time = Instant::now();
        let result = io.search_latest_refno(*target_refno, None);
        let search_time = start_time.elapsed();

        println!(
            "⚡ 主流程搜索耗时: {:.4}ms",
            search_time.as_secs_f64() * 1000.0
        );
        println!("🔍 主流程搜索结果: {:?}", result);

        if result.is_some() {
            println!("✅ 主流程成功找到参考号 {}", target_refno);
        } else {
            println!("❌ 主流程未找到参考号 {}", target_refno);
        }
    }

    println!("\n🎉 主流程搜索算法测试完成！");
}
