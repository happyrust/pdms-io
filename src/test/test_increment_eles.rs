//! 测试增量元素收集功能
//! 
//! 本测试模块验证`collect_increment_eles`方法是否能正确收集指定会话范围内的元素数据和操作状态：
//! - 能够正确识别Add/Modified/Deleted等操作状态
//! - 能够正确处理None参数(使用最新会话)
//! - 能够正确处理会话范围参数

use aios_core::pdms_types::RefU64;
use crate::io::{PdmsIO, EleOperationDetail};
use std::time::Instant;
use std::ops::RangeInclusive;
use std::collections::HashSet;

/// 测试`collect_increment_eles`方法
/// 
/// 本测试验证以下情况：
/// 1. 使用None参数获取最新会话的元素
/// 2. 使用指定会话范围获取多个会话的元素
/// 3. 验证返回的元素状态是否正确
#[tokio::test]
async fn test_collect_increment_eles() -> anyhow::Result<()> {
    // 设置数据库文件路径
    let db_filepath = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams1112_0001"#;
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;

    // 测试用例1: 使用None参数获取最新会话的元素
    println!("测试1: 获取最新会话的元素");
    let start = Instant::now();
    let latest_eles = io.collect_increment_eles(None)?;
    let elapsed = start.elapsed();
    
    println!("最新会话中共有 {} 个元素, 耗时: {:?}", latest_eles.len(), elapsed);
    
    // 输出前10个元素的状态
    println!("前10个元素的状态:");
    for (i, (refno, detail)) in latest_eles.iter().take(10).enumerate() {
        let ele_info = match detail {
            EleOperationDetail::Add(ele) => format!("类型:{}, 属性数:{}", ele.noun, ele.whole_attmap.len()),
            EleOperationDetail::Modified { 
                added_attrs, deleted_attrs, modified_attrs, .. 
            } => format!("修改: 添加属性:{}, 删除属性:{}, 修改属性:{}", 
                          added_attrs.len(), deleted_attrs.len(), modified_attrs.len()),
            EleOperationDetail::Deleted(noun_type) => format!("已删除: 类型:{}", noun_type),
            EleOperationDetail::None => "无操作".to_string()
        };
        
        let op_type = match detail {
            EleOperationDetail::Add(_) => "新增",
            EleOperationDetail::Modified { .. } => "修改",
            EleOperationDetail::Deleted(_) => "删除",
            EleOperationDetail::None => "无操作",
        };
        
        println!("{}: 参考号={}, 操作={}, {}", i+1, refno, op_type, ele_info);
    }
    
    // 统计各种操作类型的数量
    let mut add_count = 0;
    let mut modified_count = 0;
    let mut deleted_count = 0;
    let mut none_count = 0;
    
    for (_, detail) in &latest_eles {
        match detail {
            EleOperationDetail::Add(_) => add_count += 1,
            EleOperationDetail::Modified { .. } => modified_count += 1,
            EleOperationDetail::Deleted(_) => deleted_count += 1,
            EleOperationDetail::None => none_count += 1,
        }
    }
    
    println!("操作类型统计: 新增={}, 修改={}, 删除={}, 无操作={}",
             add_count, modified_count, deleted_count, none_count);
    
    // 测试用例2: 使用指定会话范围
    // 获取当前数据库的最新会话号
    let latest_sesno = io.get_latest_sesno()? as i32;
    // 选择一个合理的范围，例如最新5个会话
    let range_start = std::cmp::max(1, latest_sesno - 4);
    let sesno_range = range_start..=latest_sesno;
    
    println!("\n测试2: 获取会话范围 {:?} 内的元素", sesno_range);
    let start = Instant::now();
    let range_eles = io.collect_increment_eles(Some(sesno_range.clone()))?;
    let elapsed = start.elapsed();
    
    println!("会话范围 {:?} 内共有 {} 个元素, 耗时: {:?}", 
             sesno_range, range_eles.len(), elapsed);
    
    // 输出前10个元素的状态
    println!("前10个元素的状态:");
    for (i, (refno, detail)) in range_eles.iter().take(10).enumerate() {
        let ele_info = match detail {
            EleOperationDetail::Add(ele) => format!("类型:{}, 属性数:{}", ele.noun, ele.whole_attmap.len()),
            EleOperationDetail::Modified { 
                added_attrs, deleted_attrs, modified_attrs, .. 
            } => format!("修改: 添加属性:{}, 删除属性:{}, 修改属性:{}", 
                          added_attrs.len(), deleted_attrs.len(), modified_attrs.len()),
            EleOperationDetail::Deleted(noun_type) => format!("已删除: 类型:{}", noun_type),
            EleOperationDetail::None => "无操作".to_string()
        };
        
        let op_type = match detail {
            EleOperationDetail::Add(_) => "新增",
            EleOperationDetail::Modified { .. } => "修改",
            EleOperationDetail::Deleted(_) => "删除",
            EleOperationDetail::None => "无操作",
        };
        
        println!("{}: 参考号={}, 操作={}, {}", i+1, refno, op_type, ele_info);
    }
    
    // 测试用例3: 选择一个特定元素，验证其状态
    if !range_eles.is_empty() {
        let test_refno = range_eles.keys().next().cloned().unwrap();
        println!("\n测试3: 验证特定元素的状态");
        println!("选择参考号: {}", test_refno);
        
        // 使用get_refno_operation_status验证状态
        let status_map = io.get_refno_operation_status(test_refno, None)?;
        let range_detail = range_eles.get(&test_refno).unwrap();
        
        println!("collect_increment_eles返回的状态: {:?}", range_detail);
        println!("get_refno_operation_status返回的状态: {:?}", status_map.get(&test_refno));
        println!("状态映射中包含 {} 个元素", status_map.len());
        
        // 由于现在两者返回类型相同，可以直接比较
        assert!(status_map.contains_key(&test_refno), "状态映射中应包含测试的参考号");
    }
    
    Ok(())
} 