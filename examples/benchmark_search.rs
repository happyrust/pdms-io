use aios_core::RefU64;
use anyhow::Result;
use pdms_io::io::{benchmark_search_refno_pgno, extract_test_refnos, PdmsIO};
use std::env;

/// 运行search_refno_pgno的性能基准测试
///
/// 用法: cargo run --example benchmark_search -- <db_path> [iterations] [refno_count]
///
/// 参数:
///   - db_path: PDMS数据库文件路径
///   - iterations: 每个参考号重复测试的次数，默认为10
///   - refno_count: 用于测试的参考号数量，默认为5
#[tokio::main]
async fn main() -> Result<()> {
    // let args: Vec<String> = env::args().collect();

    // if args.len() < 2 {
    //     eprintln!("用法: cargo run --example benchmark_search -- <db_path> [iterations] [refno_count]");
    //     std::process::exit(1);
    // }

    // let db_path = &args[1];
    // let iterations = if args.len() > 2 {
    //     args[2].parse::<usize>().unwrap_or(10)
    // } else {
    //     10
    // };

    let db_path = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams1112_0001"#;
    // let refno_str = "17496/184133";
    let refno_count = 50;
    let iterations = 10;

    // let refno_count = if args.len() > 3 {
    //     args[3].parse::<usize>().unwrap_or(5)
    // } else {
    //     5
    // };

    // 打开数据库并获取真实的参考号
    let mut io = PdmsIO::new("bench", db_path, true);
    io.open()?;
    io.init_ses_range_map()?;

    println!("从数据库中提取测试用的参考号...");
    let real_refnos = match extract_test_refnos(&mut io, refno_count) {
        Ok(refnos) => {
            if refnos.is_empty() {
                println!("没有找到真实参考号，将使用模拟数据");
                vec![
                    RefU64::from_two_nums(1, 1),
                    RefU64::from_two_nums(10, 10),
                    RefU64::from_two_nums(100, 100),
                    RefU64::from_two_nums(1000, 1000),
                    RefU64::from_two_nums(10000, 10000),
                ]
            } else {
                println!("找到 {} 个真实参考号用于测试", refnos.len());
                refnos
            }
        }
        Err(e) => {
            eprintln!("提取参考号时出错: {}", e);
            println!("将使用模拟数据");
            vec![
                RefU64::from_two_nums(1, 1),
                RefU64::from_two_nums(10, 10),
                RefU64::from_two_nums(100, 100),
                RefU64::from_two_nums(1000, 1000),
                RefU64::from_two_nums(10000, 10000),
            ]
        }
    };

    // 运行基准测试
    benchmark_search_refno_pgno(db_path, &real_refnos, iterations).await?;

    Ok(())
}
