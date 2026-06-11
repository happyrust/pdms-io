// 定义测试模块
pub mod index_map_cache_test;
pub mod page_size_probe_test;
pub mod refno_test;
pub mod smoke_io_test;
/// specs/003:kv-mem 落库测试基建(T101)+ 幂等冒烟(T102)。
#[cfg(feature = "surrealdb")]
pub mod surreal_mem;
/// specs/003:增量落库幂等/水位/skip_main_data 测试(T204)。
#[cfg(feature = "surrealdb")]
pub mod surreal_ingest_test;
/// specs/004:writeback_queue 队列幂等/状态机/版本闸测试(T203)。
#[cfg(feature = "surrealdb")]
pub mod surreal_writeback_test;
/// specs/006:同步核心扫描/基线/捕获/幂等/隔离测试(T103)。
#[cfg(feature = "surrealdb")]
pub mod sync_core_test;
