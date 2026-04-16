# PDMS DB Engine V2 — core.dll 全量复刻开发计划

> 创建日期：2026-04-13
> 状态：Draft
> 目标：完全弃用现有 pdms-io 旧实现，按 core.dll db1~db5 分层架构从零构建全新引擎

---

## 1. 背景

现有 `pdms-io-fork` 实现存在以下问题：

| 问题 | 现状 |
|---|---|
| 单文件巨型模块 | `io.rs` 5700+ 行，读写逻辑混杂 |
| 页面缓存简化 | 仅 `HashMap<(ext_no, page_no)>` 简单 KV，无 pfno 池化 |
| 缺 COW 语义 | `mark_dirty()` 单标记，未实现 `db1_update_page` 的临时页机制 |
| 缺索引删除 | B-树无 `FHDELT` 删除操作 |
| 无 CE 导航栈 | 无 Current Element 指针管理 |
| 无 Extract 管理 | `db2_insert_extract` / `db2_remove_extract` 缺失 |
| 无数据库压缩 | `db5_compact` 缺失 |
| 无多用户共享 | `db5_open_shared_db` / `db5_refresh_work` 缺失 |
| 无 opcode 调度 | 未对齐 E3D 3.1 dispatcher 模式 |

## 2. 设计原则

1. **模块 1:1 映射 core.dll**：db1/db2/db3/db4/db5 各自独立 module
2. **不复用旧代码**：现有 `io.rs` / `page_manager.rs` / `writer.rs` 全部弃用
3. **对齐原始调用链**：`db5 → db4 → db3 → db1`，包括 Fortran I/O 层语义
4. **逆向文档驱动**：所有实现均以 `docs/e3d 数据库分析/` 下的逆向分析文档为准

## 3. 模块结构

```
pdms-db-engine/          # 新 crate
├── src/
│   ├── db1/             # 页面管理层
│   │   ├── mod.rs
│   │   ├── page_cache.rs      # 缓存池 + LRU (pfno 池化描述符)
│   │   ├── page_io.rs         # 物理 I/O (FHDBRN/FHDBWN)
│   │   ├── page_lock.rs       # 页面锁定 (lock_count + referenced bit)
│   │   └── page_alloc.rs      # 页面分配 (db1_get_new_page + COW)
│   │
│   ├── db2/             # 会话与头部管理层
│   │   ├── mod.rs
│   │   ├── header.rs          # 文件头读写 (modify_header_page)
│   │   ├── session.rs         # 会话管理 (get_session_pgid + 会话链)
│   │   ├── extract.rs         # Extract 记录 (insert/remove_extract)
│   │   └── db_lookup.rs       # 数据库查找表 (find_db_data / create_lookup_entry)
│   │
│   ├── db3/             # B-树索引层
│   │   ├── mod.rs
│   │   ├── btree.rs           # B-树核心结构 (节点定义 + 索引页 header)
│   │   ├── search.rs          # FHSRCH: 索引查找 (根→中间→叶三级)
│   │   ├── insert.rs          # FHXPND: 索引插入+扩展
│   │   ├── split.rs           # FHSPLT: 节点分裂 (叶/内部/根)
│   │   ├── delete.rs          # FHDELT: 节点删除 (新增，旧代码完全缺失)
│   │   ├── iter.rs            # FHITER + DB_IndexTableIterator: 遍历
│   │   └── table.rs           # 索引表管理 (create_new_table)
│   │
│   ├── db4/             # 元素与属性管理层
│   │   ├── mod.rs
│   │   ├── element.rs         # 元素创建/销毁 (create_element / clear_stack)
│   │   ├── ce.rs              # CE (Current Element) 指针 + 导航栈
│   │   ├── attrs.rs           # 属性读写分派 (get/put integer/real/string/ref/logical)
│   │   ├── lists.rs           # 列表属性 (get_list / store_list)
│   │   ├── refs.rs            # 引用关系 (insert_ref / remove_ref / get_next_ext_ref)
│   │   ├── copy.rs            # 元素拷贝 (copy_user_element)
│   │   └── page_layout.rs     # 元素页布局 (type=5 元素页 / type=7 续页)
│   │
│   ├── db5/             # 数据库访问层 (高层 API)
│   │   ├── mod.rs
│   │   ├── open.rs            # 打开 (open_read_db / open_write_db / open_shared_db)
│   │   ├── close.rs           # 关闭 (close_db)
│   │   ├── save.rs            # 保存 (save_work / partial_save_work)
│   │   ├── compact.rs         # 压缩 (compact)
│   │   ├── refresh.rs         # 刷新 (refresh_work)
│   │   └── mark.rs            # 事务标记 (set_mark / undo)
│   │
│   ├── fortran_io/      # Fortran I/O 语义层
│   │   ├── mod.rs
│   │   ├── file_ops.rs        # FIOXST/FIONEW/FUDEL/FHLOSE
│   │   ├── direct_access.rs   # DirectAccessToken (对齐 vtable 语义)
│   │   └── retry.rs           # 读取重试 (SYWAIT + FHSWIT)
│   │
│   ├── types/           # 公共类型定义
│   │   ├── mod.rs
│   │   ├── page.rs            # PageId, PageDescriptor, PageType
│   │   ├── refno.rs           # RefNo (8B 双分量)
│   │   ├── session.rs         # SessionPageData
│   │   └── index.rs           # IndexEntry, RefnoDataLoc
│   │
│   └── lib.rs           # crate 入口
│
├── tests/               # 集成测试
├── benches/             # 性能基准
└── Cargo.toml
```

