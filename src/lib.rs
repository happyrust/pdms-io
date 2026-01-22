#[allow(warnings)]

pub mod io;
pub mod defines;
pub mod common;
pub mod config;
pub mod test;
#[cfg(feature = "meilisearch")]
pub mod search;
pub mod page_manager;
pub mod writer;
pub mod element_serializer;
pub mod paged_reader;
pub mod element_record_reader;

pub mod sync;

pub mod watch;

pub mod io_log;

// 重新导出常用函数，使其可以直接从crate根访问
pub use io::{PdmsIO, benchmark_increment_eles};

// 重新导出配置管理功能
pub use config::{Config, ConfigInfo};

// 重新导出日志配置功能
pub use io_log::{init_log, init_log_with_file, init_log_advanced, LogConfig};

#[cfg(test)]
pub mod tests;

pub mod surql;






