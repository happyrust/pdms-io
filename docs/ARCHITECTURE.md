# PDMS-IO 架构说明

## 目标与范围

- 解析 AVEVA PDMS/AVEVA Marine 数据库文件（会话页、元素页、索引页），提取元素增量与历史。
- 将元素操作（新增/修改/删除）转换为结构化数据，便于落地到 SurrealDB、搜索引擎或下游同步。
- 提供本地文件监控与远端同步雏形，支撑增量获取与分发。
- 提供简单的基准、演示与测试入口，帮助验证解析正确性与性能。

## 引擎架构：单一格式核心 + 门面（specs/002 收敛后,2026-06-11）

```
调用方(watch/bins/tests, 30+ 引用点)
        │  公共 API 冻结(specs/002 contracts C1;tests/api_freeze_c1.rs 编译期锁)
        ▼
PdmsIO 门面(src/io.rs)──只拥有"什么时候读、读完干什么":
   会话范围推导 · 增量提取/历史编排 · IndexMap 构建与磁盘缓存(PIM1) ·
   deku 类型视图(PdmsHeader/SessionPageData/IndexPageData,冻结返回类型)
        │  全部字节获取经持久只读视图
        ▼
crates/e3d_io ──唯一格式核心,"字节怎么读写"的全部真相(std-only,零依赖):
   读:Edb(整文件)/ Rdb<S: PageSource>(惰性影子页:导航/B树点查 btree_find/
      叶枚举 leaves/会话链 session_chain/变长记录 element_record)
   写:COW + 新会话(EdbWriter;verify_commit/batch/dry-run/护栏,见 specs/001 US2/US5)
   页源:PageSource trait ── InMemory(buffer) | PagedFile(LRU+命中统计+C3.2 页大小探测)
        ▼
   元素库文件(512/2048/4096B 页,大端;页大小以 page_type==Session 探测为准)

parse_pdms_db(crate)= EleData 类型适配层(FR-003 决策):消费门面给的记录字节,
   不再持有独立的记录定界/取页逻辑(base-27 哈希经 aios_core::db_tool 引用)。
```

- **历史**:仓库曾三引擎并存(v1 `PdmsIO` 自带解析 / `engine_v2` db1~db5 实验层 / `e3d_io`)。
  specs/002 已收敛:`engine_v2`(39 文件)、v1 写路径(`writer.rs`/`element_serializer.rs`)、
  v1 读取辅助(`page_manager.rs`/`paged_reader.rs`/`element_record_reader.rs`)及 v1 B 树搜索
  族**全部退役删除**;逆向知识归档于 `docs/engine-v2-archaeology.md`。
- **格式字节级规范**:见 `specs/001-e3d-data-format/` 与 `docs/e3d 数据库分析/E3D_DB_文件格式规范.md`。

## 关键外部依赖

- `aios_core`: PDMS 类型定义、环境配置（数据库路径收集、RefNo/SESNo 工具）、测试 SurrealDB 初始化。
- `e3d_io`（workspace 内 `crates/e3d_io`）: **唯一格式核心**——B 树/会话链/记录定界/页大小探测/COW 写的单一实现（std-only）。
- `parse_pdms_db`: `EleData` 类型适配层（字节获取已单源 `e3d_io`，本 crate 负责记录字节 → 结构化属性的类型转换）。
- `surrealdb`: 数据持久化目标；增量主线为强类型 serde/upsert（`surreal_ingest.rs`，specs/003），字符串拼接 SQL 仅余 D4 声明的历史回填入口。
- `meilisearch-sdk`: 可选的全文搜索索引。
- `notify`/`walkdir`: 文件系统监听与遍历。
- `dashmap`/`rayon`/`tokio`: 并发访问与异步任务调度。
- `fern`/`log`: 日志输出。

## 核心库模块

- `defines.rs`
  - PDMS 头、会话页、元素页等结构体定义（`PdmsHeader`、`SessionPageData`、`EleData` 等）。
  - 常量与工具方法（如 `PAGE_SIZE`、时间转换、序列化到 JSON/SurrealQL）。
- `io.rs`
  - 核心服务 `PdmsIO`（**门面**：公共 API 冻结，字节获取全部委托 `e3d_io` 持久只读视图 `Rdb<PagedFile>`），负责文件打开、会话范围构建、索引扫描、元素读取与差异分析。
  - 增量收集：`collect_increment_eles` / `collect_increment_eles_optimized` 根据会话范围或索引映射提取 `EleOperationData`。
  - 历史/最新检索：`search_latest_refno`、`collect_ele_history` 等。
  - 数据落地：`update_elements_to_database` 唯一增量入口（委托 `surreal_ingest.rs` 强类型 upsert + 水位；可选择跳过主数据）。
  - 索引缓存：`build_index_map`、`cache_index_map`/`load_cached_index_map` 加速 RefNo 定位。
  - 辅助：`benchmark_increment_eles`、`extract_test_refnos` 等测试/基准入口。