## 4. core.dll 函数映射表

### 4.1 db1 (页面管理)

| core.dll 函数 | 2.x 地址 | 3.1 地址 | Rust 对应 | 优先级 |
|---|---|---|---|---|
| `db1_get_page` | `0x1063FD10` | `0x5AEE4E0` | `db1::page_cache::get_page()` | P1 |
| `db1_read_page` | `0x1063B980` | `0x5AF0640` | `db1::page_io::read_page()` | P1 |
| `db1_write_page` | `0x1063EEF0` | `0x5AF1C30` | `db1::page_io::write_page()` | P1 |
| `db1_update_page` | `0x10640D80` | `0x5AF1600` | `db1::page_alloc::update_page()` | P1 |
| `db1_lock_page` | `0x1063BFA0` | `0x5AEFC30` | `db1::page_lock::lock_page()` | P1 |
| `db1_is_page_incore` | — | `0x5AF04A0` | `db1::page_cache::is_page_incore()` | P1 |
| `db1_plu_locate_entry` | `0x1063B2E0` | `0x5AEF150` | `db1::page_cache::locate_entry()` | P1 |
| `db1_get_new_page` | — | — | `db1::page_alloc::get_new_page()` | P1 |

### 4.2 db2 (会话管理)

| core.dll 函数 | 功能 | Rust 对应 | 优先级 |
|---|---|---|---|
| `db2_init` | 初始化会话管理器 | `db2::mod::init()` | P3 |
| `db2_modify_header_page` | 修改头部页 | `db2::header::modify()` | P3 |
| `db2_get_session_pgid` | 获取会话页 ID | `db2::session::get_pgid()` | P3 |
| `db2_insert_extract` | 插入 Extract 记录 | `db2::extract::insert()` | P3 |
| `db2_remove_extract` | 移除 Extract 记录 | `db2::extract::remove()` | P3 |
| `db2_find_db_data` | 根据库号定位数据块 | `db2::db_lookup::find()` | P3 |
| `db2_get_db_int_att` | 获取库级整型属性 | `db2::header::get_int_att()` | P3 |
| `db2_set_db_int_att` | 设置库级整型属性 | `db2::header::set_int_att()` | P3 |

### 4.3 db3 (B-树索引)

| core.dll 函数 | Fortran 桥接 | 功能 | Rust 对应 | 优先级 |
|---|---|---|---|---|
| `db3_get_page_entry` | `FHSRCH` | B-树搜索 | `db3::search::find()` | P2 |
| `db3_insert_page_entry` | `FHXPND` | 索引插入 | `db3::insert::insert()` | P2 |
| `db3_split_node` | `FHSPLT` | 节点分裂 | `db3::split::split_node()` | P2 |
| `db3_split_root` | — | 根分裂 | `db3::split::split_root()` | P2 |
| — | `FHDELT` | 节点删除 | `db3::delete::delete()` | P2 |
| `db3_scan_index_page` | `FHITER` | 索引页扫描 | `db3::iter::scan()` | P2 |
| `db3_start_table_search` | `Iterator::ctor` | 启动搜索 | `db3::iter::TableIterator::new()` | P2 |
| `db3_get_next_table_entry` | `Iterator::increment` | 获取下一条 | `db3::iter::TableIterator::next()` | P2 |
| `db3_create_new_table` | — | 创建索引表 | `db3::table::create()` | P2 |

### 4.4 db4 (元素管理)

