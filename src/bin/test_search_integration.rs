//! 集成搜索功能的简化示例
//!
//! 展示如何在现有的增量元素收集功能中集成 Meilisearch 搜索
//!
//! 用法: cargo run --bin test_search_integration [数据库路径]

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

    // 初始化SurrealDB连接
    init_test_surreal().await.unwrap();

    // 默认的数据库路径
    let db_path = std::env::args().nth(1).unwrap_or_else(|| {
        r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams8000_0001"#.to_string()
    });

    let project_name = Path::new(&db_path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s[0..3].to_string())
        .unwrap_or_else(|| "ams".to_string());

    println!("=== PDMS 元素搜索集成示例 ===");
    println!("数据库路径: {}", db_path);
    println!("项目名称: {}", project_name);

    // 初始化PDMS IO
    let mut io = PdmsIO::new(project_name.clone(), db_path, true);
    io.open()?;

    // 配置 Meilisearch（使用默认本地配置）
    let config = MeilisearchConfig {
        url: "http://localhost:7700".to_string(),
        api_key: None,
        index_name: format!("{}_elements", project_name),
    };

    // 创建搜索客户端
    let search_client = match ElementSearchClient::new(config) {
        Ok(client) => {
            println!("✓ Meilisearch 客户端创建成功");
            client
        }
        Err(e) => {
            println!("✗ 无法连接到 Meilisearch: {}", e);
            println!("请确保 Meilisearch 服务器正在运行在 http://localhost:7700");
            println!("安装和启动 Meilisearch:");
            println!("  1. 下载: https://github.com/meilisearch/meilisearch/releases");
            println!("  2. 运行: ./meilisearch");
            return Ok(());
        }
    };

    // 初始化索引
    println!("初始化搜索索引...");
    search_client.initialize_index().await?;

    // 获取最新会话号
    let latest_sesno = io.get_latest_sesno()? as i32;
    println!("数据库最新会话号: {}", latest_sesno);

    // 收集最近的元素数据
    let range_start = std::cmp::max(1, latest_sesno);
    let sesno_range = range_start..=latest_sesno;

    println!("收集会话 {} 的元素数据...", latest_sesno);
    let start_time = Instant::now();
    let range_eles = io.collect_increment_eles(Some(sesno_range))?;
    let elapsed = start_time.elapsed();

    let total_elements: usize = range_eles.values().map(|v| v.len()).sum();
    println!("收集到 {} 个元素，耗时: {:?}", total_elements, elapsed);

    if total_elements == 0 {
        println!("没有找到元素数据，请检查数据库路径或会话号");
        return Ok(());
    }

    // 索引元素到 Meilisearch
    println!("索引元素到搜索引擎...");
    let start_time = Instant::now();
    for (sesno, elements) in &range_eles {
        search_client.index_elements(elements).await?;
        println!("  会话 {} 的 {} 个元素已索引", sesno, elements.len());
    }
    let elapsed = start_time.elapsed();
    println!("索引完成，耗时: {:?}", elapsed);

    // 等待索引完成
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // 获取索引统计
    let stats = search_client.get_index_stats().await?;
    println!("索引统计: {} 个文档", stats.number_of_documents);

    // 演示搜索功能
    println!("\n=== 搜索功能演示 ===");

    // 搜索示例1: 按类型搜索
    println!("\n1. 搜索所有 PIPE 类型的元素:");
    let pipe_results = search_client.search_by_type("PIPE", 5).await?;
    if pipe_results.is_empty() {
        println!("  未找到 PIPE 类型的元素");
    } else {
        for (i, result) in pipe_results.iter().enumerate() {
            println!("  {}. {} (参考号: {})", i + 1, result.name, result.refno);
        }
    }

    // 搜索示例2: 模糊搜索
    println!("\n2. 模糊搜索 'ELBOW':");
    let elbow_results = search_client.fuzzy_search("ELBOW", None, 5).await?;
    if elbow_results.is_empty() {
        println!("  未找到包含 'ELBOW' 的元素");
    } else {
        for (i, result) in elbow_results.iter().enumerate() {
            println!(
                "  {}. {} (类型: {}, 参考号: {})",
                i + 1,
                result.name,
                result.element_type,
                result.refno
            );
        }
    }

    // 搜索示例3: 搜索第一个元素的参考号
    if let Some(first_element) = range_eles.values().flatten().next() {
        let refno_str = first_element.refno.to_string();
        println!("\n3. 搜索参考号 '{}':", refno_str);
        let refno_results = search_client.fuzzy_search(&refno_str, None, 1).await?;
        if let Some(result) = refno_results.first() {
            println!("  找到: {} (类型: {})", result.name, result.element_type);
            println!("  属性数量: {}", result.attributes.len());
            if !result.children.is_empty() {
                println!("  子元素数量: {}", result.children.len());
            }
        } else {
            println!("  未找到该参考号");
        }
    }

    // 搜索示例4: 按操作类型搜索
    println!("\n4. 搜索新增的元素:");
    let mut filters = std::collections::HashMap::new();
    filters.insert("operation_type".to_string(), "新增".to_string());
    let new_elements = search_client.advanced_search("", &filters, None, 5).await?;
    if new_elements.is_empty() {
        println!("  未找到新增的元素");
    } else {
        for (i, result) in new_elements.iter().enumerate() {
            println!(
                "  {}. {} (类型: {}, 会话: {})",
                i + 1,
                result.name,
                result.element_type,
                result.sesno
            );
        }
    }

    println!("\n=== 搜索演示完成 ===");
    println!("提示: 您可以使用以下方法进行搜索:");
    println!("  - search_by_name(): 按名称搜索");
    println!("  - search_by_type(): 按类型搜索");
    println!("  - fuzzy_search(): 模糊搜索");
    println!("  - advanced_search(): 高级搜索（支持过滤和排序）");

    Ok(())
}
