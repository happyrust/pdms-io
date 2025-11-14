//! 历史数据全量同步 CLI
//!
//! 用法: cargo run --bin sync_history [数据库路径]
//!
//! 参数:
//! - 数据库路径: PDMS 数据库文件路径（可选，默认使用示例路径）

use aios_core::get_db_option;
use aios_core::init_test_surreal;
use pdms_io::init_log;
use pdms_io::io::sync_all_history_data;
use std::path::Path;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    init_log(log::LevelFilter::Info).unwrap();

    // 获取数据库配置（当前仅用于打印连接字符串，复用 demo_latest_data_save 的行为）
    let db_option = get_db_option();
    println!("数据库连接字符串: {}", db_option.get_version_db_conn_str());

    // 初始化 SurrealDB 连接
    println!("初始化 SurrealDB 连接...");
    init_test_surreal().await.unwrap();
    println!("✅ SurrealDB 连接成功");

    // 解析命令行参数
    let db_path = std::env::args().nth(1).unwrap_or_else(|| {
        r#"D:\\AVEVA\\Projects\\E3D2.1\\AvevaMarineSample\\ams000\\ams8000_0001"#.to_string()
    });

    // 从路径提取项目名称（目前 sync_all_history_data 自己内部固定 "ams"，这里仅打印）
    let project_name = Path::new(&db_path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s[0..3].to_string())
        .unwrap_or_else(|| "ams".to_string());

    println!("\n=== 历史同步配置 ===");
    println!("数据库路径: {}", db_path);
    println!("项目名称: {}", project_name);

    // 检查数据库文件是否存在
    if !Path::new(&db_path).exists() {
        eprintln!("❌ 错误: 数据库文件不存在: {}", db_path);
        eprintln!("请提供有效的 PDMS 数据库文件路径");
        return Ok(());
    }

    // 执行主要功能：全量历史同步
    println!("\n=== 开始历史数据全量同步 ===");
    let main_start = Instant::now();

    match sync_all_history_data(&db_path).await {
        Ok(()) => {
            let main_elapsed = main_start.elapsed();
            println!("\n🎉 历史同步完成!");
            println!("总执行时间: {:?}", main_elapsed);
        }
        Err(e) => {
            eprintln!("❌ 历史同步过程中发生错误: {}", e);
            eprintln!("错误详情: {:?}", e);
        }
    }

    println!("\n=== 历史同步程序执行完成 ===");
    Ok(())
}