| core.dll 函数 | opcode | 功能 | Rust 对应 | 优先级 |
|---|---|---|---|---|
| `db4_create_element` | 32 | 创建元素 | `db4::element::create()` | P4 |
| `db4_get_ce_att` | 80/106/... | 读属性 | `db4::attrs::get()` | P4 |
| `db4_get_list` | — | 读列表属性 | `db4::lists::get()` | P4 |
| `db4_store_list` | — | 写列表属性 | `db4::lists::store()` | P4 |
| `db4_insert_ref` | — | 建立引用 | `db4::refs::insert()` | P4 |
| `db4_remove_ref` | — | 删除引用 | `db4::refs::remove()` | P4 |
| `db4_copy_user_element` | — | 深拷贝元素 | `db4::copy::copy()` | P4 |
| `db4_clear_stack` | 38 | 清除导航栈 | `db4::ce::clear_stack()` | P4 |
| `db4_get_att_dets` | — | 属性元信息 | `db4::attrs::get_details()` | P4 |
| `db_go_to_element` | 108 | CE 导航 | `db4::ce::go_to()` | P4 |

### 4.5 db5 (访问层)

| core.dll 函数 | opcode | 功能 | Rust 对应 | 优先级 |
|---|---|---|---|---|
| `db5_open_read_db` | 134 | 只读打开 | `db5::open::read()` | P5 |
| `db5_open_write_db` | 138 | 读写打开 | `db5::open::write()` | P5 |
| `db5_open_shared_db` | — | 共享打开 | `db5::open::shared()` | P5 |
| `db5_close_db` | — | 关闭 | `db5::close::close()` | P5 |
| `db5_save_work` | — | 保存 | `db5::save::save_work()` | P5 |
| `db5_compact` | 28 | 压缩 | `db5::compact::compact()` | P5 |
| `db5_refresh_work` | 300 | 刷新 | `db5::refresh::refresh()` | P5 |
| `db5_set_mark` | — | 事务标记 | `db5::mark::set_mark()` | P5 |

## 5. 分阶段实施

### Phase 0 — 基础设施 (1周)

**目标**：搭建 crate 骨架 + 公共类型 + I/O 抽象

- [ ] 创建 `pdms-db-engine` crate，配置 Cargo.toml
- [ ] `types/page.rs`：PageId (dbno, page_no, extent 三元组)、PageDescriptor、PageType enum
- [ ] `types/refno.rs`：RefNo (refno_0 + refno_1 双分量)
- [ ] `types/session.rs`：SessionPageData 结构
- [ ] `types/index.rs`：IndexEntry、RefnoDataLoc
- [ ] `fortran_io/file_ops.rs`：FileToken trait（封装打开/关闭/模式切换）
- [ ] `fortran_io/direct_access.rs`：DirectAccessToken（对齐 vtable 语义）
- [ ] `fortran_io/retry.rs`：读取重试（err=11 → wait → switch mode → retry）
- [ ] 单元测试框架搭建

**验收标准**：所有类型定义 + FileToken trait 编译通过

### Phase 1 — db1 页面管理 (1周)

**目标**：完整复刻 core.dll 页面缓存机制

- [ ] `page_cache.rs`：
  - pfno 池化描述符（固定大小数组，非 HashMap）
  - 三元组 `(dbno, page_id, extent)` 匹配 (`is_page_incore`)
  - LRU 驱逐策略 (`plu_locate_entry`)
  - 预读机制 (`dword_6A5415C > 1` 时批量读取)
  - 本地文件缓存 (optional, 对齐 `sub_5BCC210`)
- [ ] `page_io.rs`：
  - `read_page()`：通过 DirectAccessToken 读取
  - `write_page()`：通过 DirectAccessToken 写入
  - 页面类型分类 (type=5 元素页, type=7 续页)
- [ ] `page_lock.rs`：
  - lock_count 引用计数
  - referenced bit（双态标记）
  - 锁定页面不参与 LRU 驱逐
- [ ] `page_alloc.rs`：
  - `get_new_page()`：分配新页
  - `update_page()` COW：分配临时页 → 拷贝 → 标记脏 → 原页位回写
  - 脏页安全驱逐（写回磁盘后再释放）

**验收标准**：用真实 .db 文件做 round-trip 读取，与旧实现字节级一致

### Phase 2 — db3 B-树索引 (1.5周)

**目标**：完整 B-树 CRUD + 迭代器

- [ ] `btree.rs`：
  - 索引页结构：header 0x1C 字节 (type + noun + level + unknowns + pfno)
  - 条目结构：16 字节/条 (refno_0 + refno_1 + pgno + packed_offset_flag)
  - 页类型常量 (INDEX_PAGE_NOUN = 0x00CC47DF)
