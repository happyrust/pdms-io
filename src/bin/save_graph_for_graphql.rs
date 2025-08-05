//! 将 PDMS 数据保存为 Raphtory GraphQL 服务器可以加载的格式
//!
//! 这个程序解析 PDMS 数据，保存为图文件，然后提供 GraphQL 查询示例

use pdms_io::io::PdmsIO;
// use pdms_io::raphtory_integration::{RaphtoryIntegration, RaphtoryConfig};
use std::time::Instant;
use std::env;
use std::fs::File;
use std::io::Write;
use raphtory::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    pdms_io::init_log(log::LevelFilter::Info).unwrap();

    println!("🚀 PDMS 数据保存为 GraphQL 可加载格式");
    println!("=======================================");

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
        return Err(anyhow::anyhow!("数据库文件不存在"));
    }

    println!("📖 步骤 1: 初始化 PDMS 数据库连接");
    let mut io = PdmsIO::new("ams", &db_path, true);
    io.open()?;
    io.init_ses_range_map()?;
    
    let latest_sesno = io.get_latest_sesno()?;
    println!("   ✅ 数据库连接成功");
    println!("   📈 最新会话号: {}", latest_sesno);
    println!("   📊 会话范围映射: {} 个会话", io.ses_range_map.len());

    println!("\n📊 步骤 2: 收集 PDMS 数据并构建 Raphtory 图");
    let collect_start = Instant::now();
    
    // 创建一个新的图
    let graph = Graph::new();
    
    // 收集元素数据
    let elements = io.collect_latest_eles(max_sessions).await?;
    let collect_elapsed = collect_start.elapsed();
    
    println!("   ✅ 数据收集完成");
    println!("   ⏱️  耗时: {:?}", collect_elapsed);
    println!("   📦 收集到 {} 个元素", elements.len());

    if elements.is_empty() {
        println!("⚠️  没有收集到元素，退出程序");
        return Ok(());
    }

    println!("\n🔗 步骤 3: 构建 Raphtory 图");
    let mut added_nodes = 0;
    let mut added_edges = 0;
    
    for (refno, operation_data) in elements.iter() {
        // 转换会话号为时间戳
        let timestamp = pdms_io::raphtory_integration::TimeUtils::session_to_timestamp(operation_data.sesno as i32);
        
        // 创建节点名称
        let element_name = format!("Element_{}", refno);
        let system_name = "PDMS_System";
        
        // 添加元素节点
        let element_props = vec![
            ("refno", Prop::str(refno.to_string())),
            ("session", Prop::str(operation_data.sesno.to_string())),
            ("element_type", Prop::str("pdms_element")),
        ];
        
        graph.add_node(timestamp, &element_name, element_props, None)?;
        added_nodes += 1;
        
        // 添加系统节点（如果不存在）
        graph.add_node(timestamp, system_name, vec![("node_type", Prop::str("system"))], None)?;
        
        // 添加连接边
        let operation_type = match &operation_data.detail {
            pdms_io::io::EleOperationDetail::Add(_) => "element_added",
            pdms_io::io::EleOperationDetail::Modified(_) => "element_modified", 
            pdms_io::io::EleOperationDetail::Deleted => "element_deleted",
            pdms_io::io::EleOperationDetail::None => "element_processed",
        };
        
        let edge_props = vec![
            ("operation_type", Prop::str(operation_type)),
            ("session", Prop::str(operation_data.sesno.to_string())),
            ("refno", Prop::str(refno.to_string())),
        ];
        
        graph.add_edge(timestamp, &element_name, &system_name.to_string(), edge_props, None)?;
        added_edges += 1;
    }
    
    println!("   ✅ 图构建完成");
    println!("   📊 添加了 {} 个节点", added_nodes);
    println!("   🔗 添加了 {} 条边", added_edges);

    // 显示图统计信息
    println!("\n📈 步骤 4: 图统计信息");
    println!("   节点数: {}", graph.count_nodes());
    println!("   边数: {}", graph.count_edges());
    
    if let Some(earliest) = graph.earliest_time() {
        println!("   最早时间: {}", earliest);
        println!("   对应会话: {}", pdms_io::raphtory_integration::TimeUtils::timestamp_to_session(earliest));
    }
    if let Some(latest) = graph.latest_time() {
        println!("   最晚时间: {}", latest);
        println!("   对应会话: {}", pdms_io::raphtory_integration::TimeUtils::timestamp_to_session(latest));
    }

    println!("\n💾 步骤 5: 保存图数据为 CSV 格式");
    let graph_name = "pdms_ams1112_graph";
    
    // 创建 graphs 目录
    std::fs::create_dir_all("graphs")?;
    
    // 保存边列表
    save_graph_as_csv(&graph, &format!("graphs/{}.csv", graph_name), &elements)?;
    
    println!("   ✅ 图数据已保存到: graphs/{}.csv", graph_name);

    // 生成 GraphQL 查询示例
    generate_graphql_examples(graph_name, &elements);

    println!("\n🎉 数据保存完成！");
    println!("现在可以在 GraphQL playground 中查看和测试数据了！");
    println!("GraphQL 服务器地址: http://localhost:1737/");
    
    Ok(())
}

