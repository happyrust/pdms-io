# 更新日志

## [未发布]

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