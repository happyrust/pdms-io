//! 创建包含完整PE数据的Raphtory图文件
//!
//! 这个程序解析PDMS数据，提取完整的PE数据作为节点属性，使用sesno作为时间戳

use pdms_io::io::PdmsIO;
use std::time::Instant;
use std::env;
use raphtory::prelude::*;
use raphtory::serialise::StableEncode;
use aios_core::NamedAttrValue;
use aios_core::init_surreal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    pdms_io::init_log(log::LevelFilter::Info).unwrap();

    println!("🚀 创建包含完整PE数据的Raphtory图文件");
    println!("=====================================");

    // 解析命令行参数
    let args: Vec<String> = env::args().collect();

    let (db_path, max_sessions) = if args.len() >= 2 {
        let db_path = &args[1];
        let max_sessions = if args.len() >= 3 {
            args[2].parse::<u32>().ok()
        } else {
            Some(3) // 默认处理3个会话
        };
        (db_path.clone(), max_sessions)
    } else {
        // 使用统一的配置系统
        let default_path = pdms_io::Config::get_database_path("AvevaMarineSample/ams000/ams1112_0001");
        println!("使用默认 PDMS 数据库文件: {}", default_path);
        println!("（可以通过设置环境变量 PDMS_PROJECT_PATH 来更改基础路径）");
        (default_path, Some(3))
    };

    println!("📂 数据库路径: {}", db_path);
    println!("📊 最大会话数: {:?}", max_sessions);
    println!();

    // 检查数据库文件是否存在
    if !std::path::Path::new(&db_path).exists() {
        println!("❌ 数据库文件不存在: {}", db_path);
        return Err(anyhow::anyhow!("数据库文件不存在"));
    }

    println!("📖 步骤 1: 初始化数据库连接");

    // 1.1 初始化 SurrealDB 连接
    println!("   🔗 连接 SurrealDB...");
    match init_surreal().await {
        Ok(_) => {
            println!("   ✅ SurrealDB 连接成功");
        }
        Err(e) => {
            println!("   ❌ SurrealDB 连接失败: {}", e);
            return Err(anyhow::anyhow!("SurrealDB 连接失败: {}", e));
        }
    }

    // 1.2 初始化 PDMS 数据库连接
    println!("   📂 连接 PDMS 数据库...");
    let mut io = PdmsIO::new("ams", &db_path, true);
    println!("   ✅ PDMS 数据库连接成功");
    io.open()?;
    io.init_ses_range_map()?;

    let latest_sesno = io.get_latest_sesno()?;
    println!("   ✅ 数据库连接成功");
    println!("   📈 最新会话号: {}", latest_sesno);
    println!("   📊 会话范围映射: {} 个会话", io.ses_range_map.len());

    println!("\n📊 步骤 2: 收集 PDMS 数据");
    let collect_start = Instant::now();
    let elements = io.collect_latest_eles(Some(5)).await?;
    let collect_elapsed = collect_start.elapsed();

    println!("   ✅ 数据收集完成");
    println!("   ⏱️  耗时: {:?}", collect_elapsed);
    println!("   📦 收集到 {} 个元素", elements.len());

    if elements.is_empty() {
        println!("⚠️  没有收集到元素，退出程序");
        return Ok(());
    }

    // 步骤 2.5: 保存历史数据信息到数据库
    println!("\n💾 步骤 2.5: 保存历史数据信息到 SurrealDB");
    let save_history_start = Instant::now();

    // 按会话组织数据
    let mut range_eles: std::collections::BTreeMap<u32, Vec<pdms_io::io::EleOperationData>> = std::collections::BTreeMap::new();
    for (_, element_data) in &elements {
        let sesno = element_data.sesno;
        range_eles.entry(sesno).or_insert_with(Vec::new).push(element_data.clone());
    }

    // 只更新历史数据信息，不更新主数据
    io.update_elements_to_database(&range_eles, false).await?;
    return Ok(());
    let save_history_elapsed = save_history_start.elapsed();

    println!("   ✅ 历史数据信息保存完成");
    println!("   ⏱️  耗时: {:?}", save_history_elapsed);
    println!("   📊 保存了 {} 个会话的历史数据", range_eles.len());

    println!("\n🔗 步骤 3: 构建包含完整PE数据的Raphtory图");
    let graph = Graph::new();
    let mut added_nodes = 0;
    let mut added_edges = 0;
    let mut pe_data_count = 0;

    for (refno, operation_data) in elements.iter() {
        // 使用真实的会话数据获取时间戳
        let timestamp = match io.get_sesno_datetime(operation_data.sesno) {
            Ok(datetime) => {
                // 将UTC DateTime转换为Unix时间戳(毫秒)
                datetime.timestamp_millis()
            },
            Err(e) => {
                println!("⚠️  警告：无法获取会话 {} 的时间数据，使用会话号作为时间戳: {}", operation_data.sesno, e);
                operation_data.sesno as i64
            }
        };

        // 根据操作类型处理数据
        match &operation_data.detail {
            pdms_io::io::EleOperationDetail::Add(ele_data) => {
                // 新增元素，使用PE格式的ID
                let element_name = format!("PE_{}", refno);
                let attr_map = ele_data.att_map(); // 正确获取NamedAttrMap

                // 创建基础属性
                let mut element_props = vec![
                    ("refno", Prop::str(refno.to_string())),
                    ("sesno", Prop::I64(operation_data.sesno as i64)),
                    ("operation_type", Prop::str("ADD")),
                    ("element_type", Prop::str(attr_map.get_type())),
                    ("deleted", Prop::Bool(false)), // 新增元素未被删除
                ];

                // 添加PE数据
                let pe_data = attr_map.pe(1112); // 使用正确的dbnum=1112
                element_props.push(("dbnum", Prop::I32(pe_data.dbnum)));

                // 不存储pe_data_json，只记录数量
                pe_data_count += 1;

                // 添加Owner关系信息
                let owner_refno = attr_map.get_owner();
                element_props.push(("owner_refno", Prop::str(owner_refno.to_string())));
                
                // 如果Owner不是默认值，创建Owner关系边
                if owner_refno != Default::default() {
                    let owner_name = format!("PE_{}", owner_refno);
                    let ownership_edge_props = vec![
                        ("relationship", Prop::str("owns")),
                        ("owner_refno", Prop::str(owner_refno.to_string())),
                        ("owned_refno", Prop::str(refno.to_string())),
                        ("sesno", Prop::I64(operation_data.sesno as i64)),
                    ];
                    
                    // 添加所有权边：owner -> element
                    graph.add_edge(timestamp, &owner_name, &element_name, ownership_edge_props, None)?;
                    added_edges += 1;
                }

                // 不添加专有属性，所有属性都通过枚举自动转换

                // 添加属性映射中的关键属性
                for (attr_name, attr_value) in attr_map.iter() {
                    match attr_value {
                        NamedAttrValue::StringType(text) => {
                            element_props.push((attr_name, Prop::str(text.clone())));
                        },
                        NamedAttrValue::RefU64Type(refno) => {
                            element_props.push((attr_name, Prop::str(refno.to_string())));
                        },
                        _ => {
                            // 对于所有类型，都转换为字符串表示
                            element_props.push((attr_name, Prop::str(format!("{:?}", attr_value))));
                        }
                    }
                }

                // 通过枚举自动添加所有属性

                graph.add_node(timestamp, &element_name, element_props, None)?;
                added_nodes += 1;

                println!("   📝 添加元素: {} (refno: {}, sesno: {}, 类型: {}, Owner: {:?})",
                        element_name, refno, operation_data.sesno, attr_map.get_type(), 
                        attr_map.get_owner());
            },

            pdms_io::io::EleOperationDetail::Modified(modified_element) => {
                // 修改的元素 - 使用已包含的完整数据作为新时间点插入
                let element_name = format!("PE_{}", refno);
                let attr_map = modified_element.current_data.att_map();
                
                // 创建基础属性
                let mut element_props = vec![
                    ("refno", Prop::str(refno.to_string())),
                    ("sesno", Prop::I64(operation_data.sesno as i64)),
                    ("operation_type", Prop::str("MODIFIED")),
                    ("element_type", Prop::str(attr_map.get_type())),
                    ("deleted", Prop::Bool(false)),
                ];
                
                // 添加PE数据
                let pe_data = attr_map.pe(1112);
                element_props.push(("dbnum", Prop::I32(pe_data.dbnum)));
                pe_data_count += 1;
                
                // 添加Owner关系信息
                let owner_refno = attr_map.get_owner();
                element_props.push(("owner_refno", Prop::str(owner_refno.to_string())));
                
                // 如果Owner不是默认值，创建Owner关系边
                if owner_refno != Default::default() {
                    let owner_name = format!("PE_{}", owner_refno);
                    let ownership_edge_props = vec![
                        ("relationship", Prop::str("owns")),
                        ("owner_refno", Prop::str(owner_refno.to_string())),
                        ("owned_refno", Prop::str(refno.to_string())),
                        ("sesno", Prop::I64(operation_data.sesno as i64)),
                    ];
                    
                    // 添加所有权边：owner -> element
                    graph.add_edge(timestamp, &owner_name, &element_name, ownership_edge_props, None)?;
                    added_edges += 1;
                }
                
                // 添加修改信息属性
                element_props.push(("added_attrs_count", Prop::I32(modified_element.added_attrs.len() as i32)));
                element_props.push(("deleted_attrs_count", Prop::I32(modified_element.deleted_attrs.len() as i32)));
                element_props.push(("modified_attrs_count", Prop::I32(modified_element.modified_attrs.len() as i32)));
                
                // 添加属性映射中的所有属性
                for (attr_name, attr_value) in attr_map.iter() {
                    match attr_value {
                        NamedAttrValue::StringType(text) => {
                            element_props.push((attr_name, Prop::str(text.clone())));
                        },
                        NamedAttrValue::RefU64Type(refno) => {
                            element_props.push((attr_name, Prop::str(refno.to_string())));
                        },
                        _ => {
                            element_props.push((attr_name, Prop::str(format!("{:?}", attr_value))));
                        }
                    }
                }
                
                graph.add_node(timestamp, &element_name, element_props, None)?;
                added_nodes += 1;
                
                println!("   ✏️  修改元素: {} (refno: {}, sesno: {}, 类型: {}, Owner: {:?})",
                        element_name, refno, operation_data.sesno, attr_map.get_type(), owner_refno);
            },

            pdms_io::io::EleOperationDetail::Deleted => {
                // 删除的元素
                let element_name = format!("PE_{}", refno);

                // 标记删除（保留历史）
                let element_props = vec![
                    ("refno", Prop::str(refno.to_string())),
                    ("sesno", Prop::I64(operation_data.sesno as i64)),
                    ("operation_type", Prop::str("DELETED")),
                    ("element_type", Prop::str("deleted_element")),
                    ("deleted", Prop::Bool(true)), // 已删除的元素
                ];

                graph.add_node(timestamp, &element_name, element_props, None)?;
                added_nodes += 1;

                // 删除所有从该元素指向其他元素的边（如果存在）
                // 这表示删除的元素不再拥有任何子元素
                // 由于我们使用 owner->owned 的边方向，需要查找所有以该元素为起点的边
                if let Some(node) = graph.node(&element_name) {
                    // 获取所有出边的目标节点列表（在删除前收集）
                    let outgoing_targets: Vec<String> = node.out_edges()
                        .iter()
                        .map(|edge| edge.dst().name().to_string())
                        .collect();
                    
                    // 删除所有出边
                    for target in outgoing_targets {
                        match graph.delete_edge(timestamp, &element_name, &target, None) {
                            Ok(_) => {
                                println!("   ❌ 删除边: {} -> {}", element_name, target);
                            },
                            Err(e) => {
                                println!("   ⚠️  无法删除边 {} -> {}: {}", element_name, target, e);
                            }
                        }
                    }
                }

                println!("   🗑️  删除元素: {} (refno: {}, sesno: {})",
                        element_name, refno, operation_data.sesno);
            },

            pdms_io::io::EleOperationDetail::None => {
                // 无操作的元素
                let element_name = format!("PE_{}", refno);

                let element_props = vec![
                    ("refno", Prop::str(refno.to_string())),
                    ("sesno", Prop::I64(operation_data.sesno as i64)),
                    ("operation_type", Prop::str("NONE")),
                    ("element_type", Prop::str("no_operation")),
                    ("deleted", Prop::Bool(false)), // 无操作的元素未被删除
                ];

                graph.add_node(timestamp, &element_name, element_props, None)?;
                added_nodes += 1;
            }
        }

        // 不创建session节点和边，因为node属性中已经包含sesno信息
    }

    println!("\n   ✅ 图构建完成");
    println!("   📊 添加了 {} 个节点", added_nodes);
    println!("   🔗 添加了 {} 条边", added_edges);
    println!("   🔢 包含PE数据的元素: {} 个", pe_data_count);

    // 显示图统计信息
    println!("\n📈 步骤 4: 图统计信息");
    println!("   节点数: {}", graph.count_nodes());
    println!("   边数: {}", graph.count_edges());

    if let Some(earliest) = graph.earliest_time() {
        println!("   最早时间: {} (会话 {})", earliest, earliest);
    }
    if let Some(latest) = graph.latest_time() {
        println!("   最晚时间: {} (会话 {})", latest, latest);
    }

    println!("\n💾 步骤 5: 保存图到文件系统");
    let graph_name = "pdms_complete_pe_data";

    // 创建 graphs 目录
    std::fs::create_dir_all("graphs")?;

    // 使用 Raphtory 原生编码保存图
    let graph_path = format!("graphs/{}", graph_name);
    println!("   正在保存图数据到: {}", graph_path);

    // 如果目标目录已存在，先删除它
    let graph_path_buf = std::path::PathBuf::from(&graph_path);
    if graph_path_buf.exists() {
        println!("   🗑️  删除现有目录: {}", graph_path);
        std::fs::remove_dir_all(&graph_path_buf)?;
    }

    // 使用 Raphtory 的 encode 方法保存图（这会创建 .raph 元数据文件）
    graph.encode(&graph_path_buf)?;

    println!("   ✅ 图数据已保存到磁盘");
    println!("   📁 创建了 .raph 元数据文件");
    println!("   📊 图文件夹: {}", graph_path);

    println!("\n🌐 GraphQL 服务器启动指令:");
    println!("==========================");
    println!("CARGO_MANIFEST_DIR=/Volumes/DPC/work/database/Raphtory/raphtory-graphql \\");
    println!("cargo run --manifest-path /Volumes/DPC/work/database/Raphtory/raphtory-graphql/Cargo.toml \\");
    println!("-- --working-dir $(pwd)/graphs --port 1737");
    println!();
    println!("访问地址:");
    println!("📱 GraphQL Playground: http://localhost:1737/");
    println!();
    println!("💡 GraphQL 查询示例:");
    println!("query {{");
    println!("  graph(path: \"{}\") {{", graph_name);
    println!("    name");
    println!("    nodeCount");
    println!("    edgeCount");
    println!("    earliestTime");
    println!("    latestTime");
    println!("  }}");
    println!("}}");
    println!();
    println!("💡 查询特定节点的PE数据:");
    println!("query {{");
    println!("  graph(path: \"{}\") {{", graph_name);
    println!("    nodes(limit: 5) {{");
    println!("      name");
    println!("      properties {{");
    println!("        refno");
    println!("        sesno");
    println!("        dbnum");
    println!("        element_type");
    println!("        operation_type");
    println!("        owner_refno");
    println!("        deleted");
    println!("        # 所有其他PDMS属性都会自动显示");
    println!("      }}");
    println!("    }}");
    println!("  }}");
    println!("}}");

    println!("\n🎉 包含完整PE数据的图文件创建完成！");
    println!("现在可以在 GraphQL playground 中查看真实的PDMS PE数据了！");

    Ok(())
}
