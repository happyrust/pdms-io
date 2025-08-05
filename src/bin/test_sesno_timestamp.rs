use pdms_io::io::PdmsIO;
use std::env;
use std::path::PathBuf;

/// 测试通过 sesno 获取时间戳的新功能
///
/// 使用方法：
/// 1. 使用环境变量（推荐）：
///    export PDMS_TEST_PATH="/Volumes/DPC/work/e3d_models"
///    cargo run --bin test_sesno_timestamp -- [数据库名] [会话号]
///
/// 2. 直接指定完整路径：
///    cargo run --bin test_sesno_timestamp -- <完整数据库路径> [会话号]
///
/// 示例：
/// export PDMS_TEST_PATH="/Volumes/DPC/work/e3d_models"
/// cargo run --bin test_sesno_timestamp -- "ams000/ams1112_0001" 1112
///
/// 或者：
/// cargo run --bin test_sesno_timestamp -- "/Volumes/DPC/work/e3d_models/ams000/ams1112_0001" 1112
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("使用方法:");
        println!("  1. 使用环境变量（推荐）：");
        println!("     export PDMS_TEST_PATH=\"/Volumes/DPC/work/e3d_models\"");
        println!("     {} <数据库名> [会话号]", args[0]);
        println!("     示例: {} \"ams000/ams1112_0001\" 1112", args[0]);
        println!();
        println!("  2. 直接指定完整路径：");
        println!("     {} <完整数据库路径> [会话号]", args[0]);
        println!("     示例: {} \"/Volumes/DPC/work/e3d_models/ams000/ams1112_0001\" 1112", args[0]);
        println!();
        println!("当前环境变量 PDMS_TEST_PATH: {:?}", env::var("PDMS_TEST_PATH").ok());
        return Ok(());
    }

    // 获取数据库路径
    let db_path = get_database_path(&args[1])?;

    // 检查文件是否存在
    if !std::path::Path::new(&db_path).exists() {
        eprintln!("错误：数据库文件不存在 {}", db_path);
        return Ok(());
    }
    
    println!("🔧 初始化 PDMS 数据库连接...");
    println!("   数据库路径: {}", db_path);
    let mut io = PdmsIO::new("ams", &db_path, true);
    io.open()?;
    io.init_ses_range_map()?;
    
    println!("✅ 数据库连接成功");
    
    // 获取最新的会话号
    let latest_sesno = io.get_latest_sesno()?;
    println!("📈 最新会话号: {}", latest_sesno);
    
    // 确定要测试的会话号
    let test_sesno = if args.len() >= 3 {
        args[2].parse::<u32>().unwrap_or(latest_sesno)
    } else {
        latest_sesno
    };
    
    println!("\n🕒 测试会话号 {} 的时间查询功能:", test_sesno);
    
    // 测试获取 DateTime<Utc>
    match io.get_sesno_datetime(test_sesno) {
        Ok(datetime) => {
            println!("   ✅ 获取 DateTime<Utc> 成功:");
            println!("      时间: {}", datetime);
            println!("      RFC3339 格式: {}", datetime.to_rfc3339());
        }
        Err(e) => {
            println!("   ❌ 获取 DateTime<Utc> 失败: {}", e);
            return Ok(());
        }
    }
    
    // 测试获取 Unix 时间戳
    match io.get_sesno_timestamp(test_sesno) {
        Ok(timestamp) => {
            println!("   ✅ 获取 Unix 时间戳成功:");
            println!("      时间戳: {}", timestamp);
            
            // 将时间戳转换回 DateTime 进行验证
            use chrono::{DateTime, Utc};
            if let Some(dt_from_timestamp) = DateTime::from_timestamp(timestamp, 0) {
                println!("      验证转换: {}", dt_from_timestamp);
            }
        }
        Err(e) => {
            println!("   ❌ 获取 Unix 时间戳失败: {}", e);
            return Ok(());
        }
    }
    
    // 验证两种方法的一致性
    if let (Ok(datetime), Ok(timestamp)) = (io.get_sesno_datetime(test_sesno), io.get_sesno_timestamp(test_sesno)) {
        if datetime.timestamp() == timestamp {
            println!("   ✅ 两种方法返回的时间一致");
        } else {
            println!("   ⚠️  两种方法返回的时间不一致:");
            println!("      DateTime.timestamp(): {}", datetime.timestamp());
            println!("      get_sesno_timestamp(): {}", timestamp);
        }
    }
    
    // 如果有多个会话，测试时间顺序
    if latest_sesno > 1 && test_sesno == latest_sesno {
        println!("\n🔍 测试时间顺序（比较最新会话和前一个会话）:");
        let previous_sesno = latest_sesno - 1;
        
        match (io.get_sesno_timestamp(previous_sesno), io.get_sesno_timestamp(latest_sesno)) {
            (Ok(prev_timestamp), Ok(latest_timestamp)) => {
                println!("   会话 {} 时间戳: {}", previous_sesno, prev_timestamp);
                println!("   会话 {} 时间戳: {}", latest_sesno, latest_timestamp);
                
                if prev_timestamp <= latest_timestamp {
                    println!("   ✅ 时间顺序正确（较早的会话时间 <= 较新的会话时间）");
                } else {
                    println!("   ⚠️  时间顺序异常（较早的会话时间 > 较新的会话时间）");
                }
            }
            (Err(e), _) => println!("   ❌ 获取前一个会话时间失败: {}", e),
            (_, Err(e)) => println!("   ❌ 获取最新会话时间失败: {}", e),
        }
    }
    
    // 显示会话范围信息
    println!("\n📊 会话范围信息:");
    println!("   总会话数: {}", io.ses_range_map.len());
    if let Some((min_sesno, _)) = io.ses_range_map.iter().next() {
        if let Some((max_sesno, _)) = io.ses_range_map.iter().next_back() {
            println!("   会话号范围: {} - {}", min_sesno, max_sesno);
        }
    }
    
    println!("\n🎉 测试完成！");

    Ok(())
}

/// 获取数据库路径
///
/// 支持两种方式：
/// 1. 如果输入是相对路径，则与环境变量 PDMS_TEST_PATH 组合
/// 2. 如果输入是绝对路径，则直接使用
///
/// # 参数
/// * `input_path` - 输入的路径字符串
///
/// # 返回值
/// * `anyhow::Result<String>` - 完整的数据库路径
fn get_database_path(input_path: &str) -> anyhow::Result<String> {
    let path = PathBuf::from(input_path);

    // 如果是绝对路径，直接返回
    if path.is_absolute() {
        return Ok(input_path.to_string());
    }

    // 如果是相对路径，尝试与环境变量组合
    let base_path = env::var("PDMS_TEST_PATH")
        .unwrap_or_else(|_| "/Volumes/DPC/work/e3d_models".to_string());

    let full_path = PathBuf::from(base_path).join(input_path);

    Ok(full_path.to_string_lossy().to_string())
}
