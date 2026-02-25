use aios_core::pdms_types::RefU64;
use anyhow::Result;
use pdms_io::io::{PdmsIO, demo_fast_query_with_index_map};
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    // 获取命令行参数
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "用法: {} <PDMS数据库文件路径> [参考号1] [参考号2] ...",
            args[0]
        );
        return Ok(());
    }

    let db_path = &args[1];

    // 收集参考号参数
    let mut refnos = Vec::new();
    if args.len() > 2 {
        for i in 2..args.len() {
            match RefU64::try_from(args[i].as_str()) {
                Ok(refno) => refnos.push(refno),
                Err(e) => {
                    eprintln!("无效的参考号格式 '{}': {}", args[i], e);
                    continue;
                }
            }
        }
    }

    if refnos.is_empty() {
        // 如果没有提供参考号，仅构建索引并测试单个查询
        println!("未提供参考号，构建索引后将打印索引大小");
        // 创建并打开数据库
        let mut io = PdmsIO::new("test", db_path, true);
        io.open()?;

        // 构建索引映射表
        println!("构建索引映射表...");
        let start_time = std::time::Instant::now();
        let index_map = io.build_index_map()?;
        let build_time = start_time.elapsed();

        println!(
            "索引构建完成，耗时: {:?}, 索引大小: {} 项",
            build_time,
            index_map.len()
        );

        // 如果索引非空，打印前10个项目
        if !index_map.is_empty() {
            println!("\n索引示例 (前10项):");
            for (i, (refno, locs)) in index_map.iter().take(10).enumerate() {
                for loc in locs {
                    println!("  {}: {} -> {:#4X}", i + 1, refno, loc);
                }
            }
        }
    } else {
        // 如果提供了参考号，使用示例函数演示查询
        println!("测试查询 {} 个参考号...", refnos.len());
        demo_fast_query_with_index_map(db_path, &refnos).await?;
    }

    Ok(())
}
