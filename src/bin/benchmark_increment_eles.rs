use anyhow::Result;
use pdms_io::benchmark_increment_eles;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    // 获取命令行参数
    // let args: Vec<String> = env::args().collect();
    // if args.len() < 2 {
    //     eprintln!("用法: {} <数据库文件路径>", args[0]);
    //     return Ok(());
    // }

    // // 运行基准测试
    // let db_path = &args[1];
    let db_path = std::env::args().nth(1).unwrap_or_else(|| {
        r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams1112_0001"#.to_string()
    });
    benchmark_increment_eles(&db_path).await?;

    Ok(())
}
