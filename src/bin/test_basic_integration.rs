//! 基础集成测试，不依赖 Raphtory 图功能
//! 
//! 测试基本的数据结构和 collect_latest_eles 功能

use pdms_io::io::PdmsIO;
use aios_core::pdms_types::RefU64;
use std::time::Instant;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🚀 PDMS 基础集成测试程序");
    println!("================================");

    // 解析命令行参数
    let args: Vec<String> = env::args().collect();
    let db_path = if args.len() >= 2 {
        args[1].clone()
    } else {
        r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams7999_0001"#.to_string()
    };

    println!("📂 数据库路径: {}", db_path);

    // 检查数据库文件是否存在
    if !std::path::Path::new(&db_path).exists() {
        println!("❌ 数据库文件不存在: {}", db_path);
        println!("💡 请提供有效的 PDMS 数据库路径作为参数");
        
        // 运行模拟测试
        return run_mock_test().await;
    }

    // 运行真实数据库测试
    run_real_test(&db_path).await
}

/// 运行真实数据库测试
async fn run_real_test(db_path: &str) -> anyhow::Result<()> {
    println!("\n🔧 开始真实数据库测试");
    
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
    let max_sessions = Some(3); // 限制处理会话数量
    let elements = io.collect_latest_eles(max_sessions).await?;
    let collect_elapsed = collect_start.elapsed();
    
    println!("   ✅ 数据收集完成");
    println!("   ⏱️  耗时: {:?}", collect_elapsed);
    println!("   📦 收集到 {} 个元素", elements.len());

    // 显示一些示例元素
    let mut count = 0;
    for (refno, operation_data) in elements.iter() {
        if count >= 5 {
            println!("   📝 ... 还有 {} 个元素", elements.len() - 5);
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

    // 3. 测试数据结构和类型转换
    println!("\n🔄 步骤 3: 测试数据结构和类型转换");
    
    if !elements.is_empty() {
        let (test_refno, test_data) = elements.iter().next().unwrap();
        
        // 测试时间转换
        let session_timestamp = pdms_io::raphtory_integration::TimeUtils::session_to_timestamp(test_data.sesno as i32);
        let back_to_session = pdms_io::raphtory_integration::TimeUtils::timestamp_to_session(session_timestamp);
        
        println!("   ✅ 时间转换测试:");
        println!("      会话号: {} -> 时间戳: {} -> 会话号: {}", 
                test_data.sesno, session_timestamp, back_to_session);
        
        // 测试历史元素结构
        let historical = pdms_io::raphtory_integration::HistoricalElement {
            refno: *test_refno,
            timestamp: session_timestamp,
            session_no: test_data.sesno as i32,
            data: test_data.clone(),
        };
        
        println!("   ✅ HistoricalElement 创建成功:");
        println!("      元素: {}, 时间戳: {}", historical.refno, historical.timestamp);
    }

    // 4. 测试配置功能
    println!("\n⚙️  步骤 4: 测试配置功能");
    
    let config = pdms_io::raphtory_integration::RaphtoryConfig::default();
    println!("   ✅ 默认配置:");
    println!("      图名称: {}", config.graph_name);
    println!("      数据库编号: {}", config.db_num);
    
    let custom_config = pdms_io::raphtory_integration::RaphtoryConfig {
        graph_name: "test_pdms_graph".to_string(),
        db_num: 7999,
        verbose_logging: true,
        batch_size: 500,
    };
    
    println!("   ✅ 自定义配置:");
    println!("      图名称: {}", custom_config.graph_name);
    println!("      数据库编号: {}", custom_config.db_num);
    println!("      详细日志: {}", custom_config.verbose_logging);

    // 总结
    let total_elapsed = total_start.elapsed();
    println!("\n🎉 测试完成总结");
    println!("================");
    println!("⏱️  总执行时间: {:?}", total_elapsed);
    println!("📊 处理了 {} 个元素", elements.len());
    println!("✅ 所有基础功能测试通过！");

    Ok(())
}

/// 运行模拟测试
async fn run_mock_test() -> anyhow::Result<()> {
    println!("\n🎭 运行模拟测试");
    
    // 测试配置功能
    println!("\n📋 测试配置功能");
    let config = pdms_io::raphtory_integration::RaphtoryConfig::default();
    println!("   ✅ 默认配置: 图名称={}, 数据库编号={}", config.graph_name, config.db_num);

    // 测试时间转换
    println!("\n⏰ 测试时间转换");
    for i in 1..=5 {
        let session = i * 100;
        let timestamp = pdms_io::raphtory_integration::TimeUtils::session_to_timestamp(session);
        let back = pdms_io::raphtory_integration::TimeUtils::timestamp_to_session(timestamp);
        assert_eq!(session, back);
        println!("   ✅ 会话 {} <-> 时间戳 {}", session, timestamp);
    }

    // 测试数据结构
    println!("\n📊 测试数据结构");
    let test_refno = RefU64::from_two_nums(17496, 200001);
    let test_data = pdms_io::io::EleOperationData {
        refno: test_refno,
        sesno: 501,
        detail: pdms_io::io::EleOperationDetail::None,
    };
    
    println!("   ✅ EleOperationData: 元素={}, 会话={}", test_data.refno, test_data.sesno);

    let historical = pdms_io::raphtory_integration::HistoricalElement {
        refno: test_refno,
        timestamp: 501,
        session_no: 501,
        data: test_data.clone(),
    };
    
    println!("   ✅ HistoricalElement: 元素={}, 时间戳={}", historical.refno, historical.timestamp);

    let mut timeline: pdms_io::raphtory_integration::ElementTimeline = Vec::new();
    timeline.push(historical);
    println!("   ✅ ElementTimeline: {} 条记录", timeline.len());

    println!("\n✅ 模拟测试完成！");
    println!("💡 所有基础结构和功能正常工作");
    
    Ok(())
}