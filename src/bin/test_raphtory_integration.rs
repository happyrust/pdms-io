//! 测试 Raphtory 集成功能的可执行程序
//!
//! 用法: cargo run --bin test_raphtory_integration [数据库路径] [最大会话数]

use pdms_io::io::PdmsIO;
use pdms_io::raphtory_integration::{RaphtoryIntegration, RaphtoryConfig};
use aios_core::pdms_types::RefU64;
use std::time::Instant;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    pdms_io::init_log(log::LevelFilter::Info).unwrap();

    println!("🚀 PDMS-Raphtory 集成测试程序");
    println!("=====================================");

    // 解析命令行参数
    let args: Vec<String> = env::args().collect();
    
    let (db_path, max_sessions) = if args.len() >= 2 {
        let db_path = &args[1];
        let max_sessions = if args.len() >= 3 {
            args[2].parse::<u32>().ok()
        } else {
            Some(5) // 默认处理5个会话
        };
        (db_path.clone(), max_sessions)
    } else {
        // 使用默认测试路径
        let default_path = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7999_0001"#;
        println!("使用默认数据库路径: {}", default_path);
        (default_path.to_string(), Some(3))
    };

    println!("📂 数据库路径: {}", db_path);
    println!("📊 最大会话数: {:?}", max_sessions);
    println!();

    // 检查数据库文件是否存在
    if !std::path::Path::new(&db_path).exists() {
        println!("❌ 数据库文件不存在: {}", db_path);
        println!("💡 请提供有效的 PDMS 数据库路径作为第一个参数");
        println!("   用法: cargo run --bin test_raphtory_integration <数据库路径> [最大会话数]");
        
        // 运行模拟测试
        println!("\n🎭 运行模拟测试代替...");
        return run_mock_test().await;
    }

    // 运行实际测试
    run_real_test(&db_path, max_sessions).await
}

/// 运行真实数据库测试
async fn run_real_test(db_path: &str, max_sessions: Option<u32>) -> anyhow::Result<()> {
    println!("🔧 开始真实数据库测试");
    
    let total_start = Instant::now();

    // 1. 初始化 PDMS IO
    println!("\n📖 步骤 1: 初始化 PDMS 数据库连接");
    let mut io = PdmsIO::new("ams", db_path, true);
    io.open()?;
    io.init_ses_range_map()?;
    
    let latest_sesno = io.get_latest_sesno()?;
    println!("   ✅ 数据库连接成功");
    println!("   📈 最新会话号: {}", latest_sesno);
    println!("   📊 会话范围映射: {} 个会话", io.ses_range_map.len());

    // 2. 测试基础 collect_latest_eles 功能
    println!("\n📊 步骤 2: 测试基础数据收集功能");
    let collect_start = Instant::now();
    let basic_elements = io.collect_latest_eles(max_sessions).await?;
    let collect_elapsed = collect_start.elapsed();
    
    println!("   ✅ 基础收集完成");
    println!("   ⏱️  耗时: {:?}", collect_elapsed);
    println!("   📦 收集到 {} 个元素", basic_elements.len());

    // 显示一些示例元素
    let mut count = 0;
    for (refno, operation_data) in basic_elements.iter() {
        if count >= 3 {
            println!("   📝 ... 还有 {} 个元素", basic_elements.len() - 3);
            break;
        }
        println!("   📝 元素 {}: 会话 {}, 操作: {:?}", 
                refno, operation_data.sesno, 
                match &operation_data.detail {
                    pdms_io::io::EleOperationDetail::Add(_) => "新增",
                    pdms_io::io::EleOperationDetail::Modified(_) => "修改",
                    pdms_io::io::EleOperationDetail::Deleted => "删除",
                    pdms_io::io::EleOperationDetail::None => "无操作",
                });
        count += 1;
    }

    // 3. 测试 Raphtory 集成功能
    println!("\n🔗 步骤 3: 测试 Raphtory 集成功能");
    
    // 创建配置
    let mut config = RaphtoryConfig::default();
    config.db_num = 7999;
    config.verbose_logging = true;
    config.batch_size = 500;
    config.graph_name = format!("pdms_test_graph_{}", chrono::Utc::now().timestamp());
    
    println!("   🔧 Raphtory 配置:");
    println!("      - 图名称: {}", config.graph_name);
    println!("      - 数据库编号: {}", config.db_num);
    println!("      - 批处理大小: {}", config.batch_size);

    let mut integration = RaphtoryIntegration::new(config);
    
    // 测试 collect_latest_eles_with_options
    let raphtory_start = Instant::now();
    let raphtory_elements = io.collect_latest_eles_with_options(max_sessions, Some(&mut integration)).await?;
    let raphtory_elapsed = raphtory_start.elapsed();
    
    println!("   ✅ Raphtory 集成收集完成");
    println!("   ⏱️  耗时: {:?}", raphtory_elapsed);
    println!("   📦 处理了 {} 个元素", raphtory_elements.len());

    // 4. 分析 Raphtory 图统计信息
    println!("\n📈 步骤 4: 分析 Raphtory 图统计信息");
    let stats = integration.get_statistics();
    for (key, value) in &stats {
        println!("   📊 {}: {}", key, value);
    }

    if let Some((start_time, end_time)) = integration.get_time_range() {
        println!("   ⏰ 时间范围: {} 到 {}", start_time, end_time);
        println!("   📅 对应会话: {} 到 {}", 
                pdms_io::raphtory_integration::TimeUtils::timestamp_to_session(start_time),
                pdms_io::raphtory_integration::TimeUtils::timestamp_to_session(end_time));
    }

    // 5. 测试历史查询功能
    println!("\n🔍 步骤 5: 测试历史查询功能");
    if !raphtory_elements.is_empty() {
        // 取第一个元素进行测试
        let (test_refno, test_data) = raphtory_elements.iter().next().unwrap();
        let test_timestamp = pdms_io::raphtory_integration::TimeUtils::session_to_timestamp(test_data.sesno as i32);
        
        println!("   🎯 测试元素: {}", test_refno);
        
        // 历史状态查询
        match integration.query_historical_state(*test_refno, test_timestamp)? {
            Some(historical) => {
                println!("   ✅ 历史查询成功:");
                println!("      - 元素: {}", historical.refno);
                println!("      - 时间戳: {}", historical.timestamp);
                println!("      - 会话号: {}", historical.session_no);
            }
            None => {
                println!("   ⚠️  在指定时间戳未找到元素状态");
            }
        }

        // 时间线查询
        let timeline = integration.get_element_timeline(*test_refno)?;
        println!("   📅 元素时间线: {} 个状态记录", timeline.len());
        
        for (i, item) in timeline.iter().take(3).enumerate() {
            println!("      {}. 时间戳: {}, 会话: {}", i + 1, item.timestamp, item.session_no);
        }
        if timeline.len() > 3 {
            println!("      ... 还有 {} 个状态记录", timeline.len() - 3);
        }
    }

    // 6. 测试便捷方法
    println!("\n🚀 步骤 6: 测试便捷方法");
    let convenience_start = Instant::now();
    let convenience_integration = io.collect_and_save_to_raphtory(max_sessions, 7999).await?;
    let convenience_elapsed = convenience_start.elapsed();
    
    println!("   ✅ 便捷方法执行完成");
    println!("   ⏱️  耗时: {:?}", convenience_elapsed);
    
    let convenience_stats = convenience_integration.get_statistics();
    println!("   📊 便捷方法统计: {:?}", convenience_stats);

    // 总结
    let total_elapsed = total_start.elapsed();
    println!("\n🎉 测试完成总结");
    println!("================");
    println!("⏱️  总执行时间: {:?}", total_elapsed);
    println!("📊 基础收集: {} 个元素，耗时 {:?}", basic_elements.len(), collect_elapsed);
    println!("🔗 Raphtory 集成: {} 个元素，耗时 {:?}", raphtory_elements.len(), raphtory_elapsed);
    println!("🚀 便捷方法: 耗时 {:?}", convenience_elapsed);
    println!("✅ 所有测试均通过！");

    Ok(())
}

