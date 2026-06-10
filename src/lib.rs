pub mod common;
pub mod config;
pub mod defines;
pub mod element_record_reader;
pub mod element_serializer;
#[allow(warnings)]
pub mod io;
pub mod page_manager;
pub mod paged_reader;
#[cfg(feature = "meilisearch")]
pub mod search;
pub mod test;
pub mod writer;

#[cfg(feature = "sync-archive")]
pub mod sync;

pub mod watch;

pub mod dblist;

pub mod engine_v2;
pub mod io_log;

/// Fully-offline E3D/PDMS DABACON element read + write (std-only; see docs/e3d 数据库分析/).
/// Now lives in the standalone `crates/e3d_io` crate so it builds + tests inside the workspace
/// independently of the rs-core <-> surrealdb integration blocker; re-exported here as
/// `pdms_io::e3d_decode` so existing call sites keep resolving.
pub use e3d_io as e3d_decode;

// 重新导出常用函数，使其可以直接从crate根访问
pub use io::{PdmsIO, benchmark_increment_eles};

// 重新导出配置管理功能
pub use config::{Config, ConfigInfo};

// 重新导出日志配置功能
pub use io_log::{LogConfig, init_log, init_log_advanced, init_log_with_file};

#[cfg(test)]
pub mod tests;

#[cfg(feature = "surrealdb")]
pub mod surql;
