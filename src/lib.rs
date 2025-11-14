pub mod common;
pub mod config;
pub mod defines;
#[allow(warnings)]
pub mod io;
pub mod search;
pub mod test;

pub mod sync;

pub mod watch;

pub mod io_log;

// 重新导出常用函数，使其可以直接从crate根访问
pub use io::{benchmark_increment_eles, PdmsIO};

// 重新导出配置管理功能
pub use config::{Config, ConfigInfo};

// 重新导出日志配置功能
pub use io_log::{init_log, init_log_advanced, init_log_with_file, LogConfig};

#[cfg(test)]
pub mod tests;

pub mod surql;
