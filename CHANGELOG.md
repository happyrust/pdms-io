# 更新日志GPT-5.5 Extra HighOpus 4.8 1M Max

## [未发布]

### 修复 — refno B-tree 索引全量枚举（spec 007）

> 修复部分 E3D2.1 DB 经索引只枚举到个位数 refno 的缺陷（`aps250160_0001`: 2→2748；`aps7351_0001`: 8→3,345,855，与 scan oracle 全等）。

- `refno_index.rs::parse_index_page` 不再用 `offset+0x10` 的 declared count 截断 entry，改为按页容量读到 `ref0 == 0`（真实 DB 该字段常为 2 但页内有效 entry 可达上百条）
- full enumeration 与 fallback leaf walk 统一遍历 internal page 的 start marker child（`80000001/80000001` 指向左侧/基础子树）；leaf 仍不把 marker 当元素
- `choose_child_pages()`：target 小于首个有效 key 时优先下钻 start marker child；乱序对（删除空洞）两侧子页与 marker 均纳入候选并去重
- 同 refno 多记录维持 `pos` 最大 wins（与旧 scan 从文件尾保留最新记录语义一致）
- 新增 4 只合成单测：entry 零终止、start marker 子树枚举、start marker child 单点查找、duplicate refno latest-wins
- 新增诊断工具 `examples/probe_scan_only_sessions.rs`：沿 session 链逐版本枚举索引，定位 scan/index 差异来源
- scan-only 残差豁免结论：`aps250160_0001` 的 11 条 scan-only 的索引 entry 存在于最新 session，但 loc 指向远超文件页数的失效页（如 `elem_pg=32258`，文件仅 449 页），`entry_from_loc()` 越界校验正确排除；scan 捕获的是残留历史记录字节

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