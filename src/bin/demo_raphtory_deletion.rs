//! 演示 Raphtory 中不同的删除操作方式

use raphtory::prelude::*;

fn main() -> anyhow::Result<()> {
    println!("🔍 Raphtory 删除操作演示");
    println!("========================\n");

    // 创建一个示例图
    let graph = Graph::new();
    
    // 添加一些初始数据
    println!("1️⃣ 初始状态：添加节点和边");
    
    // 时间 T1: 创建父子关系
    let t1 = 1000;
    graph.add_node(t1, "parent", [("type", Prop::str("container"))], None)?;
    graph.add_node(t1, "child1", [("type", Prop::str("element"))], None)?;
    graph.add_node(t1, "child2", [("type", Prop::str("element"))], None)?;
    
    graph.add_edge(t1, "parent", "child1", [("rel", Prop::str("owns"))], None)?;
    graph.add_edge(t1, "parent", "child2", [("rel", Prop::str("owns"))], None)?;
    
    println!("   ✅ 创建了 parent -> child1, child2 的关系");
    println!("   节点数: {}, 边数: {}\n", graph.count_nodes(), graph.count_edges());

    // 时间 T2: 演示方案1 - 软删除（标记删除）
    println!("2️⃣ 方案1：软删除 child1（标记为已删除）");
    let t2 = 2000;
    graph.add_node(
        t2, 
        "child1", 
        [
            ("type", Prop::str("element")),
            ("deleted", Prop::Bool(true)),
            ("deletion_time", Prop::I64(t2))
        ], 
        None
    )?;
    
    println!("   ✅ child1 被标记为已删除");
    println!("   节点数: {} (未变化)\n", graph.count_nodes());

    // 时间 T3: 演示方案2 - 硬删除节点
    println!("3️⃣ 方案2：硬删除 child2");
    let t3 = 3000;
    graph.delete_node(t3, "child2")?;
    
    println!("   ✅ child2 在时间 {} 后被完全删除", t3);
    println!("   节点数（全时间）: {}", graph.count_nodes());
    println!("   节点数（T3之后）: {}\n", graph.at(t3).count_nodes());

    // 时间 T4: 演示方案3 - 删除边
    println!("4️⃣ 方案3：删除 parent -> child1 的关系");
    let t4 = 4000;
    // 注意：Raphtory 中删除边需要指定层（layer），None 表示默认层
    graph.delete_edge(t4, "parent", "child1", None)?;
    
    println!("   ✅ 删除了 parent -> child1 的边");
    println!("   边数（全时间）: {}", graph.count_edges());
    println!("   边数（T4之后）: {}\n", graph.at(t4).count_edges());

    // 查询不同时间点的状态
    println!("5️⃣ 时间旅行查询：");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    // T1 时刻
    let g_t1 = graph.at(t1);
    println!("📅 时间 T1 ({}):", t1);
    println!("   节点: parent, child1, child2");
    println!("   边: parent->child1, parent->child2");
    println!("   child1 是否存在: {}", g_t1.node("child1").is_some());
    println!("   child2 是否存在: {}", g_t1.node("child2").is_some());
    
    // T2 时刻
    let g_t2 = graph.at(t2);
    println!("\n📅 时间 T2 ({}) - child1 被软删除后:", t2);
    if let Some(child1) = g_t2.node("child1") {
        let props = child1.properties().as_map();
        println!("   child1 存在，deleted = {:?}", props.get("deleted"));
    }
    
    // T3 时刻
    let g_t3 = graph.at(t3);
    println!("\n📅 时间 T3 ({}) - child2 被硬删除后:", t3);
    println!("   child1 是否存在: {}", g_t3.node("child1").is_some());
    println!("   child2 是否存在: {} (已被硬删除)", g_t3.node("child2").is_some());
    
    // T4 时刻
    let g_t4 = graph.at(t4);
    println!("\n📅 时间 T4 ({}) - 删除边后:", t4);
    println!("   parent->child1 边是否存在: {}", 
        g_t4.node("parent")
            .and_then(|n| n.out_edges())
            .map(|edges| edges.iter().any(|e| e.dst().name() == "child1"))
            .unwrap_or(false)
    );

    // 演示最佳实践：软删除 + 边删除
    println!("\n6️⃣ 最佳实践：软删除 + 删除相关边");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    let t5 = 5000;
    let element_to_delete = "element_x";
    
    // 先创建一个元素
    graph.add_node(t5, element_to_delete, [("type", Prop::str("equipment"))], None)?;
    graph.add_edge(t5, "parent", element_to_delete, [("rel", Prop::str("owns"))], None)?;
    
    // 执行删除
    let t6 = 6000;
    
    // 1. 标记为删除
    graph.add_node(
        t6, 
        element_to_delete,
        [
            ("type", Prop::str("equipment")),
            ("deleted", Prop::Bool(true)),
            ("deleted_at", Prop::I64(t6)),
            ("deletion_reason", Prop::str("user_action"))
        ],
        None
    )?;
    
    // 2. 删除所有相关的边
    graph.delete_edge(t6, "parent", element_to_delete, None)?;
    
    println!("   ✅ 执行了软删除 + 边删除");
    println!("   - 节点仍然存在（用于审计）");
    println!("   - 但所有关系已被移除");
    
    // 保存图以供后续分析
    println!("\n💾 保存演示图...");
    std::fs::create_dir_all("graphs")?;
    graph.encode(&std::path::PathBuf::from("graphs/deletion_demo"))?;
    println!("   ✅ 已保存到 graphs/deletion_demo");

    Ok(())
}