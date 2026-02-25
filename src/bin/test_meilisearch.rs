//! 测试 Meilisearch 检索功能的可执行程序
//!
//! 用法: cargo run --bin test_meilisearch [数据库路径] [Meilisearch URL]
//!
//! 如果不传入参数，则使用默认的数据库路径和 Meilisearch URL

use aios_core::get_db_option;
use aios_core::init_test_surreal;
use pdms_io::init_log;
use pdms_io::io::PdmsIO;
use pdms_io::search::{ElementSearchClient, MeilisearchConfig};
use std::path::Path;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_log(log::LevelFilter::Info).unwrap();
    let db_option = get_db_option();
    dbg!(&db_option.get_version_db_conn_str());

    // 初始化SurrealDB连接
    init_test_surreal().await.unwrap();

    // 默认的数据库路径和 Meilisearch URL
    let db_path = std::env::args().nth(1).unwrap_or_else(|| {
        r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams8000_0001"#.to_string()
    });
    let meilisearch_url = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "http://localhost:7700".to_string());

    let project_name = Path::new(&db_path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s[0..3].to_string())
        .unwrap_or_else(|| "ams".to_string());

    println!("打开数据库: {}", db_path);
    println!("Meilisearch URL: {}", meilisearch_url);

    // 初始化PDMS IO
    let mut io = PdmsIO::new(project_name.clone(), db_path, true);
    io.open()?;

    // 配置 Meilisearch
    let config = MeilisearchConfig {
        url: meilisearch_url,
        api_key: Some("masterKey123".to_string()),
        index_name: format!("{}_elements", project_name),
    };

    // 创建搜索客户端
    let search_client = ElementSearchClient::new(config)?;

    // 初始化索引
    println!("初始化 Meilisearch 索引...");
    search_client.initialize_index().await?;

    // 使用新的方法：收集并保存最新数据
    println!("\n=== 使用新方法收集并保存最新数据 ===");
    let start_time = Instant::now();
    io.collect_and_save_latest_data(Some(10), None).await?;
    let elapsed = start_time.elapsed();
    println!("新方法执行完成，总耗时: {:?}", elapsed);

    // 收集最新的元素数据用于索引
    println!("\n=== 收集数据用于 Meilisearch 索引 ===");
    let start_time = Instant::now();
    let latest_elements = io.collect_latest_eles(Some(10)).await?;
    let elapsed = start_time.elapsed();

    let total_elements = latest_elements.len();
    println!("收集到 {} 个元素，耗时: {:?}", total_elements, elapsed);
    //保存到数据库
    io.collect_and_save_latest_data(Some(10), Some(latest_elements.clone()))
        .await?;

    if total_elements > 0 {
        // 将元素数据索引到 Meilisearch
        println!("将元素数据索引到 Meilisearch...");
        let start_time = Instant::now();
        let elements: Vec<_> = latest_elements.into_values().collect();
        search_client.index_elements(&elements).await?;
        let elapsed = start_time.elapsed();
        println!("索引完成，耗时: {:?}", elapsed);

        // 等待索引完成
        println!("等待索引完成...");
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // 获取索引统计信息
        let stats = search_client.get_index_stats().await?;
        println!("索引统计信息:");
        println!("  文档数量: {}", stats.number_of_documents);
        println!("  是否正在索引: {}", stats.is_indexing);
        println!("  字段分布: {:?}", stats.field_distribution);

        // 测试搜索功能
        println!("\n=== 开始测试搜索功能 ===");

        // 测试1: 按名称模糊搜索
        println!("\n1. 按名称模糊搜索 'PIPE':");
        let start_time = Instant::now();
        let name_results = search_client.search_by_name("24384", 10).await?;
        let elapsed = start_time.elapsed();
        println!("找到 {} 个结果，耗时: {:?}", name_results.len(), elapsed);
        for (i, result) in name_results.iter().take(5).enumerate() {
            println!(
                "  {}. {} (类型: {}, 参考号: {})",
                i + 1,
                result.name,
                result.element_type,
                result.refno
            );
        }

        println!("\n=== 搜索测试完成 ===");
    } else {
        println!("没有找到新的元素数据，跳过 Meilisearch 索引");
    }

    Ok(())
}
