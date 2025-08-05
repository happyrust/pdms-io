//! PDMS-Raphtory 数据存储和 GraphQL 服务演示
//!
//! 这个程序演示了如何：
//! 1. 解析 PDMS 数据并存储到 Raphtory 图数据库
//! 2. 保存图数据到文件系统
//! 3. 启动 GraphQL 服务器以便可视化数据

use pdms_io::io::PdmsIO;
use pdms_io::raphtory_integration::{RaphtoryIntegration, RaphtoryConfig};
use aios_core::pdms_types::RefU64;
use std::time::Instant;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    pdms_io::init_log(log::LevelFilter::Info).unwrap();

    println!("🚀 PDMS-Raphtory 服务器演示程序");
    println!("=====================================");

    // 解析命令行参数
    let args: Vec<String> = env::args().collect();
    
    let (db_path, max_sessions) = if args.len() >= 2 {
        let db_path = &args[1];
        let max_sessions = if args.len() >= 3 {
            args[2].parse::<u32>().ok()
        } else {
            Some(10) // 默认处理10个会话
        };
        (db_path.clone(), max_sessions)
    } else {
        // 使用默认的真实 PDMS 数据库文件
        let default_path = "/Volumes/DPC/work/e3d_models/AvevaMarineSample/ams000/ams1112_0001";
        println!("使用默认 PDMS 数据库文件: {}", default_path);
        (default_path.to_string(), Some(5))
    };

    println!("📂 数据库路径: {}", db_path);
    println!("📊 最大会话数: {:?}", max_sessions);
    println!();

    // 检查数据库文件是否存在
    if !std::path::Path::new(&db_path).exists() {
        println!("❌ 数据库文件不存在: {}", db_path);
        println!("请确保路径正确或提供有效的 PDMS 数据库文件路径");
        return Err(anyhow::anyhow!("数据库文件不存在"));
    }

    // 运行真实数据库测试
    run_demo_with_real_data(&db_path, max_sessions).await
}

/// 使用真实 PDMS 数据库运行演示
async fn run_demo_with_real_data(db_path: &str, max_sessions: Option<u32>) -> anyhow::Result<()> {
    println!("🔧 开始真实数据库演示");
    
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

    // 2. 创建 Raphtory 集成配置
    println!("\n🔗 步骤 2: 配置 Raphtory 集成");
    
    let mut config = RaphtoryConfig::default();
    config.db_num = 7999;
    config.verbose_logging = true;
    config.batch_size = 1000;
    config.graph_name = "pdms_production_graph".to_string();
    
    println!("   🔧 Raphtory 配置:");
    println!("      - 图名称: {}", config.graph_name);
    println!("      - 数据库编号: {}", config.db_num);
    println!("      - 批处理大小: {}", config.batch_size);

    let mut integration = RaphtoryIntegration::new(config);
    
    // 3. 收集并存储 PDMS 数据
    println!("\n📊 步骤 3: 收集并存储 PDMS 数据到 Raphtory");
    let collect_start = Instant::now();
    let elements = io.collect_latest_eles_with_options(max_sessions, Some(&mut integration)).await?;
    let collect_elapsed = collect_start.elapsed();
    
    println!("   ✅ 数据收集和存储完成");
    println!("   ⏱️  耗时: {:?}", collect_elapsed);
    println!("   📦 处理了 {} 个元素", elements.len());

    // 4. 分析图统计信息
    println!("\n📈 步骤 4: 分析图统计信息");
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

    // 5. 保存图数据
    println!("\n💾 步骤 5: 保存图数据到文件系统");
    let save_start = Instant::now();
    integration.finalize_and_save().await?;
    let save_elapsed = save_start.elapsed();
    
    println!("   ✅ 图数据保存完成");
    println!("   ⏱️  耗时: {:?}", save_elapsed);
    println!("   📁 保存路径: graphs/{}", integration.get_graph_name());

    // 总结
    let total_elapsed = total_start.elapsed();
    println!("\n🎉 数据存储完成总结");
    println!("==================");
    println!("⏱️  总执行时间: {:?}", total_elapsed);
    println!("📊 处理了 {} 个元素", elements.len());
    println!("💾 图数据已保存至: graphs/{}", integration.get_graph_name());
    
    print_graphql_instructions(integration.get_graph_name());

    Ok(())
}