fn save_graph_as_csv(
    _graph: &Graph, 
    output_path: &str, 
    elements: &std::collections::HashMap<aios_core::pdms_types::RefU64, pdms_io::io::EleOperationData>
) -> anyhow::Result<()> {
    let mut file = File::create(output_path)?;
    writeln!(file, "time,src,dst,operation_type,session,refno")?;
    
    let mut count = 0;
    for (refno, operation_data) in elements.iter() {
        let timestamp = pdms_io::raphtory_integration::TimeUtils::session_to_timestamp(operation_data.sesno as i32);
        let element_name = format!("Element_{}", refno);
        let system_name = "PDMS_System";
        
        let operation_type = match &operation_data.detail {
            pdms_io::io::EleOperationDetail::Add(_) => "element_added",
            pdms_io::io::EleOperationDetail::Modified(_) => "element_modified", 
            pdms_io::io::EleOperationDetail::Deleted => "element_deleted",
            pdms_io::io::EleOperationDetail::None => "element_processed",
        };
        
        writeln!(file, "{},{},{},{},{},{}", 
                timestamp, 
                element_name, 
                system_name, 
                operation_type,
                operation_data.sesno,
                refno)?;
        count += 1;
    }
    
    println!("   📁 保存了 {} 条边记录", count);
    Ok(())
}

fn generate_graphql_examples(graph_name: &str, elements: &std::collections::HashMap<aios_core::pdms_types::RefU64, pdms_io::io::EleOperationData>) {
    println!("\n💡 GraphQL 查询示例");
    println!("===================");
    
    println!("\n1. 基本图信息查询:");
    println!("```graphql");
    println!("query {{");
    println!("  graph(path: \"{}\") {{", graph_name);
    println!("    name");
    println!("    nodeCount");
    println!("    edgeCount");
    println!("    earliestTime");
    println!("    latestTime");
    println!("  }}");
    println!("}}");
    println!("```");
    
    println!("\n2. 查询前10个节点:");
    println!("```graphql");
    println!("query {{");
    println!("  graph(path: \"{}\") {{", graph_name);
    println!("    nodes {{");
    println!("      list(first: 10) {{");
    println!("        name");
    println!("        properties {{");
    println!("          key");
    println!("          value");
    println!("        }}");
    println!("        history {{");
    println!("          additions");
    println!("        }}");
    println!("      }}");
    println!("    }}");
    println!("  }}");
    println!("}}");
    println!("```");
    
    println!("\n3. 查询边信息:");
    println!("```graphql");
    println!("query {{");
    println!("  graph(path: \"{}\") {{", graph_name);
    println!("    edges {{");
    println!("      list(first: 10) {{");
    println!("        src {{");
    println!("          name");
    println!("        }}");
    println!("        dst {{");
    println!("          name");
    println!("        }}");
    println!("        properties {{");
    println!("          key");
    println!("          value");
    println!("        }}");
    println!("        history {{");
    println!("          additions");
    println!("        }}");
    println!("      }}");
    println!("    }}");
    println!("  }}");
    println!("}}");
    println!("```");
    
    // 显示一个具体的元素示例
    if let Some((refno, operation_data)) = elements.iter().next() {
        let element_name = format!("Element_{}", refno);
        println!("\n4. 查询特定元素 ({}):", element_name);
        println!("```graphql");
        println!("query {{");
        println!("  graph(path: \"{}\") {{", graph_name);
        println!("    node(name: \"{}\") {{", element_name);
        println!("      name");
        println!("      properties {{");
        println!("        key");
        println!("        value");
        println!("      }}");
        println!("      edges {{");
        println!("        list {{");
        println!("          dst {{");
        println!("            name");
        println!("          }}");
        println!("          properties {{");
        println!("            key");
        println!("            value");
        println!("          }}");
        println!("        }}");
        println!("      }}");
        println!("    }}");
        println!("  }}");
        println!("}}");
        println!("```");
        
        println!("\n5. 时间窗口查询 (会话 {}):", operation_data.sesno);
        let timestamp = pdms_io::raphtory_integration::TimeUtils::session_to_timestamp(operation_data.sesno as i32);
        println!("```graphql");
        println!("query {{");
        println!("  graph(path: \"{}\") {{", graph_name);
        println!("    at(time: {}) {{", timestamp);
        println!("      nodeCount");
        println!("      edgeCount");
        println!("      nodes {{");
        println!("        list(first: 5) {{");
        println!("          name");
        println!("          properties {{");
        println!("            key");
        println!("            value");
        println!("          }}");
        println!("        }}");
        println!("      }}");
        println!("    }}");
        println!("  }}");
        println!("}}");
        println!("```");
    }
}