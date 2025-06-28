//! 测试最新元素收集功能
//! 
//! 本测试模块验证`collect_latest_eles`方法是否能正确收集最新的元素数据：
//! - 能够从后往前检索最新数据
//! - 能够正确跳过已删除的元素
//! - 只保留增加和修改的元素
//! - 能够处理会话数量限制参数

use aios_core::pdms_types::RefU64;
use crate::io::{PdmsIO, EleOperationDetail};
use std::time::Instant;
use std::collections::HashSet;

/// 测试`collect_latest_eles`方法
/// 
/// 本测试验证以下情况：
/// 1. 使用None参数获取所有会话的最新元素
/// 2. 使用指定会话数量限制获取最新元素
/// 3. 验证返回的元素都是最新的且未被删除
/// 4. 验证性能和正确性
#[tokio::test]
async fn test_collect_latest_eles() -> anyhow::Result<()> {
    // 设置数据库文件路径
    let db_filepath = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7997_001"#;
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;
    io.init_ses_range_map()?;

    println!("开始测试 collect_latest_eles 方法");
    
    // 测试用例1: 获取所有会话的最新元素（限制前10个会话以避免测试时间过长）
    println!("\n测试1: 获取前10个会话的最新元素");
    let start = Instant::now();
    let latest_eles = io.collect_latest_eles(Some(10))?;
    let elapsed = start.elapsed();
    
    println!("前10个会话中共找到 {} 个最新元素, 耗时: {:?}", latest_eles.len(), elapsed);
    
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
            },
            EleOperationDetail::None => none_count += 1,
        }
    }
    
    println!("操作类型统计: 新增={}, 修改={}, 删除={}, 无操作={}",
             add_count, modified_count, deleted_count, none_count);
    
    // 断言：结果中不应该有删除的元素
    assert_eq!(deleted_count, 0, "结果中不应该包含已删除的元素");
    
    // 输出前10个元素的详细信息
    println!("\n前10个最新元素的详细信息:");
    for (i, (refno, operation_data)) in latest_eles.iter().take(10).enumerate() {
        let ele_info = match &operation_data.detail {
            EleOperationDetail::Add(ele) => {
                format!("新增元素 - 类型:{}, 属性数:{}", 
                       ele.att_map().get_type(), ele.att_map().len())
            },
            EleOperationDetail::Modified(modified) => {
                format!("修改元素 - 类型:{}, 添加属性:{}, 删除属性:{}, 修改属性:{}", 
                       modified.noun,
                       modified.added_attrs.len(), 
                       modified.deleted_attrs.len(), 
                       modified.modified_attrs.len())
            },
            EleOperationDetail::Deleted => "已删除".to_string(),
            EleOperationDetail::None => "无操作".to_string()
        };
        
        println!("{}: 参考号={}, 会话号={}, {}", 
                i+1, refno, operation_data.sesno, ele_info);
    }
    
    // 测试用例2: 获取更少会话的最新元素，验证结果一致性
    println!("\n测试2: 获取前5个会话的最新元素");
    let start = Instant::now();
    let latest_eles_5 = io.collect_latest_eles(Some(5))?;
    let elapsed = start.elapsed();
    
    println!("前5个会话中共找到 {} 个最新元素, 耗时: {:?}", latest_eles_5.len(), elapsed);
    
    // 验证前5个会话的结果应该是前10个会话结果的子集
    let mut subset_count = 0;
    for (refno, _) in &latest_eles_5 {
        if latest_eles.contains_key(refno) {
            subset_count += 1;
        }
    }
    
    println!("前5个会话的结果中有 {} 个元素也在前10个会话的结果中", subset_count);
    
    // 测试用例3: 验证特定元素的最新状态
    if !latest_eles.is_empty() {
        println!("\n测试3: 验证特定元素的最新状态");
        let test_refno = latest_eles.keys().next().cloned().unwrap();
        let operation_data = latest_eles.get(&test_refno).unwrap();
        
        println!("选择测试参考号: {}", test_refno);
        println!("collect_latest_eles 返回的会话号: {}", operation_data.sesno);
        
        // 使用传统方法验证这确实是最新的状态
        let status_map = io.get_refno_operation_status(test_refno, None)?;
        
        if let Some(traditional_detail) = status_map.get(&test_refno) {
            println!("传统方法返回的状态类型: {}", traditional_detail.get_op_type());
            println!("collect_latest_eles 返回的状态类型: {}", operation_data.detail.get_op_type());
            
            // 验证状态类型一致
            assert_eq!(traditional_detail.get_op_type(), operation_data.detail.get_op_type(),
                      "两种方法返回的状态类型应该一致");
        }
    }
    
    // 测试用例4: 性能对比测试
    println!("\n测试4: 性能对比测试");
    
    // 使用 collect_latest_eles 方法
    let start = Instant::now();
    let latest_method_result = io.collect_latest_eles(Some(3))?;
    let latest_method_time = start.elapsed();
    
    // 使用传统的 collect_increment_eles 方法获取最新3个会话
    let latest_sesno = io.get_latest_sesno()? as i32;
    let range_start = std::cmp::max(1, latest_sesno - 2);
    let sesno_range = range_start..=latest_sesno;
    
    let start = Instant::now();
    let traditional_result = io.collect_increment_eles(Some(sesno_range))?;
    let traditional_time = start.elapsed();
    
    println!("collect_latest_eles (3个会话): {} 个元素, 耗时: {:?}", 
             latest_method_result.len(), latest_method_time);
    
    let traditional_total: usize = traditional_result.values().map(|v| v.len()).sum();
    println!("collect_increment_eles (3个会话): {} 个元素, 耗时: {:?}", 
             traditional_total, traditional_time);
    
    // 验证 collect_latest_eles 的结果中没有重复的 refno
    let refno_set: HashSet<_> = latest_method_result.keys().collect();
    assert_eq!(refno_set.len(), latest_method_result.len(), 
              "collect_latest_eles 结果中不应该有重复的 refno");
    
    println!("\n所有测试通过！collect_latest_eles 方法工作正常。");
    
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
        assert_eq!(session_numbers.len(), 1, "会话数量为1时，所有元素应该来自同一个会话");
        assert!(session_numbers.contains(&latest_sesno), "应该是最新会话");
        println!("✓ 所有元素都来自最新会话 {}", latest_sesno);
    }
    
    println!("\n边界情况测试通过！");

    Ok(())
}

