use crate::io::PdmsIO;
use aios_core::pdms_types::{EleOperation, RefU64};
use std::env;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// 定义测试专用的辅助函数
fn get_test_db_path() -> PathBuf {
    // 尝试从环境变量获取数据库路径
    if let Ok(path) = env::var("PDMS_TEST_DB_PATH") {
        return PathBuf::from(path);
    }

    // 如果环境变量未设置，使用默认测试数据库路径
    // 请根据您的实际情况修改此路径
    PathBuf::from("./test_data/test.pdms")
}

fn format_duration(duration: Duration) -> String {
    if duration.as_millis() > 0 {
        format!("{:.2}ms", duration.as_millis() as f64)
    } else {
        format!("{:.2}μs", duration.as_micros() as f64)
    }
}