- [ ] `search.rs` (FHSRCH)：
  - RootIndexPage 解析 → lower/upper 子树选择
  - RefnoIndexPage 遍历 → 叶子页定位
  - IndexPageData 精确匹配
- [ ] `insert.rs` (FHXPND)：
  - 叶子节点插入 (排序插入)
  - 内部节点边界更新
  - 容量检查 → 触发 split
- [ ] `split.rs` (FHSPLT)：
  - 叶子页分裂 (中点拆分)
  - 内部页分裂 (保留 start_marker)
  - 根节点分裂 (提升树高)
- [ ] `delete.rs` (FHDELT)：
  - 叶子节点删除
  - 合并/重平衡 (underflow 处理)
  - 级联更新父节点边界
- [ ] `iter.rs`：
  - `TableIterator::new()` (对齐 `DB_IndexTableIterator::ctor`)
  - `TableIterator::next()` (对齐 `DB_IndexTableIterator::increment`)
  - `scan_page()` (FHITER)
- [ ] `table.rs`：创建新索引表

**验收标准**：索引重建 + 全量遍历与旧实现结果一致；插入→分裂→查找 round-trip 正确

### Phase 3 — db2 会话管理 (0.5周)

**目标**：会话链 + 头部 + Extract

- [ ] `header.rs`：
  - PdmsHeader 布局（0x00~0x3F 各字段读写）
  - `modify_header_page()`
  - `get_int_att()` / `set_int_att()`
  - `get_arr_att()` / `set_arr_att()`
- [ ] `session.rs`：
  - 会话页解析 (type=3)
  - 会话链遍历 (latest_ses_pgno → prev 链)
  - `get_session_pgid()`
  - 会话页构建 (含时间戳/计算机名/注释)
- [ ] `extract.rs`：
  - `insert_extract()`
  - `remove_extract()`
- [ ] `db_lookup.rs`：
  - 数据库块查找表 (find / create / find_empty / find_current)
  - 辅助数据库块检测 (`there_are_aux_db_blocks`)

**验收标准**：会话链完整性校验通过；头部读写 round-trip 正确

### Phase 4 — db4 元素管理 (2周)

**目标**：完整的元素 CRUD + 属性读写 + CE 导航

- [ ] `page_layout.rs`：
  - 元素页 (type=5) 解析/写入：页头 + 元素记录列表 + 空闲空间
  - 续页 (type=7) 链接拼接：`ElementRecordReader` 等价
  - 元素记录序列化：impl_len(4B) → refno(8B) → type_hash(4B) → owner(8B) → attrs
- [ ] `ce.rs`：
  - Current Element 指针管理
  - 导航栈 (push/pop/clear)
  - `go_to_element(refno)` (opcode 108)
- [ ] `attrs.rs`：
  - 按类型分派读取 (integer/real/string/reference/logical)
  - 按类型分派写入 (put_integer/put_string/put_reference/...)
  - 属性元信息查询 (`get_att_dets`)
  - 隐式属性 (固定偏移) vs 显式属性 (变长)
- [ ] `lists.rs`：
  - 整型数组读写 (`get_int_array` / `put_int_array`)
  - 引用数组读写 (`get_ref_array` / `put_ref_array`)
  - 实数数组读写 (`get_real_array`)
- [ ] `refs.rs`：
  - 父子引用建立 (`insert_ref`)
  - 引用删除 (`remove_ref`)
  - 外部引用遍历 (`get_next_ext_ref`)
  - 成员列表维护 (FOWN/LOWN/PREX/NEXX)
- [ ] `element.rs`：
  - `create_element()` (opcode 32)：分配页空间 + 初始化元素头
  - `init_element_page()`
- [ ] `copy.rs`：
  - `copy_user_element()`：深拷贝元素及所有属性

**验收标准**：元素全量解析 + round-trip 序列化一致性；CE 导航栈功能测试

### Phase 5 — db5 高层 API (1周)

**目标**：面向应用层的完整 API

- [ ] `open.rs`：
  - `open_read_db(dbno, session)`：只读模式 (mode=7)
  - `open_write_db(dbno)`：读写模式 (独占锁)
  - `open_shared_db(dbno)`：共享模式 (多用户)
  - 文件名构造：`<project_dir>/<project_name>NNN` (3位补零)
- [ ] `close.rs`：
  - `close_db()`：同步缓存 + 释放句柄