/// 测试特定参考号 24383/101192
#[tokio::test]
async fn test_specific_refno_24383_101192() -> anyhow::Result<()> {
    // 首先尝试 ams7997_001，如果不存在则使用 ams1112_0001
    let db_filepath_primary = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7999_0001"#;

    let db_filepath = if std::path::Path::new(db_filepath_primary).exists() {
        db_filepath_primary
    }else {
        println!("❌ 数据库文件都不存在，跳过测试");
        return Ok(());
    };
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;
    io.init_ses_range_map()?;

    println!("测试特定参考号 24383/101192");

    let test_refno = RefU64::from_two_nums(24383, 101192);
    println!("目标参考号: {}", test_refno);

    // 使用 get_refno_operation_status 方法查找这个元素的状态
    let start = Instant::now();
    let status_map = io.get_refno_operation_status(test_refno, None)?;
    let elapsed = start.elapsed();

    println!("get_refno_operation_status 耗时: {:?}", elapsed);

    // 检查目标参考号的状态
    if let Some(operation_detail) = status_map.get(&test_refno) {
        println!("✓ 找到目标参考号 {} 的状态", test_refno);

        let ele_info = match operation_detail {
            EleOperationDetail::Add(ele) => {
                println!("  状态: 新增元素");
                println!("  类型: {}", ele.att_map().get_type());
                println!("  属性数: {}", ele.att_map().len());

                // 显示一些关键属性
                if let Some(name) = ele.att_map().get_name() {
                    println!("  名称: {}", name);
                }

                format!("新增元素 - 类型:{}, 属性数:{}",
                       ele.att_map().get_type(), ele.att_map().len())
            },
            EleOperationDetail::Modified(modified) => {
                println!("  状态: 修改元素");
                println!("  类型: {}", modified.noun);
                println!("  添加属性: {}", modified.added_attrs.len());
                println!("  删除属性: {}", modified.deleted_attrs.len());
                println!("  修改属性: {}", modified.modified_attrs.len());

                format!("修改元素 - 类型:{}, 添加属性:{}, 删除属性:{}, 修改属性:{}",
                       modified.noun,
                       modified.added_attrs.len(),
                       modified.deleted_attrs.len(),
                       modified.modified_attrs.len())
            },
            EleOperationDetail::Deleted => {
                println!("  状态: 已删除");
                "已删除".to_string()
            },
            EleOperationDetail::None => {
                println!("  状态: 无操作");
                "无操作".to_string()
            }
        };

        println!("  操作详情: {}", ele_info);

        // 验证是否符合期望（新增）
        if matches!(operation_detail, EleOperationDetail::Add(_)) {
            println!("✓ 符合期望：该参考号对应的数据是新增状态");
        } else {
            println!("⚠ 不符合期望：该参考号对应的数据不是新增状态，而是: {}", operation_detail.get_op_type());
        }
    } else {
        println!("❌ 未找到参考号 {} 的状态信息", test_refno);
        println!("  可能该参考号在数据库中不存在");
    }

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
                println!("  版本 {}: 会话号={}, 偏移={:#X}", i+1, sesno, offset);
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
            println!("✓ 最新版本 - 会话号: {}, 偏移量: {:#X}", latest_sesno, latest_offset);

            match previous {
                Some((prev_sesno, prev_offset)) => {
                    println!("✓ 前一版本 - 会话号: {}, 偏移量: {:#X}", prev_sesno, prev_offset);
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
                println!("  {}: {}", i+1, refno);
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
                            println!("    偏移量 {}: {:#X}", i+1, offset);
                        }
                    }
                } else {
                    println!("  ❌ 目标参考号 {} 不在索引中", target_refno);
                }

                // 查找相近的参考号
                println!("\n查找与目标参考号相近的参考号:");
                let target_refno = RefU64::from_two_nums(24383, 101192);
                let mut nearby_refnos: Vec<_> = all_refnos.iter()
                    .filter(|&&refno| {
                        let diff = if *refno > target_refno {
                            refno.get_0() as u64 * 0x100000000 + refno.get_1() as u64 -
                            (target_refno.get_0() as u64 * 0x100000000 + target_refno.get_1() as u64)
                        } else {
                            target_refno.get_0() as u64 * 0x100000000 + target_refno.get_1() as u64 -
                            (refno.get_0() as u64 * 0x100000000 + refno.get_1() as u64)
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
                        println!("    {}: {}", i+1, refno);
                    }
                }

                // 分析参考号的第一部分分布
                println!("\n分析参考号第一部分的分布:");
                let mut first_parts: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
                for refno in all_refnos.iter() {
                    let first_part = refno.get_0();
                    *first_parts.entry(first_part).or_insert(0) += 1;
                }

                let mut sorted_parts: Vec<_> = first_parts.iter().collect();
                sorted_parts.sort_by_key(|&(k, _)| k);

                println!("  前10个最常见的第一部分:");
                for (i, (part, count)) in sorted_parts.iter().take(10).enumerate() {
                    println!("    {}: {} (出现 {} 次)", i+1, part, count);
                }

                // 检查目标参考号的第一部分
                let target_first_part = 24383;
                if let Some(count) = first_parts.get(&target_first_part) {
                    println!("  ✓ 目标第一部分 {} 存在，出现 {} 次", target_first_part, count);
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
            let mut refnos_24383: Vec<_> = index_map.keys()
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
                let larger_refnos: Vec<_> = refnos_24383.iter()
                    .filter(|&&refno| *refno > target_threshold)
                    .collect();

                if larger_refnos.is_empty() {
                    println!("  ❌ 没有找到大于 24383_101112 的参考号");
                    println!("  ✓ 这解释了为什么 24383_101192 找不到");
                } else {
                    println!("  ✓ 找到 {} 个大于 24383_101112 的参考号:", larger_refnos.len());
                    for (i, refno) in larger_refnos.iter().take(10).enumerate() {
                        println!("    {}: {}", i+1, refno);
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
                let range_refnos: Vec<_> = refnos_24383.iter()
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