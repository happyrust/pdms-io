//! CSV 数据加载到 Raphtory GraphQL 服务器
//!
//! 这个程序将 CSV 文件中的 PDMS 数据加载到 Raphtory 图数据库，
//! 并启动 GraphQL 服务器以便在 playground 中查看数据

use raphtory::prelude::*;
use std::fs::File;
use std::io::{BufRead, BufReader};
use anyhow::Result;
use tokio;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 启动 PDMS 数据 Raphtory GraphQL 服务器");
    println!("=========================================");
    
    // 创建图
    let graph = Graph::new();
    
    // 加载 CSV 数据
    let csv_path = "graphs/pdms_real_data.csv";
    println!("📂 加载 CSV 文件: {}", csv_path);
    
    load_csv_to_graph(&graph, csv_path)?;
    
    // 显示图统计信息
    println!("\n📊 图统计信息:");
    println!("   节点数: {}", graph.count_nodes());
    println!("   边数: {}", graph.count_edges());
    
    if let Some(earliest) = graph.earliest_time() {
        println!("   最早时间: {}", earliest);
    }
    if let Some(latest) = graph.latest_time() {
        println!("   最晚时间: {}", latest);
    }
    
    // 保存图到文件
    let graph_name = "pdms_real_data_graph";
    println!("\n💾 保存图数据: {}", graph_name);
    
    // 创建 graphs 目录
    std::fs::create_dir_all("graphs")?;
    
    // 保存图数据（作为边列表）
    save_graph_as_edges(&graph, &format!("graphs/{}.csv", graph_name))?;
    
    println!("\n🌐 GraphQL 服务器启动指令:");
    println!("==========================");
    println!("cd /Volumes/DPC/work/database/Raphtory");
    println!("cargo run --bin raphtory-graphql -- --working-dir /Volumes/DPC/work/new-crates/pdms-io/graphs --port 1736");
    println!();
    println!("或者从当前目录:");
    println!("CARGO_MANIFEST_DIR=/Volumes/DPC/work/database/Raphtory/raphtory-graphql cargo run --manifest-path /Volumes/DPC/work/database/Raphtory/raphtory-graphql/Cargo.toml -- --working-dir $(pwd)/graphs --port 1736");
    println!();
    println!("访问地址:");
    println!("📱 GraphQL Playground: http://localhost:1736/graphql");
    println!("🏠 Web UI: http://localhost:1736/");
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
    println!("        history {{");
    println!("          additions");
    println!("        }}");
    println!("      }}");
    println!("    }}");
    println!("  }}");
    println!("}}");
    
    Ok(())
}

fn load_csv_to_graph(graph: &Graph, csv_path: &str) -> Result<()> {
    let file = File::open(csv_path)?;
    let reader = BufReader::new(file);
    
    let mut line_count = 0;
    let mut loaded_count = 0;
    
    for line in reader.lines() {
        line_count += 1;
        let line = line?;
        
        // 跳过标题行
        if line_count == 1 && line.starts_with("time,") {
            continue;
        }
        
        // 解析 CSV 行: time,src,dst,type,session,refno
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 6 {
            let time: i64 = parts[0].parse().unwrap_or(0);
            let src = parts[1];
            let dst = parts[2];
            let edge_type = parts[3];
            let session = parts[4];
            let refno = parts[5];
            
            // 添加节点
            let _src_node = graph.add_node(time, src, NO_PROPS, None)?;
            let _dst_node = graph.add_node(time, dst, NO_PROPS, None)?;
            
            // 添加边，包含元数据
            let edge_props = vec![
                ("operation_type", Prop::str(edge_type)),
                ("session", Prop::str(session)),
                ("refno", Prop::str(refno)),
            ];
            
            graph.add_edge(time, src, dst, edge_props, None)?;
            
            loaded_count += 1;
            
            if loaded_count % 100 == 0 {
                println!("   已加载 {} 条记录...", loaded_count);
            }
        }
    }
    
    println!("   ✅ 加载完成: {} 条记录", loaded_count);
    Ok(())
}

fn save_graph_as_edges(graph: &Graph, output_path: &str) -> Result<()> {
    use std::io::Write;
    
    let mut file = File::create(output_path)?;
    writeln!(file, "time,src,dst,type,session,refno")?;
    
    let mut count = 0;
    for edge in graph.edges() {
        for time in edge.history() {
            let src = edge.src().name();
            let dst = edge.dst().name();
            
            // 获取边属性
            let operation_type = edge.properties().get("operation_type")
                .map(|p| match p {
                    Prop::Str(s) => s.to_string(),
                    _ => "unknown".to_string()
                }).unwrap_or_else(|| "unknown".to_string());
            let session = edge.properties().get("session")
                .map(|p| match p {
                    Prop::Str(s) => s.to_string(),
                    _ => "0".to_string()
                }).unwrap_or_else(|| "0".to_string());
            let refno = edge.properties().get("refno")
                .map(|p| match p {
                    Prop::Str(s) => s.to_string(),
                    _ => "0".to_string()
                }).unwrap_or_else(|| "0".to_string());
            
            writeln!(file, "{},{},{},{},{},{}", 
                    time, src, dst, operation_type, session, refno)?;
            count += 1;
        }
    }
    
    println!("   ✅ 保存了 {} 条边记录到 {}", count, output_path);
    Ok(())
}