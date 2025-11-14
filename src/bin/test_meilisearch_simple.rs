//! 简单的 Meilisearch 搜索功能测试
//!
//! 这个测试程序演示了如何使用 Meilisearch 搜索功能，不依赖外部数据库

use pdms_io::search::{ElementDocument, ElementSearchClient, MeilisearchConfig};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🔍 Meilisearch 搜索功能测试");
    println!("================================");

    // 1. 创建 Meilisearch 配置
    let config = MeilisearchConfig {
        url: "http://localhost:7700".to_string(),
        api_key: Some("masterKey123".to_string()),
        index_name: "test_pdms_elements".to_string(),
    };

    // 2. 创建搜索客户端
    println!("📡 正在连接到 Meilisearch 服务器...");
    let client = match ElementSearchClient::new(config) {
        Ok(client) => {
            println!("✅ 成功连接到 Meilisearch 服务器");
            client
        }
        Err(e) => {
            println!("❌ 无法连接到 Meilisearch 服务器: {}", e);
            println!("💡 请确保 Meilisearch 服务器正在运行:");
            println!("   - 下载: https://github.com/meilisearch/meilisearch/releases");
            println!("   - 运行: ./meilisearch --master-key=your-master-key");
            println!(
                "   - 或使用 Docker: docker run -it --rm -p 7700:7700 getmeili/meilisearch:latest"
            );
            return Ok(());
        }
    };

    // 3. 初始化索引
    println!("🔧 正在初始化搜索索引...");
    match client.initialize_index().await {
        Ok(_) => println!("✅ 索引初始化成功"),
        Err(e) => {
            println!("❌ 索引初始化失败: {}", e);
            return Ok(());
        }
    }

    // 4. 创建测试数据
    println!("📝 正在创建测试数据...");
    let test_documents = create_test_documents();

    println!("📤 正在添加测试文档到索引...");
    match client.add_documents(&test_documents).await {
        Ok(_) => {
            println!("✅ 测试文档添加成功");
            // 等待索引更新
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
        Err(e) => {
            println!("❌ 测试文档添加失败: {}", e);
            return Ok(());
        }
    }

    // 6. 测试搜索功能
    println!("\n🔍 开始测试搜索功能:");
    println!("====================");

    // 测试按名称搜索
    println!("\n1️⃣ 测试按名称搜索 'PIPE':");
    test_search_by_name(&client, "PIPE").await;

    // 测试按类型搜索
    println!("\n2️⃣ 测试按类型搜索 'PIPE':");
    test_search_by_type(&client, "PIPE").await;

    // 测试模糊搜索
    println!("\n3️⃣ 测试模糊搜索 'valve':");
    test_fuzzy_search(&client, "valve", None).await;

    // 测试高级搜索
    println!("\n4️⃣ 测试高级搜索:");
    let mut filters = HashMap::new();
    filters.insert("element_type".to_string(), "PIPE".to_string());
    match client
        .advanced_search("", &filters, Some("timestamp:desc"), 10)
        .await
    {
        Ok(results) => {
            println!("   找到 {} 个结果:", results.len());
            for doc in results {
                println!("   - {}: {} ({})", doc.id, doc.name, doc.element_type);
            }
        }
        Err(e) => println!("   ❌ 搜索失败: {}", e),
    }

    // 获取索引统计
    println!("\n📊 索引统计信息:");
    test_index_stats(&client).await;

    println!("\n🎉 测试完成！");
    Ok(())
}

fn create_test_documents() -> Vec<ElementDocument> {
    vec![
        ElementDocument {
            id: "1".to_string(),
            refno: "1".to_string(),
            name: "PIPE-001".to_string(),
            element_type: "PIPE".to_string(),
            sesno: 1,
            operation_type: "Add".to_string(),
            attributes_text: "PIPE PIPE-001 DN100 length 1000".to_string(),
            attributes: HashMap::from([
                ("diameter".to_string(), "100".to_string()),
                ("length".to_string(), "1000".to_string()),
            ]),
            children: vec!["2".to_string(), "3".to_string()],
            timestamp: "2024-03-20T10:00:00Z".to_string(),
        },
        ElementDocument {
            id: "2".to_string(),
            refno: "2".to_string(),
            name: "VALVE-001".to_string(),
            element_type: "VALVE".to_string(),
            sesno: 1,
            operation_type: "Add".to_string(),
            attributes_text: "VALVE VALVE-001 ball valve DN100".to_string(),
            attributes: HashMap::from([
                ("type".to_string(), "ball".to_string()),
                ("size".to_string(), "100".to_string()),
            ]),
            children: vec![],
            timestamp: "2024-03-20T10:01:00Z".to_string(),
        },
        ElementDocument {
            id: "3".to_string(),
            refno: "3".to_string(),
            name: "PIPE-002".to_string(),
            element_type: "PIPE".to_string(),
            sesno: 1,
            operation_type: "Add".to_string(),
            attributes_text: "PIPE PIPE-002 DN150 length 2000".to_string(),
            attributes: HashMap::from([
                ("diameter".to_string(), "150".to_string()),
                ("length".to_string(), "2000".to_string()),
            ]),
            children: vec![],
            timestamp: "2024-03-20T10:02:00Z".to_string(),
        },
    ]
}

async fn test_search_by_name(client: &ElementSearchClient, query: &str) {
    match client.search_by_name(query, 10).await {
        Ok(results) => {
            println!("   找到 {} 个结果:", results.len());
            for result in results {
                println!(
                    "   - {} ({}): {}",
                    result.name, result.element_type, result.refno
                );
            }
        }
        Err(e) => println!("   ❌ 搜索失败: {}", e),
    }
}

async fn test_search_by_type(client: &ElementSearchClient, type_query: &str) {
    match client.search_by_type(type_query, 10).await {
        Ok(results) => {
            println!("   找到 {} 个结果:", results.len());
            for result in results {
                println!(
                    "   - {} ({}): {}",
                    result.name, result.element_type, result.refno
                );
            }
        }
        Err(e) => println!("   ❌ 搜索失败: {}", e),
    }
}

async fn test_fuzzy_search(client: &ElementSearchClient, query: &str, type_filter: Option<&str>) {
    match client.fuzzy_search(query, type_filter, 10).await {
        Ok(results) => {
            println!("   找到 {} 个结果:", results.len());
            for result in results {
                println!(
                    "   - {} ({}): {}",
                    result.name, result.element_type, result.refno
                );
            }
        }
        Err(e) => println!("   ❌ 搜索失败: {}", e),
    }
}

async fn test_index_stats(client: &ElementSearchClient) {
    match client.get_index_stats().await {
        Ok(stats) => {
            println!("   📈 文档数量: {}", stats.number_of_documents);
            println!(
                "   🔄 正在索引: {}",
                if stats.is_indexing { "是" } else { "否" }
            );
            println!("   📊 字段分布:");
            for (field, count) in stats.field_distribution {
                println!("      - {}: {}", field, count);
            }
        }
        Err(e) => println!("   ❌ 获取统计失败: {}", e),
    }
}
