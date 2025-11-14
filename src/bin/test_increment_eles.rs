//! 测试增量元素收集功能的可执行程序
//!
//! 用法: cargo run --bin test_increment_eles [数据库路径] [参考号]
//!
//! 如果不传入参数，则使用默认的数据库路径和参考号

use aios_core::get_db_option;
use aios_core::pdms_types::EleOperation;
use aios_core::RefU64;
use pdms_io::init_log;
use pdms_io::io::{EleOperationData, EleOperationDetail, PdmsIO};
use std::path::Path;
use std::time::Instant;
// use aios_core::NamedAttrValue;
use aios_core::init_test_surreal; // 导入初始化SurrealDB的函数
use aios_core::SUL_DB; // 导入SurrealDB全局连接
use std::collections::HashMap;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_log(log::LevelFilter::Debug).unwrap();
    let db_option = get_db_option();
    dbg!(&db_option.get_version_db_conn_str());
    // 初始化SurrealDB连接
    init_test_surreal().await.unwrap();

    // 默认的数据库路径和参考号
    let db_path = std::env::args().nth(1).unwrap_or_else(|| {
        r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams8000_0001"#.to_string()
    });
    let refno_str = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "17496_497143".to_string());

    let project_name = Path::new(&db_path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s[0..3].to_string())
        .unwrap_or_else(|| "ams".to_string());

    println!("打开数据库: {}", db_path);
    println!("测试参考号: {}", &refno_str);

    // 初始化PDMS IO
    let mut io = PdmsIO::new(project_name.clone(), db_path, true);
    io.open()?;

    // let operation = io.get_refno_operation_status(refno_str.into(), None);
    // dbg!(&operation);
    // return Ok(());

    // 获取最新会话号
    let latest_sesno = io.get_latest_sesno()? as i32;
    println!("数据库最新会话号: {}", latest_sesno);

    // 测试用例1: 使用None获取最新会话
    // println!("\n测试1: 获取最新会话的元素");
    // let start_time = Instant::now();
    // let max_sesno = io.get_latest_att_pgno()? as i32;
    // dbg!(max_sesno);
    // let latest_eles = io.collect_increment_eles(Some(0..=max_sesno)).unwrap();
    // let elapsed = start_time.elapsed();

    // println!("最新会话中共有 {} 个元素, 耗时: {:?}", latest_eles.len(), elapsed);

    // 测试用例2: 使用固定范围
    let range_start = std::cmp::max(1, latest_sesno);
    let sesno_range = range_start..=latest_sesno;

    println!("\n测试2: 获取会话范围 {:?} 内的元素", sesno_range);
    let start_time = Instant::now();
    let range_eles = io.collect_increment_eles(Some(sesno_range.clone()))?;
    let elapsed = start_time.elapsed();

    // 将元素操作保存到SurrealDB
    io.update_elements_to_database(&range_eles, true).await?;
    Ok(())
}
