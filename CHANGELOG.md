# 更新日志

## [未发布]

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