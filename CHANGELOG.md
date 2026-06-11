# 更新日志

## [未发布]

### 进行中 — SurrealDB → E3D 写回管道(specs/004,2026-06-11 起)

> 在 001 验证过的写能力与 003 落库形态之间架安全写回管道;grill Q1~Q6 全按推荐拍板(纯函数入口→队列两层 / 001 原语全集 / 默认副本+verify 强制 / 接受回声 / kv-mem+sam7200 验收 / 严格管道范围)。

- **e3d_io 决策 A 扩展**(红线经正式决策放行):`EdbWriter` 增 refno 导向薄变体 ×6(`set_inline_at`/`set_pos_at`/`rename_at`/`set_members_at`/`delete_at`/`insert_clone_at`)+ 解析助手 ×2,各为既有 name 方法严格同构 ⇒ **无名元素(真实库 ~88%)可入 batch 单会话编辑**;格式/事务核心零改动,std-only 不变(39+1 全绿)。
- **写回核心 Phase 1**:`src/writeback_core.rs`——强类型 `EditOp` 六原语(serde + schema_version)+ `apply_writeback`(batch 单会话原子 + `delete_guards` force 分级 + `verify_commit` 强制 + 逐笔 refno 读回核验 + `element_diff` 摘要;任何失败 = 零输出字节)+ 文件包装(默认副本 `.e3dout`,未确认 in-place 拒绝)。sam7200 四测试全过(含护栏+verify 双闸拦危险删除);T106 GATE exit 0。
- **实测钉住的语义边界**:同批内 `InsertClone` 须排在其模板编辑之前(契约 E3-A1 注);`rename` = 改写既有 NAME 条目,无名元素首次命名留 005。
- 待续:Phase 2 队列层(`writeback_queue`,kv-mem)→ Phase 3 回声闭环 + CLI → Phase 4 文书。

### 变更 — E3D 增量落库收敛为单一强类型入口(specs/003,2026-06-11)

> `update_elements_to_database` 从 no-op 占位真实化为唯一增量落库入口;字符串拼接 SQL 时代落幕。

- **入口真实化**:`surreal_ingest.rs` 强类型行(`SurrealValue`/serde)upsert 写 `ses`/`pe_ses_h`/`pe`;记录 ID 由 `(dbnum, refno, sesno)` 确定性生成 ⇒ **重放幂等**(≥3 次重放计数/内容不变);`ingest_watermark` 水位单调升,已落会话跳过且可观测(`IngestReport`)。
- **测试基建**:kv-mem(`mem://`)全离线验收——共享运行时陷阱 ×2 入档(单线程 rt 卡死/全局连接绑定首 rt);真实样本端到端(sam7200 逐项对应、ams1112 计数耗时报告)。
- **存量收敛**:`to_surql` 空串占位与旧 `save_sessions_and_elements` 拼接路径删除,`collect_and_save_latest_data` 保存段委托唯一入口;`sync_history` 等注释坟场净删 ~944 行;`store_all_refno_sesno_map` 保留为**全库历史回填专用入口**(契约 D4 声明,拼接段记 004 候选)。
- **验收**:`--features surrealdb` workspace 构建+全套测试两道闸(T206/T304)均 exit 0;落库构造点 grep 单一入口;`crates/e3d_io` 零改动。

### 变更 — E3D I/O 三引擎收敛为单一格式核心(specs/002,2026-06-11)

> v1 `PdmsIO` 自带解析 / `engine_v2` / `e3d_io` 三套并存实现收敛为 **`crates/e3d_io` 单核心 + `PdmsIO` 门面**;切换为大爆炸(Q4=C,用户拍板),全程公共 API 零变更(`tests/api_freeze_c1.rs` 编译期冻结锁)。

