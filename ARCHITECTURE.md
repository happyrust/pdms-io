# pdms_io 架构文档

> PDMS (Plant Design Management System) 数据库文件解析与分析工具

## 目录

- [项目概述](#项目概述)
- [技术栈](#技术栈)
- [项目结构](#项目结构)
- [核心架构](#核心架构)
- [PDMS 二进制格式](#pdms-二进制格式)
- [模块详解](#模块详解)
- [数据流](#数据流)
- [关键算法](#关键算法)
- [外部依赖关系](#外部依赖关系)
- [可执行文件](#可执行文件)
- [测试体系](#测试体系)
- [可选功能](#可选功能)

---

## 项目概述

`pdms_io` 是一个 Rust crate，用于解析和操作 PDMS/E3D（AVEVA 工厂设计管理系统）的二进制数据库文件。项目提供以下核心能力：

- **二进制解析**：读取 PDMS 专有格式的数据库文件（大端序，分页存储）
- **B+树索引搜索**：通过优化的 B+树算法快速定位参考号（refno）
- **增量变更检测**：按会话（session）范围收集元素的新增/修改/删除操作
- **文件同步**：基于内容寻址的分块同步与压缩
- **文件监控**：实时监视数据库文件变化
- **数据库写入**：构建并写入新的会话、索引和数据页面
- **搜索集成**：可选的 Meilisearch 全文搜索支持

---

## 技术栈

| 类别 | 技术 |
|------|------|
| 语言 | Rust (edition 2024, nightly) |
| 异步运行时 | tokio |
| 二进制解析 | deku (大端序)、nom (组合子解析) |
| 序列化 | serde / serde_json / rkyv |
| 并行处理 | rayon |
| 数据库 | SurrealDB (内存模式 kv-mem) |
| 搜索引擎 | Meilisearch (可选) |
| 文件监控 | notify |
| 内容同步 | dpcsync (Blake2b 哈希分块) |
| HTTP 客户端 | reqwest (rustls-tls) |

---

## 项目结构

```
pdms_io/
├── Cargo.toml                   # 主 crate 配置
├── build.rs                     # 构建脚本 (protoc 环境变量)
├── DbOption.toml                # 运行时数据库配置
├── pdms-test-data/              # 测试用 PDMS 二进制数据
│   ├── sam7200_0001             # 样本数据库文件
│   ├── ele_data_0/1            # 元素数据测试文件
│   └── att_data_0/1            # 属性数据测试文件
│
├── src/
│   ├── lib.rs                   # 模块入口与公共导出
│   ├── main.rs                  # 主程序 (PdmsWatcher)
│   │
│   │── ────── 核心层 ──────
│   ├── defines.rs               # PDMS 数据结构与页面类型定义
│   ├── io.rs                    # PdmsIO 主接口 (B+树搜索、增量收集)
│   ├── page_manager.rs          # LRU 页面缓存管理器
│   ├── paged_reader.rs          # 跨页连续读取器
│   ├── element_record_reader.rs # 变长元素记录读取器
│   ├── element_serializer.rs    # 元素序列化为 PDMS 二进制格式
│   ├── writer.rs                # 数据库写入层 (页面、会话、头部)
│   │
│   │── ────── 功能层 ──────
│   ├── search.rs                # Meilisearch 搜索集成 (feature: meilisearch)
│   ├── watch.rs                 # 文件系统监控 (PdmsWatcher)
│   ├── dblist.rs                # DBLIST 文本文件解析器
│   ├── config.rs                # 环境变量配置管理
│   ├── io_log.rs                # 日志配置
│   ├── common.rs                # 通用工具函数
│   │
│   │── ────── 同步层 ──────
│   ├── sync/
│   │   ├── mod.rs               # 同步模块入口
│   │   ├── sync.rs              # 归档压缩
│   │   ├── files.rs             # 远程文件同步
│   │   ├── compress.rs          # 分块去重压缩
│   │   ├── clone.rs             # 数据库克隆
│   │   └── utils.rs             # 工具函数
│   │
│   │── ────── 测试 ──────
│   ├── test/                    # 单元测试辅助
│   ├── tests/                   # 集成测试
│   ├── surql/                   # SurrealQL 占位模块
│   │
│   │── ────── 可执行文件 ──────
│   └── bin/                     # 13 个测试/工具二进制
│
├── crates/
│   └── parse_pdms_db/           # PDMS 二进制解析子 crate
│       └── src/
│           ├── lib.rs           # 解析器入口
│           ├── parse.rs         # 核心解析逻辑
│           ├── parser/
│           │   ├── attribute/   # 属性解析 (隐式/显式/表达式)
│           │   ├── element/     # 元素解析 (头部/子节点)
│           │   ├── database/    # 数据库解析 (头部/索引/校验)
│           │   ├── attlib/      # Attlib noun schema 解析
│           │   ├── primitives.rs
│           │   └── combinator.rs
│           └── test_cases/      # 解析器测试数据
│
└── tests/                       # 顶层集成测试
```

---

## 核心架构

### 分层设计

```
┌─────────────────────────────────────────────────────┐
│                  应用层 (Application)                  │
│  PdmsWatcher │ 搜索集成 │ CLI 工具 │ 文件同步           │
├─────────────────────────────────────────────────────┤
│               业务逻辑层 (Business Logic)               │
│  PdmsIO (B+树搜索 │ 增量收集 │ 变更检测)               │
├─────────────────────────────────────────────────────┤
│                读写层 (Read/Write)                     │
│  PagedReader │ ElementRecordReader │ ElementWriter    │
├─────────────────────────────────────────────────────┤
│               页面管理层 (Page Management)              │
│  PageManager (LRU 缓存)                               │
├─────────────────────────────────────────────────────┤
│               解析层 (Parsing)                         │
│  parse_pdms_db (nom 组合子 │ deku 结构体解析)           │
├─────────────────────────────────────────────────────┤
│               类型层 (Types)                           │
│  aios_core (RefU64 │ NamedAttrMap │ EleData)          │
└─────────────────────────────────────────────────────┘
```

### 核心类型关系

```
PdmsIO
 ├── PageManager (LRU 页面缓存)
 │    ├── File (std::fs)
 │    └── HashMap<PageKey, Vec<u8>>
 ├── PagedReader (跨页读取)
 ├── ElementRecordReader (变长记录)
 └── 使用 parse_pdms_db 解析
      └── 输出 EleData (aios_core 类型)

DatabaseWriter
 ├── ElementWriter (数据页写入)
 ├── DataPageWriter (页面分配)
 ├── SessionBuilder (会话页构建)
 └── HeaderUpdater (头部更新)
```

---

## PDMS 二进制格式

### 文件结构

PDMS 数据库文件采用固定大小的分页存储，所有多字节字段均为**大端序**（Big-Endian）。

| 偏移 | 内容 | 说明 |
|------|------|------|
| 0x00 - 0x3F | PdmsHeader (64 bytes) | 文件头 |
| 0x40 - ... | Pages[] | 数据页序列 |

### 页面大小

| 常量 | 大小 | 说明 |
|------|------|------|
| `PAGE_SIZE_512` | 512 字节 | 旧版 PDMS |
| `PAGE_SIZE_2K` | 2048 字节 | E3D/新版 PDMS（默认） |
| `PAGE_SIZE_4K` | 4096 字节 | 部分 E3D/PDMS |

页面大小由文件头的 `page_size` 字段声明，无效时回退到 2048 字节。

### 文件头 (PdmsHeader)

```
偏移    字段                  类型    说明
0x00    unknown_0_0           i32     未知
0x04    version               i32     版本号 (=2)
0x08    db_num                i32     数据库编号
0x0C    unknown_1_0           i32     未知 (=1)
0x10    unknown_1_1           i32     未知 (=1)
0x14    unknown_1_2           i32     未知 (=0)
0x18    flags                 i32     标志位 (=0xFFFFFFFF)
0x1C    unknown_1_4           i32     未知 (=0)
0x20    creation_time         u32     创建时间
0x24    unknown_2             i32     标志位 (=0xFFFFFFFF)
0x28    latest_ses_pgno       u32     最新会话页号
0x2C    ext_no                u32     扩展号
0x30    session_page_no       u32     会话页面号
0x34    page_size             u32     页面大小声明
0x38    stored_page_count     u32     存储页数
0x3C    unknown_3             u32     未知 (=2)
```

### 页面类型 (PageType)

| 值 | 枚举 | 说明 |
|----|------|------|
| 1 | RefArray | 引用数组页面 |
| 3 | Session | 会话页面（链式连接） |
| 5 | Data | 数据页面 |
| 7 | Special | 特殊页面 |
| 8 | Index | 索引页面（B+树节点） |

### 会话页面 (SessionPageData)

会话页面通过 `last_ses_pageno` 字段形成向前链表，从 `latest_ses_pgno`（文件头）开始可遍历所有会话。

```
偏移    字段                  类型    说明
0x00    page_type             i32     页面类型 (=3)
0x04    last_ses_pageno       i32     前一会话页号
0x08    last_ses_extno        i32     前一会话扩展号
0x0C    sesno                 i32     会话编号
0x10    unknown_0             i32     固定 0xFFFFFFFF
0x14    end_pgno              u32     会话最后一页页号
0x18    end_extno             u32     会话最后一页扩展号
0x1C    index_root_pageno     u32     索引根页号
0x20    index_root_extno      u32     索引根扩展号
0x24    claim_pageno          u32     声明页号
0x28    claim_extno           u32     声明扩展号
```

### 索引页面 (B+树)

索引页面构成 B+树结构，叶子节点存储 `RefnoDataLoc` 数组，内部节点存储子页面指针。

**RefnoDataLoc** (参考号定位):
```
字段          类型    说明
refno_high    u32     参考号高位（db_num）
refno_low     u32     参考号低位
pgno          u32     数据页号
offset_flag   u32     偏移量 (高20位) + 标志 (低12位)
```

### 数据页面

数据页面存储元素原始数据 (`EleRawData`)，包含：
- 参考号 (refno)
- 类型名 (noun)
- 父节点引用
- 隐式属性（位置、方向等固定属性）
- 显式属性（用户自定义属性）
- 子节点列表

---

## 模块详解

### `defines.rs` — 数据结构定义

定义所有 PDMS 二进制格式对应的 Rust 结构体，使用 `deku` 进行大端序反序列化。

**核心类型：**

| 类型 | 说明 |
|------|------|
| `PdmsHeader` | 64字节文件头 |
| `DbPageBasicInfo` | 文件头 + 最新会话 + 文件大小 |
| `SessionPageData` | 会话页面数据 |
| `IndexPageData` | B+树索引页面 |
| `RefnoDataLoc` | 参考号 → (页号, 偏移) 定位 |
| `ElePageData` | 元素数据页面 |
| `EleRawData` | 原始元素数据 |
| `PageType` | 页面类型枚举 |
| `DataPageSubtype` | 数据页子类型 |

### `io.rs` — 核心 I/O 接口 (~5700行)

项目最核心的模块，提供 `PdmsIO` 结构体作为所有数据库操作的统一接口。

**PdmsIO 主要方法：**

| 方法 | 说明 |
|------|------|
| `new() / open()` | 创建并打开数据库 |
| `read_pdms_header()` | 读取文件头 |
| `read_ses_data()` | 读取会话页面 |
| `read_index_data()` | 读取索引页面 |
| `search_refno_pgno_optimized()` | B+树优化搜索参考号 |
| `search_latest_refno()` | 查找最新参考号位置 |
| `parse_raw_element()` | 解析指定偏移的元素 |
| `auto_get_raw_element()` | 按参考号查找并解析元素 |
| `collect_increment_eles()` | 收集会话范围内的增量元素 |
| `build_index_map()` | 构建参考号 → 偏移映射 |
| `get_refno_operation_status()` | 获取参考号操作状态 (Add/Modify/Delete) |
| `get_page_basic_info()` | 获取页面基本信息 |

**关键数据类型：**

| 类型 | 说明 |
|------|------|
| `ModifiedElement` | 元素修改详情（含属性级别差异） |
| `EleOperationDetail` | 操作详情 (Add/Modified/Deleted/None) |
| `EleOperationData` | 按会话输出的操作数据 |
| `ElementHashOptions` | 元素哈希计算选项 |

### `page_manager.rs` — 页面缓存

基于 LRU 策略的页面缓存管理器，按 `PageKey = (ext_no, page_no)` 索引。

- **读取**：优先从缓存返回，缓存未命中时从磁盘加载
- **写入**：标记脏页，支持批量刷盘
- **统计**：命中率、加载次数、脏页数

### `paged_reader.rs` — 跨页读取

封装 `PageManager`，支持跨越页面边界的连续字节读取。当请求数据跨越两个或多个页面时，自动拼接。

### `element_record_reader.rs` — 变长记录读取

处理 PDMS 中变长元素记录的读取，记录可能跨越多个页面。自动管理缓冲区增长和段合并。

### `element_serializer.rs` — 元素序列化

将 `EleData` 结构体序列化回 PDMS 二进制格式，用于数据库写入。

### `writer.rs` — 数据库写入

| 类型 | 说明 |
|------|------|
| `ElementWriter` | 页面级别元素写入 |
| `DataPageWriter` | 数据页面分配与写入 |
| `SessionBuilder` | 构建会话页面字节流 |
| `HeaderUpdater` | 更新文件头 |
| `DatabaseWriter` | 组合所有写入操作 |

### `search.rs` — Meilisearch 搜索 (feature: meilisearch)

提供 PDMS 元素的全文搜索能力：

| 类型/方法 | 说明 |
|-----------|------|
| `MeilisearchConfig` | 配置 (默认 localhost:7700) |
| `ElementDocument` | 搜索文档结构 |
| `ElementSearchClient` | 搜索客户端 |
| `fuzzy_search()` | 模糊搜索 |
| `search_by_name()` | 按名称搜索 |
| `search_by_type()` | 按类型搜索 |
| `advanced_search()` | 高级搜索 |

### `watch.rs` — 文件监控

`PdmsWatcher` 使用 `notify` crate 监视 PDMS 项目目录，当数据库文件变化时触发回调。

- 扫描目录获取所有数据库文件头信息
- 初始化 CBA (Content-Based Archive) 归档
- 支持异步文件变化事件流

### `sync/` — 文件同步模块

| 文件 | 说明 |
|------|------|
| `compress.rs` | 基于 dpcsync 的分块去重压缩 (Blake2b512 哈希) |
| `clone.rs` | 从归档/HTTP 读取器克隆数据库 |
| `files.rs` | 从远程 HTTP 服务同步文件 |
| `sync.rs` | 归档压缩入口 |
| `utils.rs` | 人类可读大小格式化等工具 |

### `dblist.rs` — DBLIST 解析

解析 PDMS 的 DBLIST 文本文件，该文件定义了项目中数据库文件的列表和路径映射。支持续行合并和引号处理。

### `config.rs` — 配置管理

基于环境变量的配置，支持 `PDMS_TEST_PATH`、`PDMS_PROJECT_PATH` 等变量。

---

## 数据流

### 读取流程

```
PDMS 二进制文件
       │
       ▼
┌──────────────┐     ┌──────────────────────────┐
│ PageManager  │◄────│ 磁盘 I/O (std::fs::File)  │
│ (LRU 缓存)   │     └──────────────────────────┘
└──────┬───────┘
       │ get_page(ext_no, page_no)
       ▼
┌──────────────┐     ┌────────────────────────────┐
│ PagedReader  │────►│ ElementRecordReader        │
│ (跨页读取)    │     │ (变长记录, 段合并)           │
└──────┬───────┘     └────────────┬───────────────┘
       │                          │ raw bytes
       ▼                          ▼
┌─────────────────────────────────────────────────┐
│                  PdmsIO                          │
│  ┌─────────────────┐  ┌──────────────────────┐  │
│  │ B+树索引搜索     │  │ 增量元素收集           │  │
│  │ refno → (pg, off)│  │ 按会话范围遍历         │  │
│  └────────┬────────┘  └──────────┬───────────┘  │
│           │                      │               │
│           ▼                      ▼               │
│  ┌─────────────────────────────────────────────┐│
│  │ parse_pdms_db::parse_raw_ele_data()         ││
│  │ 原始字节 → EleData (结构化元素数据)           ││
│  └─────────────────────────────────────────────┘│
└───────────────────────┬─────────────────────────┘
                        │
        ┌───────────────┼───────────────┐
        ▼               ▼               ▼
┌──────────────┐ ┌──────────────┐ ┌──────────────┐
│ Meilisearch  │ │ SurrealDB    │ │  应用消费     │
│ 全文搜索索引  │ │ 结构化存储   │ │ (CLI/API)    │
└──────────────┘ └──────────────┘ └──────────────┘
```

### 写入流程

```
EleData (结构化元素)
       │
       ▼
┌──────────────────────┐
│ ElementSerializer    │
│ (序列化为二进制)       │
└──────────┬───────────┘
           │
           ▼
┌──────────────────────┐
│ DatabaseWriter       │
│ ├── DataPageWriter   │ ← 分配数据页, 写入元素
│ ├── ElementWriter    │ ← 更新索引条目
│ ├── SessionBuilder   │ ← 构建新会话页面
│ └── HeaderUpdater    │ ← 更新文件头
└──────────┬───────────┘
           │
           ▼
┌──────────────────────┐
│ PageManager          │
│ (脏页写回磁盘)        │
└──────────────────────┘
```

---

## 关键算法

### B+树索引搜索

PDMS 数据库使用 B+树索引将参考号（refno）映射到数据页位置。`search_refno_pgno_optimized()` 实现了优化的搜索算法：

1. **从索引根页开始**：通过 `SessionPageData.index_root_pageno` 获取根节点
2. **二分查找**：在每个索引页的 `refno_locs` 数组中进行二分查找
3. **路径选择优化**：利用页面层级信息减少不必要的 I/O
4. **叶子节点定位**：在叶子节点找到 `RefnoDataLoc`，包含 (pgno, offset)

性能提升达 99%+（相比暴力遍历）。可通过 `debug_btree_search` feature 启用调试输出。

### 增量变更检测

`collect_increment_eles()` 按会话范围收集元素变更：

1. 遍历指定会话范围的索引
2. 对每个参考号，比较当前版本与前一版本的数据
3. 生成 `EleOperationDetail`：
   - **Add**：新增元素（前一版本不存在）
   - **Modified**：属性级别差异对比（隐式/显式/UDA 属性的增删改）
   - **Deleted**：元素被删除

### 元素哈希

`ElementHashOptions` 支持自定义哈希计算策略，用于快速判断元素是否变化：

- 可忽略易变属性 (pgno, sesno 等)
- 可忽略自定义键
- 统计相邻变更计数器

---

## 外部依赖关系

### 内部 crate

```
pdms_io (主 crate)
├── aios_core (path: ../rs-core)
│   ├── 核心类型: RefU64, NamedAttrMap, EleOperation
│   ├── PDMS 类型: PdmsDatabaseInfo, NamedAttrValue
│   ├── 数据库集成: SUL_DB (SurrealDB 全局连接)
│   └── 工具: decode_chars_data, get_db_option
│
└── parse_pdms_db (path: crates/parse_pdms_db)
    ├── 解析函数: parse_raw_ele_data, parse_ele_data
    ├── 输出类型: EleData
    └── 解析器模块: attribute/, element/, database/, attlib/
```

### 关键外部依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| deku | 0.16.0 | 大端序二进制结构体解析 |
| nom | 7.1.3 | 组合子解析器 |
| tokio | 1.32+ | 异步运行时 |
| rayon | 1.8+ | 数据并行处理 |
| surrealdb | 自定义 fork | 结构化数据存储 (内存模式) |
| notify | 8.0 | 文件系统事件监控 |
| dpcsync | 自定义 | 内容寻址分块同步 |
| dashmap | 6.1 | 并发 HashMap |
| blake2 | 0.10.6 | 内容哈希 (文件同步) |
| reqwest | 0.11 | HTTP 客户端 (rustls) |
| meilisearch-sdk | 0.28.0 | 全文搜索 (可选) |
| glam | 0.30.9 | 向量/矩阵运算 |

---

## 可执行文件

项目包含 13 个二进制可执行文件 (`src/bin/`)：

| 二进制 | 说明 | 用法 |
|--------|------|------|
| `pdms_io` | 主程序：文件监控与写入测试 | `cargo run` |
| `test_get_refno_status` | 测试参考号操作状态查询 | `cargo run --bin test_get_refno_status -- <db_path> [refno]` |
| `test_increment_eles` | 测试增量元素收集 | `cargo run --bin test_increment_eles -- <db_path> [start] [end]` |
| `benchmark_increment_eles` | 增量处理性能基准测试 | `cargo run --bin benchmark_increment_eles` |
| `test_page_types` | 页面类型检测与验证 | `cargo run --bin test_page_types` |
| `test_header_fix` | 头部解析修复验证 | `cargo run --bin test_header_fix -- <db_path>` |
| `test_index_map` | 索引映射构建测试 | `cargo run --bin test_index_map` |
| `test_sesno_timestamp` | 会话时间戳处理测试 | `cargo run --bin test_sesno_timestamp` |
| `test_all_fixes` | 回归测试合集 | `cargo run --bin test_all_fixes` |
| `demo_latest_data_save` | 最新数据保存演示 | `cargo run --bin demo_latest_data_save` |
| `test_meilisearch` | Meilisearch 集成测试 | `cargo run --bin test_meilisearch --features meilisearch` |
| `test_meilisearch_simple` | 简单 Meilisearch 测试 | `cargo run --bin test_meilisearch_simple --features meilisearch` |
| `test_search_integration` | 搜索功能集成测试 | `cargo run --bin test_search_integration --features meilisearch` |
| `check_instances_json` | 实例 JSON 检查 | `cargo run --bin check_instances_json` |

---

## 测试体系

### 测试层次

| 层次 | 位置 | 说明 |
|------|------|------|
| 单元测试 | `src/test/` | 核心功能测试 (解析、参考号、增量收集等) |
| 模块集成测试 | `src/tests/` | 模块间集成 (PdmsIO 冒烟测试、索引缓存等) |
| 顶层集成测试 | `tests/` | 端到端测试 (DBLIST 回归、完整解析诊断等) |
| 性能基准 | `src/bin/benchmark_*` | 增量处理性能 |

### 测试数据

- `pdms-test-data/sam7200_0001`：样本 PDMS 数据库文件
- `pdms-test-data/ele_data_*`、`att_data_*`：元素和属性测试数据
- `crates/parse_pdms_db/src/test_cases/`：解析器专用测试数据

### 运行测试

```bash
cargo test --lib        # 单元测试 (71 通过, 5 忽略)
cargo test              # 全部测试 (含集成测试)
```

---

## 可选功能

| Feature | 说明 | 启用方式 |
|---------|------|---------|
| `debug_parse` | 启用解析调试输出 | `--features debug_parse` |
| `debug_btree_search` | 启用 B+树搜索调试输出 | `--features debug_btree_search` |
| `meilisearch` | 启用 Meilisearch 搜索支持 | `--features meilisearch` |

---

*文档生成日期: 2026-03-02*
