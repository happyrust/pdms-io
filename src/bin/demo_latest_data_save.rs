//! 演示最新数据收集和保存功能
//!
//! 这个程序演示了如何使用 collect_and_save_latest_data 方法
//! 来收集最新的元素数据并保存到 SurrealDB 数据库中。
//!
//! 用法: cargo run --bin demo_latest_data_save [数据库路径] [最大会话数]
//!
//! 参数:
//! - 数据库路径: PDMS 数据库文件路径（可选，默认使用示例路径）
//! - 最大会话数: 要处理的最大会话数量（可选，默认为 5）

use aios_core::get_db_option;
use aios_core::init_test_surreal;
use pdms_io::init_log;
use pdms_io::io::PdmsIO;
use std::path::Path;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    init_log(log::LevelFilter::Info).unwrap();

    // 获取数据库配置
    let db_option = get_db_option();
    println!("数据库连接字符串: {}", db_option.get_version_db_conn_str());

    // 初始化SurrealDB连接
    println!("初始化 SurrealDB 连接...");
    init_test_surreal().await.unwrap();
    println!("✅ SurrealDB 连接成功");

    // 解析命令行参数
    let db_path = std::env::args().nth(1).unwrap_or_else(|| {
        r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams8000_0001"#.to_string()
    });

    let max_sessions = std::env::args()
        .nth(2)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(5);

    // 从路径提取项目名称
    let project_name = Path::new(&db_path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s[0..3].to_string())
        .unwrap_or_else(|| "ams".to_string());

    println!("\n=== 配置信息 ===");
    println!("数据库路径: {}", db_path);
    println!("项目名称: {}", project_name);
    println!("最大会话数: {}", max_sessions);

    // 检查数据库文件是否存在
    if !Path::new(&db_path).exists() {
        eprintln!("❌ 错误: 数据库文件不存在: {}", db_path);
        eprintln!("请提供有效的 PDMS 数据库文件路径");
        return Ok(());
    }

    // 初始化PDMS IO
    println!("\n=== 初始化 PDMS IO ===");
    let mut io = PdmsIO::new(project_name.clone(), &db_path, true);

    let init_start = Instant::now();
    io.open()?;
    let init_elapsed = init_start.elapsed();
    println!("✅ PDMS IO 初始化完成，耗时: {:?}", init_elapsed);

    // 显示数据库基本信息
    let latest_sesno = io.get_latest_sesno()?;
    let latest_dt = io.get_latest_dt()?;
    println!("数据库最新会话号: {}", latest_sesno);
    println!("数据库最新时间: {}", latest_dt.format("%Y-%m-%d %H:%M:%S"));

    // 执行主要功能：收集并保存最新数据
    println!("\n=== 开始收集并保存最新数据 ===");
    let main_start = Instant::now();

    match io
        .collect_and_save_latest_data(Some(max_sessions), None)
        .await
    {
        Ok(()) => {
            let main_elapsed = main_start.elapsed();
            println!("\n🎉 任务完成!");
            println!("总执行时间: {:?}", main_elapsed);
        }
        Err(e) => {
            eprintln!("❌ 执行过程中发生错误: {}", e);
            eprintln!("错误详情: {:?}", e);
        }
    }

    // 显示一些统计信息
    println!("\n=== 统计信息 ===");

    // 尝试再次收集数据以显示统计
    let stats_start = Instant::now();
    let latest_elements = io.collect_latest_eles(Some(max_sessions)).await?;
    let stats_elapsed = stats_start.elapsed();

    println!("当前最新元素数量: {}", latest_elements.len());
    println!("统计耗时: {:?}", stats_elapsed);

    // 按会话分组显示
    let mut session_counts = std::collections::HashMap::new();
    for element in latest_elements.values() {
        *session_counts.entry(element.sesno).or_insert(0) += 1;
    }

    if !session_counts.is_empty() {
        println!("\n按会话分布:");
        let mut sessions: Vec<_> = session_counts.iter().collect();
        sessions.sort_by_key(|(sesno, _)| *sesno);
        for (sesno, count) in sessions {
            println!("  会话 {}: {} 个元素", sesno, count);
        }
    }

    println!("\n=== 程序执行完成 ===");
    Ok(())
}