- **页源抽象**:`e3d_io::page_source` 新增 `PageSource` trait + `InMemory`/`PagedFile`(LRU+命中统计,C3.2 页大小探测单源 `probe_page_size`,承接 v1 `PageManager` 语义)。
- **只读视图**:`e3d_io::read_view::Rdb<S>`(惰性影子页)——`btree_find` 目标式点查(sam7200 10392 键穷举双源等值)、`leaves` 枚举、`session_chain` 会话链、`element_record` 变长记录(ams1112 430 条 v1 逐字节 parity)。
- **`PdmsIO` 换芯**:探测/会话链/B 树点查/整树枚举/记录读取/字节路径逐项委托 `e3d_io`;`cache_hit_rate` 统计源切至影子页命中;索引磁盘缓存 `PIM1` 逐字节兼容(契约 C3.4=a)。
- **孤岛退役**:删除 `engine_v2`(39 文件)+ `verify_engine_v2`、v1 写路径(`writer.rs`/`element_serializer.rs`)、v1 读取辅助(`page_manager.rs`/`paged_reader.rs`/`element_record_reader.rs`)及 v1 B 树搜索族死代码;逆向知识归档 `docs/engine-v2-archaeology.md`(core.dll 六层函数对照 + 七项必保陷阱)。
- **验收**:workspace 构建 + 全套测试**历史首次全绿**(含 103MB ams1112 全库解析回归);`e3d_io` 独立 38+1 全绿、零第三方依赖;`parse_pdms_db` 定位为 `EleData` 类型适配层(FR-003)。

### 新增 — E3D/PDMS 元素数据**离线解析 + 安全写**(读/格式全闭环,IDA 2.10 权威 + 实测)

> 基于 AVEVA Everything3D 2.10 `core.dll` 逐函数反编译 + 真实样本(sam7200/acp7002/ams1112/amssys)交叉验证。给定元素记录即可纯文件离线解出 `noun · NAME · refno · owner · 全部隐式/显式(DA)属性 · 引用(连通+跨库目录) · owner 层级树`。

- **集成模块** `src/e3d_decode.rs`(std-only,接入 `lib.rs` `pub mod e3d_decode`):`SchemaSet`(跨库 `*vir.dat` typedef)/`Edb`/`decode_full`/`index_db`/`db1_dehash`/`set_inline_value`(安全在位写)。edition-2024 自检 **5 测试**(读计数/WELD POS/UDA/real+int+ref 在位写)。
- **独立 Rust 工具** `tools/e3d_decode_rs/`(完整读取/解码/JSON 导出 + 跨库引用解析);与 Python 工具链**属性级对齐**(目录库 100%、设计库 99.95%/implicit 0 diff)。
- **属性取值权威化**(`db4_get_ce_att` 0x10612A50 全函数反编译):type 枚举(2/6 实数、3/7 整、4/8/16 引用、5 布尔、10/15 文本、14/18 UDA 表)、`sel`(record[10]bit29)主/备 offset + packed/unpacked、标量/计数前缀、定宽表 `dbl_10F68E90`;**offset 磁盘来源 = 模式库 `*vir.dat`**(typedef skeleton K/I/J)。
- **UDA**:存储=DA 区以 hash(>0x171FAD39)为键的条目,real/int/text/ref 强类型值纯离线可解;真名需字典库(udalib)。
- **写侧**:页**无校验和**(`db1_read/write_page`),安全在位定长值写已实现验证;完整 COW 写机制(B 树插入/分裂 + `db5_save_work` 会话提交/page0 重指)已权威分析(未实现)。
- **修复**:Python 解码器 `e3d_attr_decoder.py` 文本 `latin1`→**UTF-8**(中文元素名 `/穹顶`/`/天花板` 不再乱码);短 skeleton 越界崩溃(amssys);大库 B 树遍历 leaf cap 截断(ams1112 真实 ~42 万元素);`detect_page_size` 纠错为 `字数×4`。
- **构建解阻**:`Cargo.toml` 暂移除缺失的可选 `dpcsync` path 依赖(`sync-archive` 去 `dep:dpcsync`;含恢复说明),使默认构建可解析。
- 文档:`docs/e3d 数据库分析/`(`E3D_DB_文件格式规范.md` §7.6–§7.10/§12、`E3D_DB_解析指导.md`、`E3D_DB_索引.md`、`离线属性解析_总结.md` + 工具脚本)与 `.planning/` 逆向记录。

### 修复 — 显式属性与元素记录边界对齐 core.dll

- 新增 `parse_packed_explicit_entry`，按 core.dll packed header（dab_type << 26 | payload_len_words）切分显式条目
- `parse_raw_explicit_attrs` 改为先切条目再解析表达式 payload，修复 PHEI 等 packed 表达式字段误读
- 显式块扫描终止条件补充 `hash == -1`，`collect_explict_data` 可识别 reserved prefix 后的负 hash 非表达式属性
- `ElementRecordReader` 不在首个未知 word 截断，避免丢失十几 KB 之后的 explicit block；新增相邻记录边界探测
- 元素记录读取上限改为 64KB，超限返回已读数据而非报错；`len_words=0` 块头改为跳过继续扫描