- `search.rs`
  - Meilisearch 适配层：`ElementSearchClient` 将 `EleOperationData` 转为 `ElementDocument`，支持索引初始化、批量写入与简单查询。
- `watch.rs`
  - `PdmsWatcher` 负责遍历数据库目录，记录 `DbPageBasicInfo`（最新会话页、文件大小等），并准备本地 `.cba` 压缩文件（同步逻辑待实现）。
- `sync/*`
  - 文件同步雏形：压缩（`compress_archive`）、远端元数据获取（`get_remote_db_headers`）等，尚未集成完整流程。
- `config.rs`
  - 路径与环境变量解析：`PDMS_PROJECT_PATH` / `PDMS_TEST_PATH`，`Config::get_database_path` 等。
- `io_log.rs`
  - 日志初始化工具：控制台/文件/高级配置。
- `surreal_ingest.rs`
  - specs/003 落库核心：`SesRow`/`PeSesHRow`/`PeRow`/`WatermarkRow` 强类型行 + `ingest_increments` 纯函数（kv-mem 可直接驱动）+ `IngestReport` 可观测回执。
- `surql/`
  - 预置 SurrealQL 片段与占位模块，便于集中管理查询语句。
- `bin/`
  - 演示与调试程序（基准测试、搜索集成、索引映射检查等）。
- `test/` 与 `tests/`
  - 针对元素解析、历史数据、增量收集等的单元与集成测试。

## 关键数据流

1. **解析与增量提取**
  - `PdmsIO::open` 读取头部并构建会话范围（`init_ses_range_map`）。
  - 索引扫描（`collect_refno_locs_in_session`/`build_index_map`）→ 定位元素页 → 解析 `EleData`。
  - 比对上一会话状态，产出 `EleOperationData`（Add/Modify/Delete）。
2. **持久化（specs/003 收敛后,2026-06-11）**
  - **唯一增量入口** `update_elements_to_database`（签名冻结）→ `surreal_ingest.rs`：强类型 serde/upsert 写 `ses` / `pe_ses_h` /（除非 `skip_main_data`）`pe`，并维护 `ingest_watermark` 水位（`dbnum → 已落 sesno`，单调升、重放跳过可观测）；记录 ID 由 `(dbnum, refno, sesno)` 确定性生成 ⇒ 重放幂等。
  - `collect_and_save_latest_data` 的保存段委托上述入口（收集/组织段保留）；旧字符串拼接路径（`to_surql`/`save_sessions_and_elements`/`sync_history` 坟场）已退役删除。
  - `store_all_refno_sesno_map` 为**全库历史回填专用入口**（含物理 offset 的全量收集，契约 D4 声明保留；其遗留拼接段记 004 候选强类型化）。
3. **搜索索引**
  - `collect_increment_eles` 结果 → `ElementSearchClient::index_elements` → Meilisearch。
4. **监控与同步**
  - `PdmsWatcher::init_local_watcher` 扫描工程目录，缓存各数据库文件的最新会话信息，预留 `.cba` 压缩任务用于后续分发。
  - `sync::files::sync_e3d_files`/`sync::sync::compress_archive` 作为远端同步与压缩的起点（尚未串联）。

## 状态与缓存

- 会话范围缓存：`ses_range_map`（会话号 → 页号范围）、`sesno_pgno_map`（会话号 → 起始页号）。
- 元素索引缓存：`DashMap`/`BTreeMap` 保存 RefNo → 物理偏移映射，可持久化到本地文件。
- 文件监控缓存：`PdmsWatcher.headers` 追踪每个数据库文件的最新会话信息；`file_name_full_path_map` 辅助定位文件。

## 日志与可观测性

- 通过 `init_log`/`init_log_with_file`/`init_log_advanced` 配置输出目的地、颜色与详细程度。
- 主要调试输出集中在 `io.rs` 与示例二进制中，便于跟踪解析、写入与索引进度。

## 测试与演示

- `src/test/`* 覆盖元素解析、增量收集、历史状态、文件同步等基础能力。
- 演示程序：`bin/benchmark_increment_eles.rs`（性能）、`bin/test_search_integration.rs`（搜索接入）、`bin/test_get_refno_status.rs` 等。

## 演进建议

- 补齐 `sync`/`surql` 模块，形成端到端的增量同步链路。
- 为关键路径（索引构建、元素解析、数据库写入）添加基准测试与性能计数器。
- 提升健壮性：减少 `unwrap`/`expect`，为文件/网络异常和数据异常提供可恢复策略。