- [ ] `save.rs`：
  - `save_work()`：脏页 flush → 索引更新 → 会话页写入 → 头更新
  - `partial_save_work()`：部分保存
  - `undo_failed_flush()`：撤销失败刷新
- [ ] `compact.rs`：
  - `compact()`：数据库压缩，整理碎片空间
- [ ] `refresh.rs`：
  - `refresh_work()`：重新加载其他用户最新修改
- [ ] `mark.rs`：
  - `set_mark()`：设置事务回滚标记点
  - `undo()`：回滚到标记点

**验收标准**：完整 open → modify → save → reopen → verify 流程

### Phase 6 — 集成与迁移 (1周)

**目标**：将 pdms-io-fork 主入口切换到新引擎

- [ ] 在 Cargo.toml 中添加 `pdms-db-engine` 依赖
- [ ] 创建 adapter 层：将旧 `PdmsIO` API 代理到新引擎
- [ ] 回归测试：现有所有 test case 通过
- [ ] 性能基准对比 (bench)
- [ ] 迁移文档 + 旧代码废弃标记

**验收标准**：所有现有测试通过 + 性能不劣于旧实现

## 6. 关键技术决策

| 维度 | 旧实现 | 新实现 (core.dll 对齐) |
|---|---|---|
| 缓存结构 | `HashMap<(ext_no, page_no)>` | pfno 池化描述符数组 + (dbno, page_id, extent) 三元组 |
| 脏页处理 | `mark_dirty()` 单标记 | COW: 分配临时页 → 拷贝 → 标脏 → 原页回写 |
| 索引操作 | 无删除 | FHSRCH + FHXPND + FHSPLT + FHDELT 全套 |
| 代码结构 | `io.rs` 5700行单文件 | db1~db5 五层分离，每层独立 module |
| 调度模式 | 直接函数调用 | opcode dispatcher (可选, 为 FFI/harness 铺路) |
| 元素访问 | 无状态式 parse | CE 指针 + 导航栈 |
| Extract | 缺失 | `db2_insert_extract` / `db2_remove_extract` |
| 压缩 | 缺失 | `db5_compact` |
| 多用户 | 缺失 | `open_shared_db` + `refresh_work` |
| I/O 重试 | 无 | err=11 → SYWAIT(0.5s) → FHSWIT → retry |
| 页面大小 | 硬编码 2K | 运行时检测 (512/2K/4K) |

## 7. 依赖关系图

```
Phase 0 (types + fortran_io)
    │
    ├──→ Phase 1 (db1: page management)
    │        │
    │        ├──→ Phase 2 (db3: B-tree index)
    │        │        │
    │        │        └──→ Phase 4 (db4: element management)
    │        │                 │
    │        │                 └──→ Phase 5 (db5: high-level API)
    │        │                          │
    │        │                          └──→ Phase 6 (integration)
    │        │
    │        └──→ Phase 3 (db2: session management)
    │                 │
    │                 └──→ Phase 5 (db5)
    │
    └──→ Phase 3 (db2)
```

## 8. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|---|---|---|
| `db1_update_page` COW 语义理解偏差 | 写入数据损坏 | IDA Pro 二次校验 + 参照 `2026-04-11_Phase1_2.10_db1基线校准.md` |
| `FHDELT` B-树删除逻辑复杂 | 索引结构损坏 | 先实现软删除(标记)，后实现物理合并 |
| 多用户共享模式的文件锁竞争 | 数据不一致 | Phase 5 实现，参考 `FHSWIT` 模式切换逻辑 |
| 与旧实现的 API 不兼容 | 迁移困难 | Phase 6 adapter 层做缓冲 |

## 9. 预计工期

| Phase | 内容 | 工期 |
|---|---|---|
| P0 | 基础设施 | 1 周 |
| P1 | db1 页面管理 | 1 周 |
| P2 | db3 B-树索引 | 1.5 周 |
| P3 | db2 会话管理 | 0.5 周 |
| P4 | db4 元素管理 | 2 周 |
| P5 | db5 高层 API | 1 周 |
| P6 | 集成与迁移 | 1 周 |
| **合计** | | **约 8 周** |

## 10. 参考文档

- `docs/e3d 数据库分析/db2_db5_驱动层分析总结.md`
- `docs/e3d 数据库分析/数据库读取解析架构.md`
- `docs/e3d 数据库分析/2026-04-11_Phase1_2.10_db1基线校准.md`
- `docs/core_dll_harness_plan.md`
- `ida_exports/` (IDA Pro 导出数据)