/// 运行模拟测试
async fn run_mock_test() -> anyhow::Result<()> {
    println!("🎭 运行模拟 Raphtory 集成测试");
    
    let start_time = Instant::now();

    // 创建配置
    let mut config = RaphtoryConfig::default();
    config.db_num = 9999;
    config.verbose_logging = true;
    config.graph_name = "mock_test_graph".to_string();

    let mut integration = RaphtoryIntegration::new(config);
    integration.initialize()?;

    // 创建模拟数据
    let mut mock_elements = std::collections::HashMap::new();
    
    println!("📝 创建模拟元素数据...");
    for i in 1..=10 {
        let refno = RefU64::from_two_nums(17496, 200000 + i);
        let session_no = 500 + i as u32;
        
        let operation_data = pdms_io::io::EleOperationData {
            refno,
            sesno: session_no,
            detail: pdms_io::io::EleOperationDetail::None,
        };
        
        mock_elements.insert(refno, operation_data);
        println!("   📦 模拟元素 {}: 会话 {}", refno, session_no);
    }

    // 存储到 Raphtory
    println!("\n🔗 存储到 Raphtory 图数据库...");
    integration.store_elements(&mock_elements)?;

    // 获取统计信息
    let stats = integration.get_statistics();
    println!("\n📊 图统计信息:");
    for (key, value) in &stats {
        println!("   {}: {}", key, value);
    }

    // 测试查询功能
    println!("\n🔍 测试查询功能...");
    let test_refno = RefU64::from_two_nums(17496, 200001);
    let test_timestamp = pdms_io::raphtory_integration::TimeUtils::session_to_timestamp(501);
    
    if let Some(historical) = integration.query_historical_state(test_refno, test_timestamp)? {
        println!("   ✅ 历史查询成功: 元素 {} 在时间 {}", historical.refno, historical.timestamp);
    }

    let timeline = integration.get_element_timeline(test_refno)?;
    println!("   📅 时间线查询: {} 个记录", timeline.len());

    // 完成
    integration.finalize_and_save().await?;
    
    let elapsed = start_time.elapsed();
    println!("\n✅ 模拟测试完成，耗时: {:?}", elapsed);

    Ok(())
}