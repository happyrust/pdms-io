//! Raphtory 集成测试
//!
//! 测试 PDMS 数据与 Raphtory 时间图数据库的集成功能

use crate::io::PdmsIO;
use crate::raphtory_integration::{RaphtoryIntegration, RaphtoryConfig, TimeUtils};
use aios_core::pdms_types::RefU64;
use std::collections::HashMap;
use std::time::Instant;

/// 测试基本的 Raphtory 集成功能
#[tokio::test]
async fn test_basic_raphtory_integration() -> anyhow::Result<()> {
    println!("🔧 开始测试基本 Raphtory 集成功能");

    // 创建配置
    let mut config = RaphtoryConfig::default();
    config.db_num = 7999;
    config.verbose_logging = true;
    config.graph_name = "test_pdms_graph".to_string();

    // 创建 Raphtory 集成实例
    let mut integration = RaphtoryIntegration::new(config);

    // 初始化
    integration.initialize()?;
    println!("✅ Raphtory 集成初始化成功");

    // 测试时间工具
    let session_no = 123;
    let timestamp = TimeUtils::session_to_timestamp(session_no);
    let converted_back = TimeUtils::timestamp_to_session(timestamp);
    assert_eq!(session_no, converted_back);
    println!("✅ 时间转换功能正常: {} <-> {}", session_no, timestamp);

    // 获取初始统计信息
    let initial_stats = integration.get_statistics();
    println!("📊 初始统计信息: {:?}", initial_stats);

    // 测试空数据存储
    let empty_elements = HashMap::new();
    integration.store_elements(&empty_elements)?;
    println!("✅ 空数据存储测试通过");

    // 完成存储
    integration.finalize_and_save().await?;
    println!("✅ 存储完成测试通过");

    Ok(())
}

/// 测试收集最新会话数据并存储到 Raphtory
#[tokio::test]
async fn test_collect_latest_session_with_raphtory() -> anyhow::Result<()> {
    println!("🔧 开始测试收集最新会话数据并存储到 Raphtory");

    // 注意：这里使用一个测试数据库路径，实际使用时请替换为有效路径
    let test_db_path = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7999_0001"#;
    
    // 检查测试数据库是否存在
    if !std::path::Path::new(test_db_path).exists() {
        println!("⚠️  测试数据库路径不存在，跳过实际数据测试: {}", test_db_path);
        
        // 进行模拟测试
        return test_mock_raphtory_integration().await;
    }

    let mut io = PdmsIO::new("ams", test_db_path, true);
    io.open()?;
    io.init_ses_range_map()?;

    println!("📂 数据库打开成功，开始收集数据");

    // 创建 Raphtory 配置
    let mut config = RaphtoryConfig::default();
    config.db_num = 7999;
    config.verbose_logging = true;
    config.batch_size = 100;

    // 创建 Raphtory 集成实例
    let mut integration = RaphtoryIntegration::new(config);

    let start_time = Instant::now();
    
    // 测试 collect_latest_eles_with_options 方法
    let max_sessions = Some(3); // 限制处理会话数量以加快测试
    let latest_elements = io.collect_latest_eles_with_options(max_sessions, Some(&mut integration)).await?;

    let elapsed = start_time.elapsed();
    
    println!("📊 收集完成:");
    println!("   - 处理时间: {:?}", elapsed);
    println!("   - 收集到的元素数量: {}", latest_elements.len());

    // 显示一些收集到的元素信息
    let mut count = 0;
    for (refno, operation_data) in latest_elements.iter() {
        if count >= 5 { // 只显示前5个
            break;
        }
        println!("   - 元素 {}: 会话号 {}", refno, operation_data.sesno);
        count += 1;
    }

    // 获取 Raphtory 统计信息
    let stats = integration.get_statistics();
    println!("📈 Raphtory 图统计信息:");
    for (key, value) in stats {
        println!("   - {}: {}", key, value);
    }

    // 获取时间范围
    if let Some((start, end)) = integration.get_time_range() {
        println!("⏰ 时间范围: {} 到 {} (会话 {} 到 {})", 
                start, end,
                TimeUtils::timestamp_to_session(start),
                TimeUtils::timestamp_to_session(end));
    }

    println!("✅ 测试完成");
    Ok(())
}