/// 使用模拟数据运行演示
async fn run_demo_with_mock_data() -> anyhow::Result<()> {
    println!("🎭 运行模拟数据演示");
    
    let start_time = Instant::now();

    // 创建配置
    let mut config = RaphtoryConfig::default();
    config.db_num = 9999;
    config.verbose_logging = true;
    config.graph_name = "pdms_demo_graph".to_string();

    let mut integration = RaphtoryIntegration::new(config);
    integration.initialize()?;

    // 创建模拟数据
    let mut mock_elements = std::collections::HashMap::new();
    
    println!("📝 创建模拟 PDMS 元素数据...");
    for i in 1..=20 {
        let refno = RefU64::from_two_nums(17496, 300000 + i);
        let session_no = 1000 + i as u32;
        
        let operation_data = pdms_io::io::EleOperationData {
            refno,
            sesno: session_no,
            detail: pdms_io::io::EleOperationDetail::None,
        };
        
        mock_elements.insert(refno, operation_data);
        
        if i <= 5 {
            println!("   📦 模拟元素 {}: 会话 {}", refno, session_no);
        } else if i == 6 {
            println!("   📝 ... 还有 {} 个元素", 20 - 5);
        }
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

    // 保存图数据
    println!("\n💾 保存图数据到文件系统...");
    integration.finalize_and_save().await?;
    
    let elapsed = start_time.elapsed();
    println!("\n✅ 模拟演示完成，耗时: {:?}", elapsed);
    println!("💾 图数据已保存至: graphs/{}", integration.get_graph_name());
    
    print_graphql_instructions(integration.get_graph_name());

    Ok(())
}

/// 打印 GraphQL 服务器启动指令
fn print_graphql_instructions(graph_name: &str) {
    println!("\n🌐 GraphQL 服务器启动指令");
    println!("=========================");
    println!("现在可以启动 Raphtory GraphQL 服务器来查看数据：");
    println!();
    println!("方式一 - 从 Raphtory 源码目录启动:");
    println!("cd /Volumes/DPC/work/database/Raphtory");
    println!("cargo run --bin raphtory-graphql --working-dir /Volumes/DPC/work/new-crates/pdms-io/graphs --port 1736");
    println!();
    println!("方式二 - 从当前目录启动:");
    println!("CARGO_MANIFEST_DIR=/Volumes/DPC/work/database/Raphtory/raphtory-graphql cargo run --manifest-path /Volumes/DPC/work/database/Raphtory/raphtory-graphql/Cargo.toml --working-dir $(pwd)/graphs --port 1736");
    println!();
    println!("启动后，可以在浏览器中访问:");
    println!("📱 GraphQL Playground: http://localhost:1736/graphql");
    println!("🏠 Web UI: http://localhost:1736/");
    println!();
    println!("📊 图名称: {}", graph_name);
    println!("📂 数据路径: graphs/{}", graph_name);
    println!();
    println!("💡 GraphQL 查询示例:");
    println!("query {{");
    println!("  graph(path: \"{}\") {{", graph_name);
    println!("    name");
    println!("    nodeCount");
    println!("    edgeCount");
    println!("    earliestTime");
    println!("    latestTime");
    println!("    nodes {{");
    println!("      list(first: 10) {{");
    println!("        name");
    println!("        type");
    println!("        history {{");
    println!("          additions");
    println!("        }}");
    println!("      }}");
    println!("    }}");
    println!("  }}");
    println!("}}");
}