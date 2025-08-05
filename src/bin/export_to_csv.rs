//! 将真实 PDMS 数据导出为 CSV 格式供 Raphtory GraphQL 加载
//!
//! 用法: cargo run --bin export_to_csv [数据库路径] [最大会话数]

use pdms_io::io::PdmsIO;
use pdms_io::raphtory_integration::{RaphtoryIntegration, RaphtoryConfig};
use aios_core::pdms_types::RefU64;
use std::time::Instant;
use std::env;
use std::fs::File;
use std::io::Write;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    pdms_io::init_log(log::LevelFilter::Info).unwrap();

    println!("🚀 PDMS 数据导出到 CSV 程序");
    println!("===========================");

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

    // 检查数据库文件是否存在
    if !std::path::Path::new(&db_path).exists() {
        println!("❌ 数据库文件不存在: {}", db_path);
        return Err(anyhow::anyhow!("数据库文件不存在"));
    }

    println!("\n📖 步骤 1: 初始化 PDMS 数据库连接");
    let mut io = PdmsIO::new("ams", &db_path, true);
    io.open()?;
    io.init_ses_range_map()?;
    
    let latest_sesno = io.get_latest_sesno()?;
    println!("   ✅ 数据库连接成功");
    println!("   📈 最新会话号: {}", latest_sesno);
    println!("   📊 会话范围映射: {} 个会话", io.ses_range_map.len());

    println!("\n📊 步骤 2: 收集 PDMS 数据");
    let collect_start = Instant::now();
    let elements = io.collect_latest_eles(max_sessions).await?;
    let collect_elapsed = collect_start.elapsed();
    
    println!("   ✅ 数据收集完成");
    println!("   ⏱️  耗时: {:?}", collect_elapsed);
    println!("   📦 收集到 {} 个元素", elements.len());

    if elements.is_empty() {
        println!("⚠️  没有收集到元素，退出程序");
        return Ok(());
    }

    println!("\n💾 步骤 3: 导出到 CSV 文件");
    let csv_path = "graphs/pdms_real_data.csv";
    let mut file = File::create(csv_path)?;
    
    // 写入 CSV 头部
    writeln!(file, "time,src,dst,type,session,refno")?;
    
    let mut exported_count = 0;
    for (refno, operation_data) in elements.iter().take(1000) { // 限制导出数量以便测试
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
        exported_count += 1;
    }
    
    println!("   ✅ CSV 导出完成");
    println!("   📁 文件路径: {}", csv_path);
    println!("   📦 导出了 {} 个元素", exported_count);

    println!("\n📈 步骤 4: 显示统计信息");
    
    // 按会话分组统计
    let mut session_counts = std::collections::HashMap::new();
    for element in elements.values() {
        *session_counts.entry(element.sesno).or_insert(0) += 1;
    }
    
    println!("   按会话分布:");
    let mut sessions: Vec<_> = session_counts.iter().collect();
    sessions.sort_by_key(|(sesno, _)| *sesno);
    for (sesno, count) in sessions.iter().take(10) {
        println!("     会话 {}: {} 个元素", sesno, count);
    }
    if sessions.len() > 10 {
        println!("     ... 还有 {} 个会话", sessions.len() - 10);
    }

    // 显示一些示例元素
    println!("\n   示例元素:");
    let mut count = 0;
    for (refno, operation_data) in elements.iter() {
        if count >= 5 {
            break;
        }
        println!("     元素 {}: 会话 {}, 操作: {:?}", 
                refno, operation_data.sesno, 
                match &operation_data.detail {
                    pdms_io::io::EleOperationDetail::Add(_) => "新增",
                    pdms_io::io::EleOperationDetail::Modified(_) => "修改",
                    pdms_io::io::EleOperationDetail::Deleted => "删除",
                    pdms_io::io::EleOperationDetail::None => "无操作",
                });
        count += 1;
    }

    println!("\n🎉 导出完成！");
    println!("现在可以在 Raphtory GraphQL playground 中查看真实的 PDMS 历史数据");
    println!("GraphQL 服务器地址: http://localhost:1736/");
    
    Ok(())
}