/// 模拟 Raphtory 集成测试（当没有真实数据库时使用）
async fn test_mock_raphtory_integration() -> anyhow::Result<()> {
    println!("🎭 开始模拟 Raphtory 集成测试");

    // 创建配置
    let mut config = RaphtoryConfig::default();
    config.db_num = 9999;
    config.verbose_logging = true;

    // 创建集成实例
    let mut integration = RaphtoryIntegration::new(config);
    integration.initialize()?;

    // 创建模拟数据
    let mut mock_elements = HashMap::new();
    
    for i in 1..=5 {
        let refno = RefU64::from_two_nums(17496, 100000 + i);
        let session_no = 100 + i as u32;
        
        // 创建模拟的操作数据
        let operation_data = crate::io::EleOperationData {
            refno,
            sesno: session_no,
            detail: crate::io::EleOperationDetail::None,
        };
        
        mock_elements.insert(refno, operation_data);
    }

    println!("📝 创建了 {} 个模拟元素", mock_elements.len());

    // 存储到 Raphtory
    integration.store_elements(&mock_elements)?;

    // 获取统计信息
    let stats = integration.get_statistics();
    println!("📊 模拟数据统计信息: {:?}", stats);

    // 测试历史查询功能
    let test_refno = RefU64::from_two_nums(17496, 100001);
    let test_timestamp = TimeUtils::session_to_timestamp(101);
    
    if let Some(historical_element) = integration.query_historical_state(test_refno, test_timestamp)? {
        println!("🔍 历史查询成功: 元素 {} 在时间 {} 的状态",
                historical_element.refno, historical_element.timestamp);
    } else {
        println!("🔍 历史查询: 在指定时间点未找到元素");
    }

    // 测试时间线查询
    let timeline = integration.get_element_timeline(test_refno)?;
    println!("📅 元素 {} 的时间线包含 {} 个记录", test_refno, timeline.len());

    // 完成存储
    integration.finalize_and_save().await?;

    println!("✅ 模拟测试完成");
    Ok(())
}

/// 测试 collect_and_save_to_raphtory 便捷方法
#[tokio::test]
async fn test_collect_and_save_to_raphtory() -> anyhow::Result<()> {
    println!("🔧 开始测试 collect_and_save_to_raphtory 便捷方法");

    let test_db_path = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7999_0001"#;
    
    if !std::path::Path::new(test_db_path).exists() {
        println!("⚠️  测试数据库路径不存在，跳过此测试: {}", test_db_path);
        return Ok(());
    }

    let mut io = PdmsIO::new("ams", test_db_path, true);
    io.open()?;
    io.init_ses_range_map()?;

    let start_time = Instant::now();
    
    // 使用便捷方法收集并保存数据
    let integration = io.collect_and_save_to_raphtory(Some(2), 7999).await?;
    
    let elapsed = start_time.elapsed();

    println!("📊 便捷方法执行完成:");
    println!("   - 执行时间: {:?}", elapsed);
    
    let stats = integration.get_statistics();
    println!("   - 图统计信息: {:?}", stats);

    if let Some((start, end)) = integration.get_time_range() {
        println!("   - 时间范围: {} 到 {}", start, end);
    }

    println!("✅ 便捷方法测试完成");
    Ok(())
}

/// 测试 Raphtory 配置功能
#[test]
fn test_raphtory_config() {
    println!("🔧 开始测试 Raphtory 配置功能");

    // 测试默认配置
    let default_config = RaphtoryConfig::default();
    assert_eq!(default_config.graph_name, "pdms_temporal_graph");
    assert_eq!(default_config.db_num, 0);
    assert_eq!(default_config.verbose_logging, false);
    assert_eq!(default_config.batch_size, 1000);

    println!("✅ 默认配置测试通过");

    // 测试自定义配置
    let mut custom_config = RaphtoryConfig::default();
    custom_config.graph_name = "custom_graph".to_string();
    custom_config.db_num = 12345;
    custom_config.verbose_logging = true;
    custom_config.batch_size = 500;

    assert_eq!(custom_config.graph_name, "custom_graph");
    assert_eq!(custom_config.db_num, 12345);
    assert_eq!(custom_config.verbose_logging, true);
    assert_eq!(custom_config.batch_size, 500);

    println!("✅ 自定义配置测试通过");

    // 测试使用 with_default_config
    let db_config = RaphtoryIntegration::with_default_config(98765);
    let config = db_config.get_config();
    assert_eq!(config.db_num, 98765);
    assert_eq!(config.graph_name, "pdms_db_98765_graph");

    println!("✅ with_default_config 测试通过");
    println!("✅ 配置功能测试完成");
}