### 变更 — engine_v2 代码格式化

- 全模块 `rustfmt` 统一格式（`DbError` 变体、模块声明顺序等），无行为变更

### 清理

- 移除误提交的 `meilisearch.exe`、`nasm-installer.exe` 与 `MEILISEARCH_IMPLEMENTATION_SUMMARY.md`

### 文档与调试

- 新增 `docs/e3d 数据库分析/2026-05-27_core_dll显式属性读取对齐分析.md`
- 新增 `tests/aps7201_expression_debug.rs` 表达式调试入口

### 新增 — engine_v2 全新数据库引擎

> 基于 core.dll 逆向分析，按 db1~db5 五层架构从零构建，全 Rust 实现，弃用 Fortran I/O 依赖。

#### 公共类型 (`engine_v2::types`)

- `PageId` — (dbno, page_id, extent) 三元组页面标识
- `PageDescriptor` — pfno 池化描述符（lock_count + referenced bit + dirty）
- `PageType` / `PageSize` — 页面类型枚举与运行时大小检测
- `RefNo` — 8 字节双分量引用号
- `IndexEntry` / `IndexPageHeader` — B-树索引条目与页头
- `SessionPageData` / `DbHeader` — 会话页与文件头部
- `DbError` / `DbResult` — 统一错误类型

#### Rust I/O 层 (`engine_v2::io_layer`)

- `FileHandle` — 替代 FIOXST/FIONEW，基于 `std::fs::File`
- `DirectReader` — 替代 FHDBRN/DirectAccessToken，页面对齐读取
- `RetryPolicy` — 替代 SYWAIT+FHSWIT，err=11 重试 + reopen

#### db1 页面管理 (`engine_v2::db1`)

- `PageCache` — pfno 池化缓存池 + LRU 驱逐 + 脏页安全写回
- `PageIO` — 页面物理 I/O + 批量预读

#### db2 会话管理 (`engine_v2::db2`)

- `HeaderManager` — PdmsHeader 读写 + 库级属性存取
- `SessionManager` — 会话链遍历（latest→prev→...）+ 按 sesno 查找
- `ExtractManager` — Extract 记录管理（占位）
- `DbLookup` — 数据库查找表（占位）

#### db3 B-树索引 (`engine_v2::db3`)

- `BTreeNode` — 节点解析 + 起始标记过滤 + 二分查找
- `BTreeSearch` — FHSRCH 三级搜索（根→中间→叶）
- `BTreeInsert` — FHXPND 排序插入 + 容量触发分裂
- `BTreeSplit` — FHSPLT 叶/内部/根节点分裂
- `BTreeDelete` — FHDELT 删除 + underflow 处理（旧代码完全缺失）
- `TableIterator` — DB_IndexTableIterator 全量遍历
- `create_new_table` — 创建空 B-树根节点

#### db4 元素管理 (`engine_v2::db4`)

- `CurrentElement` — CE 导航栈 go_to/push/pop/clear（旧代码完全缺失）
- `AttrReader` / `AttrWriter` — 属性读写分派（int/real/string/ref/logical/array）
- `RefManager` — FOWN/LOWN/PREX/NEXX 引用关系管理
- `ElementPageHeader` / `ElementRecordHeader` — 元素页/续页布局
- `ContinuationReader` — 跨页数据结束位置检测
- `ElementOps` — 元素创建/拷贝（占位）

#### db5 高层 API (`engine_v2::db5`)

- `Database` — 统一入口（open_read/open_write/find_element/go_to_element/save/close）
- `DbOpen` — 只读/读写打开 + 页面大小自动检测 + 路径构造
- `DbClose` — flush + sync
- `DbSave` — save_work（脏页→头部更新→磁盘同步）

#### 集成 (`engine_v2::adapter`)

- `from_legacy_header` / `to_legacy_header` — 新旧 Header 互转
- `refno_from_u64` / `refno_to_u64` — RefNo 兼容转换

### 文档

- 新增 `docs/engine_v2_architecture.html` — Canvas 绘制的七层架构图