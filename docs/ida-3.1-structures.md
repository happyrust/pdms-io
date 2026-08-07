# E3D 3.1 Core.dll 数据库结构恢复

**目标**: Everything3D 3.1 `core.dll`  
**IDB**: `D:\AVEVA\Everything3D3.1\core.dll.i64`  
**MD5**: `b7def476fb703917923c5b9e54717c69`  
**分析日期**: 2026-05-11  
**方法**: IDA Pro MCP 反编译 + 现有 2.10 文档交叉验证 + fixture 文件实测验证

---

## 1. 与 E3D 2.10 的关键差异

| 项目 | E3D 2.10 | E3D 3.1 | 证据 |
|---|---|---|---|
| Base address | ~0x10xxxxxx | 0x5170000 | `survey_binary` |
| 函数命名 | `db1_get_page` 等显式前缀 | Fortran 大写 (`FHDBRN`) + C API (`db_*`) | `func_query` |
| 文件字节序 | 大端 (BE) | 大端 (BE) | fixture 验证 (Fortran 遗留) |
| page_size 单位 | 未知 | 4 字节字（×4 得字节数） | fixture: offset 0x34=512, 实际页=2048 字节 |
| 主数据页 type_id | 7618321 (0x743F11) | 7618377 (0x7434F9) | `decompile sub_5AFB660` |
| 索引页 type_id | 13393183 (0xCC5D1F) | 13387743 (0xCC47DF) | `decompile sub_5AEB6B0` |
| IDA 类型表 | 未知 | 0 个 E3D 领域结构体 | `type_query`: 129 structs 全为 Windows/CRT |

**结论**: 2.10 文档的页面结构模型整体正确，但**魔术常量不同**。3.1 的所有具体数值必须从 IDA 重新验证。

---

## 2. 函数分层架构

```
┌──────────────────────────────────────────────────┐
│ db5: C API 包装器 (db_*)                          │
│   db_read_page, db_open_read_db, db_go_to_element │
│   模式: db_*(args) → dispatcher(CMD, args)        │
│   地址范围: 0x5AAxxxx                              │
├──────────────────────────────────────────────────┤
│ db5: Dispatcher 分发器                             │
│   日志、命令历史、错误追踪                          │
│   命令名表: off_6003900[cmd] → 字符串名             │
│   地址范围: 0x5ACxxxx-0x5ADxxxx                     │
├──────────────────────────────────────────────────┤
│ db4: 核心实现 (元素/记录/属性)                      │
│   sub_5AEB6B0 (go_to_element)                      │
│   sub_5AF8230 (get_header_integer)                  │
│   地址范围: 0x5AExxxx-0x5AFxxxx                     │
├──────────────────────────────────────────────────┤
│ db2: 页 I/O + 重试/模式切换                         │
│   sub_5AFB660 (页缓存管理)                          │
│   sub_5AF0640 (单页读取 + 3次重试)                  │
├──────────────────────────────────────────────────┤
│ db1: Fortran 文件处理器 (FH*)                       │
│   FHDBRN (页读取), FHDBWN (页写入)                  │
│   FHSPLT (B+tree split), FHFIND (索引查找)          │
├──────────────────────────────────────────────────┤
│ db1: C++ I/O 桥接                                   │
│   sub_5B9B400: IToken → DirectAccessToken           │
│   通过 RTDynamicCast + vtable[4] 做实际磁盘 I/O    │
├──────────────────────────────────────────────────┤
│ db0: 文件层 (FL*)                                   │
│   FLOPEN, FLCLOS, FLPAGE, FLFINI                   │
└──────────────────────────────────────────────────┘
```

**全局状态**:
- `dword_6453B98` (0x6453B98): 全局错误码，0 = 成功
- `off_6003900` (0x6003900): 命令名字符串指针表
- `dword_6453DC0`, `dword_6453DC4`: 当前数据库状态指针

---

## 3. 页面结构

### 3.1 通用页面头 (PageHeader)

每个数据库页面以 24+ 字节头部开始。

| 偏移 | 大小 | 字段 | 说明 | 置信度 | 证据 |
|---|---|---|---|---|---|
| +0 | 4 | page_type | 页面类型 (1/3/5/7/8) | 高 | doc + `*v3 == 5` in sub_5AFB660 |
| +4 | 4 | type_id | 类型标识/魔术常量 | 高 | doc + `v3[1] == 7618377` |
| +8 | 4 | db_handle | 数据库句柄 | 高 | doc + FHDBRN 参数 `4 * *a4` |
| +12 | 4 | ext_no | 扩展号 | 中 | doc; 3.1 未直接验证 |
| +16 | 4 | page_no | 页面号 | 中 | doc + 错误串 `Pageno is %d` |
| +20 | 4 | bucket_id | 桶 ID (低13位) | 中 | doc + `v38 & 0x1FFF` |

### 3.2 页面类型

| 类型 | 页面号 | 用途 |
|---|---|---|
| 0 (descriptor) | 0 | 数据库描述符，元数据属性 |
| 1 | 2 | 引用数组页面 |
| 3 | 由 descriptor 指向 | 会话页面 |
| 5 | 3-N | 数据页面（元素记录、索引） |
| 7 | - | 特殊/元数据页面 |
| 8 | - | 索引页面 |

### 3.3 数据页面子类型 (Type 5)

| type_id (3.1) | 十六进制 | 用途 | 置信度 | 证据 |
|---|---|---|---|---|
| 7618377 | 0x7434F9 | 主数据页（元素记录） | 高 | `sub_5AFB660: v3[1] == 7618377` |
| 13387743 | 0xCC47DF | 索引/B+tree 页面 | 高 | `sub_5B01340(dbno, 13387743, ...)` |

---

## 4. RefNo 编码

RefNo 由两个 32 位字组成。word1 必须 >= 0。

**数据库号提取**:

```
dbno = (word0 & 0x1FFF) | ((word0 >> 13) & 0x3E000)
     = bits[0:12] 拼接 bits[26:30] << 13
```

**证据**: `sub_5AEB6B0` 入口: `v3 = *a1 & 0x1FFF | (v2 >> 13) & 0x3E000`

**哈希索引**: `hash_slot = word1 % hash_table_size`

---

## 5. 页面地址编码

索引搜索 (`sub_5B01340`) 返回的结果编码为两个 32 位字：

| 字段 | 位 | 说明 | 证据 |
|---|---|---|---|
| page_number | 第 1 个字完整 | 页面号 | `v35 = v37` 传给 `sub_5AEE4E0` |
| slot_offset | 第 2 个字 bits[0:12] | 页内槽位偏移 | `v36 = v38 & 0x1FFF` |
| record_slot_index | 第 2 个字 bits[13:24] | 记录槽位索引 | `v19 = (v38 >> 13) & 0xFFF` |

---

## 6. 元素页面条目 (Element Page Entry)

数据页面内的每个元素记录槽位包含：

| 偏移 (相对于 slot) | 大小 | 字段 | 说明 | 置信度 |
|---|---|---|---|---|
| 4 * slot_index | 2 | record_type_or_len | 16 位值（记录类型或长度） | 中 |
| 4 * slot_index + 4 | 4 | refno_word0 | 元素 RefNo 第 1 字 | 高 |
| 4 * slot_index + 8 | 4 | refno_word1 | 元素 RefNo 第 2 字 | 高 |
| 4 * slot_index + 12 | 4 | record_reference | 记录引用值，用于记录查找 | 中 |

**证据**: `sub_5AEB6B0 LABEL_97`: `v34 = *(_DWORD *)(v32 + 4*v31 + 4)` 与 `*a1` 比较

---

## 7. 元素导航栈 (Element Navigation Stack)

用于层次遍历。全局变量 `dword_6A54024` 指向当前栈。

**栈头** (16 字节):

| 偏移 | 字段 | 说明 |
|---|---|---|
| +0 | next_ptr | 下一个栈（链表） |
| +4 | prev_ptr | 前一个栈（双向链表） |
| +8 | stack_depth | 当前深度（-1 = 空） |
| +12 | stack_id | 唯一标识符 |

**每个栈条目** (60 字节, 15 个 DWORD):

| DWORD 索引 | 字段 | 说明 | 置信度 |
|---|---|---|---|
| 0 | page_context_ptr | 页面上下文指针 | 中 |
| 1 | element_page_buf | 已加载的元素页面缓冲区 | 高 |
| 2-3 | ref_counts | 引用计数 | 低 |
| 4 | refno_word0 | 元素 RefNo 第 1 字 | 高 |
| 5 | refno_word1 | 元素 RefNo 第 2 字 | 高 |
| 6 | page_addr | 索引查找返回的页面地址 | 高 |
| 7 | page_offset_encoded | 编码的页面偏移 | 高 |
| 8 | dbno | 数据库号 | 高 |
| 9-10 | db_tokens | 数据库子令牌 | 中 |
| 11 | page_ptr | 已加载页面指针 | 高 |
| 12 | slot_index | 页内记录槽位索引 | 高 |
| 13 | slot_value | 槽位处读取的 16 位值 | 中 |
| 14 | dirty_flag | 脏标记（始终初始化为 0） | 高 |

---

## 8. Per-DB 状态结构 (216 字节)

`db_open` 时通过 `malloc(0xD8)` 分配。包含 5 个相同的 10-DWORD 文件令牌子结构。

| DWORD 索引 | 字段 | 说明 | 证据 |
|---|---|---|---|
| 4 | mode | 数据库模式 (1=read, 2=write) | `v58[4]` 与 open 参数比较 |
| 8 | db_type_id | 数据库类型 ID | 与模板表 `dword_6A54198` 匹配 |
| 9 | db_version | 数据库版本号 | `abs32(v58[9])` 与模板版本比较 |
| 85 | linked_page_a | 多文件一致性链接页 | 文件链验证 |
| 103 | file_type_count | 文件类型条目数 | 循环边界 |
| 104+ | file_type_ids[] | 文件类型 ID 数组 | 循环查找 |

---

## 9. B+tree 索引搜索

**搜索函数**: `sub_5B01340`

```c
sub_5B01340(
    int dbno,           // 数据库号
    int tree_type_id,   // 索引树类型 (13387743 = RefNo 索引)
    int* search_key,    // 搜索键 (RefNo = 2 个 DWORD)
    int key_size,       // 键大小 (2)
    int* result,        // 输出: 页面地址 + 编码偏移
    int* search_mode    // 搜索模式 (2 = 精确匹配?)
)
```

- **错误码 534**: 未找到
- **证据**: `sub_5AEB6B0`: `sub_5B01340(dbno, 13387743, a1, 2, &v37, &v43)`

---

## 10. 页读取调用链

```
db_read_page(token, pageno, buf)
  └→ sub_5AD5EE0(288, token, pageno, buf)     [dispatcher: 日志+错误]
      └→ sub_5AFB660(token, pageno, buf)       [页缓存: 读头页+目标页]
          └→ sub_5AF0640(token, pageno, buf, handle)  [单页读: 重试+模式切换]
              ├→ FHDBRN(&token, &pageno, buf, &handle)  [Fortran 解引用]
              │   └→ sub_5B9B400(token, pageno, buf, handle*4, 1)
              │       └→ DirectAccessToken::vtable[4]()  [磁盘 I/O]
              ├→ [失败 error 11] SYWAIT(0.5s) → 重试
              ├→ [再次失败] FHSWIT(CLOSED) → FHSWIT(READ) → 重试
              └→ [最终失败] FLFINI(ABORT)
```

---

## 11. 全局变量索引

| 地址 | 名称 | 用途 |
|---|---|---|
| 0x6453B98 | dword_6453B98 | 全局错误码 |
| 0x6453BA0 | byte_6453BA0 | 错误消息缓冲区 (512 字节) |
| 0x6453DC0 | dword_6453DC0 | 当前 DB 头页缓冲区 |
| 0x6453DC4 | dword_6453DC4 | 当前 DB 句柄/令牌 |
| 0x6453B70 | dword_6453B70 | 命令追踪回调 |
| 0x6423A28 | dword_6423A28 | 错误记录回调 |
| 0x6003900 | off_6003900 | 命令名字符串指针表 |
| 0x6A54024 | dword_6A54024 | 当前元素导航栈 |
| 0x6A5401C | dword_6A5401C | 栈链表头 |
| 0x6A54030 | dword_6A54030 | 当前数据库号存储 |
| 0x6A540C8 | dword_6A540C8 | 元素哈希表大小 |
| 0x6A540CC | dword_6A540CC | 元素哈希表基址 |
| 0x6A54190 | dword_6A54190 | 已打开数据库表 |
| 0x6A54180 | dword_6A54180 | 已打开数据库数量 |

---

## 12. 不确定字段与后续工作

以下字段需要进一步反编译验证：

1. **PageHeader ext_no (+12)**: 2.10 文档说是扩展号，3.1 未在反编译中直接确认
2. **Element page entry 的 record_type_or_len**: 16 位值的确切含义（长度 vs 类型标志）
3. **Navigation stack entry 的 ref_counts (DWORD 2-3)**: 可能是引用计数或其他标志
4. **Per-DB state 的 DWORD 85/91/92**: 多文件一致性机制的细节
5. **B+tree node 内部结构**: sub_5B01340 内部的节点布局尚未反编译
6. **Session page 详细字段**: 3.1 的会话结构可能与 2.10 有差异
7. **Record segment 跨页读取**: 大记录如何分段存储在多个页面中

每个不确定字段都需要目标函数的反编译 + fixture 字节验证才能提升置信度。

---

## 13. 只读 Rust IO 核心架构

基于上述恢复的 E3D 3.1 结构，设计一个只读 Rust 库，目标是打开 E3D 数据库文件、按 RefNo 定位元素、读取原始记录并暴露结构化属性视图。

### 13.1 模块划分

```
e3d_reader/
├── src/
│   ├── lib.rs              # 公共 re-exports
│   ├── page/
│   │   ├── mod.rs           # PageHeader, PageType, RawPage
│   │   ├── io.rs            # 页级文件 I/O (seek + read)
│   │   └── cache.rs         # 只读页缓存 (LRU or simple HashMap)
│   ├── meta/
│   │   ├── mod.rs           # DbDescriptor, FileInfo
│   │   ├── descriptor.rs    # 解析 page 0: 属性 ID → 偏移映射
│   │   ├── file_info.rs     # 解析 page 1: 签名、页大小、页数
│   │   └── constants.rs     # E3D 3.1 魔术常量 (type_ids, property_ids)
│   ├── session/
│   │   ├── mod.rs           # SessionPage, SessionWalker
│   │   └── session.rs       # 解析 type-3 页面, 回溯 session 链
│   ├── index/
│   │   ├── mod.rs           # IndexSearch, SearchResult
│   │   └── btree.rs         # B+tree 只读搜索 (对应 sub_5B01340)
│   ├── record/
│   │   ├── mod.rs           # RawRecord, ElementRecordView
│   │   ├── element_page.rs  # 解析 type-5 数据页内的元素槽位
│   │   ├── raw_record.rs    # 按槽位读取原始字节 (含跨页分段)
│   │   └── view.rs          # 结构化视图: RefNo, 属性, 层次引用
│   ├── refno.rs             # RefNo 类型: 编码/解码, dbno 提取
│   ├── engine.rs            # ReadOnlyEngine: 只读 public API
│   └── error.rs             # E3dReaderError 枚举
```

### 13.2 核心类型

```rust
/// 元素引用号：两个 u32，编码了 dbno 和元素标识
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct RefNo {
    pub word0: u32,
    pub word1: u32,
}

impl RefNo {
    pub fn dbno(&self) -> u32 {
        (self.word0 & 0x1FFF) | ((self.word0 >> 13) & 0x3E000)
    }
    pub fn is_valid(&self) -> bool {
        (self.word1 as i32) >= 0 && self.dbno() > 0
    }
}

/// 页面地址：页号 + 编码偏移
pub struct PageAddress {
    pub page_number: u32,
    pub slot_offset: u16,      // bits[0:12] of word1
    pub slot_index: u16,       // bits[13:24] of word1
}

/// 通用页面头
pub struct PageHeader {
    pub page_type: PageType,
    pub type_id: u32,
    pub db_handle: u32,
    pub ext_no: u32,
    pub page_no: u32,
    pub bucket_id: u16,
}

pub enum PageType {
    Descriptor,    // 0
    RefArray,      // 1
    Session,       // 3
    Data,          // 5
    Special,       // 7
    Index,         // 8
    Unknown(u32),
}

/// 数据库描述符 (page 0)
pub struct DbDescriptor {
    pub page_size: u32,
    pub db_mark: u32,
    pub session_page_no: u32,
    pub total_pages: u32,
    pub bucket_type_id: u32,
}

/// 文件信息 (page 1)
pub struct FileInfo {
    pub signature: u32,
    pub page_size: u32,
    pub page_count: u32,
    pub ext_size: u32,
    pub max_ext: u32,
}

/// 元素记录的结构化视图
pub struct ElementRecordView {
    pub refno: RefNo,
    pub page_address: PageAddress,
    pub raw_bytes: Vec<u8>,
    // 后续可扩展属性解析
}

/// 只读引擎打开选项
pub struct OpenOptions {
    pub path: PathBuf,
    pub cache_pages: usize,  // 页缓存容量，默认 256
}
```

### 13.3 只读 Public API

```rust
pub struct ReadOnlyEngine { /* ... */ }

impl ReadOnlyEngine {
    /// 打开数据库文件（只读）
    pub fn open(options: OpenOptions) -> Result<Self, E3dReaderError>;

    /// 获取数据库元数据
    pub fn descriptor(&self) -> &DbDescriptor;
    pub fn file_info(&self) -> &FileInfo;
    pub fn page_size(&self) -> u32;
    pub fn page_count(&self) -> u32;

    /// 回溯 session 链
    pub fn sessions(&self) -> Result<Vec<SessionInfo>, E3dReaderError>;

    /// 按 RefNo 查找元素（B+tree 索引搜索）
    pub fn find_element(&self, refno: RefNo)
        -> Result<ElementRecordView, E3dReaderError>;

    /// 读取指定页的原始字节
    pub fn read_page(&self, page_no: u32) -> Result<Vec<u8>, E3dReaderError>;

    /// 遍历所有元素（全页扫描）
    pub fn scan_elements(&self)
        -> Result<impl Iterator<Item = ElementRecordView>, E3dReaderError>;
}
```

### 13.4 数据流

```
                    用户调用
                       │
                       ▼
               ┌─────────────┐
               │   engine     │  ReadOnlyEngine::find_element(refno)
               └──────┬──────┘
                       │
            ┌──────────┼──────────┐
            ▼          ▼          ▼
       ┌─────────┐ ┌────────┐ ┌────────┐
       │  meta   │ │ index  │ │ record │
       │ 读描述符│ │B+tree  │ │读元素页│
       │ 读文件头│ │搜索    │ │解析槽位│
       └────┬────┘ └───┬────┘ └───┬────┘
            │          │          │
            └──────────┼──────────┘
                       ▼
                 ┌───────────┐
                 │   page    │  PageCache + PageIO
                 │ 缓存+读取 │
                 └─────┬─────┘
                       │
                       ▼
                   文件系统
                  (std::fs)
```

**读取流程**:

1. `engine.open()` → 读取 page 0 (descriptor) + page 1 (file_info) → 确定 page_size
2. `engine.find_element(refno)` → 提取 dbno → index.btree_search(refno) → 得到 PageAddress
3. PageAddress → page.read(page_number) → 从缓存或磁盘加载
4. record.parse_slot(page_buf, slot_index) → 验证 RefNo 匹配 → 构建 ElementRecordView

### 13.5 设计决策

| 决策 | 选择 | 原因 |
|---|---|---|
| I/O 模型 | 同步 `std::fs::File` | 只读场景，避免 async 复杂度 |
| 页缓存 | `HashMap<u32, Vec<u8>>` + LRU 淘汰 | 简单有效；只读无脏页 |
| 错误处理 | `thiserror` 枚举 | 可区分 I/O 错误、格式错误、未找到 |
| 字节序 | **大端** (`from_be_bytes`) | 数据库文件沿用 Fortran 大端约定，fixture 验证确认 |
| page_size | offset 0x34 的值 × 4 | 字段存储 4 字节字数（如 512 字=2048 字节），fixture 验证确认 |
| RefNo | `Copy` 类型 | 8 字节，频繁传递 |
| 跨页记录 | 延后到实现阶段 | 需要更多 IDA 证据确认分段机制 |

### 13.6 明确非目标

本只读核心**不实现**：

- 写回、`save_work`、session commit
- Claim/release 元素锁定
- Refresh / multiwrite merge / compact
- 实时 `core.dll` FFI 调用
- 完整的属性类型解析（先证明读取路径）
- Multi-extent 支持（需要更多 IDA 证据）
- Extract 语义
- 与旧 `PdmsIO` / `writer.rs` 的兼容层

### 13.7 与旧代码的关系

现有 `crates/pdmsdb_engine_v2/` 已有 db1-db5 分层，可作为**对照参考**：

| 旧模块 | 新模块 | 关系 |
|---|---|---|
| `db1/page_store.rs` | `page/io.rs` + `page/cache.rs` | 新模块从 3.1 IDA 证据重建 |
| `db2/header.rs`, `db2/session.rs` | `meta/`, `session/` | 新模块使用 3.1 常量 |
| `db3/index.rs` | `index/btree.rs` | 新模块基于 sub_5B01340 签名 |
| `db4/record_reader.rs`, `db4/ce.rs` | `record/` | 新模块使用 3.1 元素页面条目结构 |
| `db5/mod.rs`, `core/mod.rs` | `engine.rs` | 新 API 限定只读 |
| `fortran_io/` | 不需要 | Rust 直接文件 I/O，无需模拟 Fortran |

旧代码的**假设不可直接继承**，但其模块边界和类型命名可以参考。每个新类型的字段偏移必须追溯到 `struct_layouts.json` 中的 IDA 证据。

---

## 14. Noun Template / Attribute Schema 调用链

> **新增 2026-05-16，Slice 4 Step 14** — IDA-evidence-driven 调用链文档，
> 为 `e3d31-attribute-parsing` Followup #3 (DB-internal noun template loader)
> 提供 turn-key 实施手册。本节信息全部来自 `user-ida-pro-mcp.decompile` 在
> 本会话期间获取的 `sub_5AF6AB0` / `sub_5B03900` / `sub_5AF0640` / `sub_5AECBC0`
> / `sub_5AA9270` 解读。

### 14.1 调用链全图

```
DB_Noun::ReadData (0x58D6D20)
    │  lazy-init: 第一次访问 noun 时填 DB_Noun +236/+240 attribute hash list
    ▼
DB_DB::convertToDabType (0x58FA670)
    │  noun_class → DAB template type id
    ▼
sub_5AC32F0(536, ...)  ←  DAB dispatcher
    │  op 536 → off_6003900[536] = "db_get_att_list_for_given_template" (DGTALT)
    ▼
sub_5B03900  ←  GALFE = Get Att List For Element
    ├─ a1 == 0 (default template path):
    │   v7 = *(BYTE**)(dword_6A54024 + 60 * dword_6A54024[2] + 16)
    │   → template payload pointer (current DB context)
    └─ a1 != 0 (named template path):
        sub_5AF6AB0(template_type, attr_hash, mode, &out_ptr, ...)
            ↓
            descriptor lookup → child binary-search by hash → cached chunks
            ↓
            sub_5AF0640(handle, token, buf, count)  ←  raw chunk reader
                ↓
                FHDBRN(&handle, &token, buf, &count)  ←  Fortran file page read
                    (与 attlib.dat / ams1112_0001 主 DB 读路径同一机制)
```

### 14.2 Template Payload Layout (核心数据结构)

`sub_5B03900` 在 `v7 = template_payload_ptr` 上的访问揭示了 layout：

| 偏移 | 大小 | 字段 | 含义 | 证据 |
|---|---|---|---|---|
| +36 (= `((int*)v7)[9]`) | 4 | count | 该模板中 attribute slot 数量 | `if ( *((int *)v7 + 9) <= 0 ) return 1` |
| +56 + 4·i | 4 | attr_hash[i] | 第 i 个 attribute 的 PDMS base-27 hash | `*(_DWORD *)&v7[4 * v9 + 56]` 用作 token + 写到 a3 输出 |
| +60 + 4·i | 4 | stride[i] | 跳到下一个 attribute slot 的字偏移（最小 12） | `v9 += *(_DWORD *)&v7[4 * v9 + 60]` (`i+=stride` 推进游标) |
| +64 + 4·i | 4 | aux/token[i] | 该 attribute 的辅助数据/token（写到 a4 输出） | `*(_DWORD *)(a4 + 4 * (*v8)++) = *(_DWORD *)&v20[4 * v9 + 64]` |

**注意**: `stride` 字段是变长的（每个 attribute 自己声明跳多少 dword），所以 attribute records 是**变长 record，按 stride 串联**，不是固定 12-byte / 16-byte。`+56 / +60 / +64` 是第一个 attribute record 的基址 offset；后续 attribute 在 `+56 + cumulative_stride`。

排除集：`dword_6423A38` (size `dword_6423A40`) 是要从输出中过滤掉的 attribute hashes（GALFE 内部跳过它们）。

### 14.3 Template Descriptor Table (`dword_6A54198`)

`sub_5AF6AB0` 的入口在 `dword_6A54198` 数组里按 `a1`（template type id）线性搜索：

| 数组步长 | 24 bytes (6 dwords) |
|---|---|
| 数组项数 | `dword_6A54178` |

**每个 descriptor (24 bytes)**:

| DWORD 索引 | 字段 | 含义 | 证据 |
|---|---|---|---|
| 0 | type_id | descriptor 匹配的 template type id | `while (*v8 != a1) ++v7; v8 += 6;` |
| 2 | file_handle | 传给 `sub_5AF0640` 的 file handle | `sub_5AF0640(*(_DWORD *)((char *)dword_6A54198 + v52 + 8), ...)` |
| 3 | child_count | 子条目数（attribute 数） | `v13 = *((_DWORD *)v11 + 3)` |
| 4 | child_array_ptr | 子条目数组（28 bytes/child） | `v57 = *((_DWORD *)v11 + 4)` |
| 5 | cache_ptr | child cache 数组（12 bytes/child） | `v17 = *((_DWORD *)v48 + 5)` |

**每个 child (28 bytes, sorted by hash 用于 binary search)**:

| 偏移 | 字段 | 用途 |
|---|---|---|
| +0 | hash | binary-search key (`a2` 参数) |
| +4 | token1 | 第一个 chunk 的 file token |
| +8 | size1 | 第一个 chunk 字节数 |
| +12 | token2 | 第二个 chunk token |
| +16 | size2 | 第二个 chunk 字节数 |
| +20 | token3 | 第三个 chunk token |
| +24 | size3 | 第三个 chunk 字节数 |

**Cache (12 bytes/child)**: 三个 lazy-loaded chunk 指针（`malloc` 出来，存入对应位置）。`sub_5AF0640` 一次读 511 records，多 chunk 拼接。

### 14.4 Per-DB Active Template Index (`dword_6A54024`)

是个 runtime per-context pointer，由 `sub_5AA9270` 系列 dispatcher 在 DAB op 调用前后切换：

```c
dword_6A5402C = dword_6A54024;        // save
dword_6A54024 = dword_6A54028;        // load incoming context
sub_5ABB0D0(op_code, ...);            // execute op against this DB context
dword_6A54028 = dword_6A54024;        // save back
dword_6A54024 = dword_6A5402C;        // restore previous
```

布局 (per-DB context block, 60 bytes per template entry):

| 字段 | 偏移 | 用途 |
|---|---|---|
| `dword_6A54024 + 0` | 0 | base of `template_entries[]` |
| `dword_6A54024 + 8` | 8 | active template index (`>=0` valid; `<0` 表示未初始化) |
| `dword_6A54024 + 60 * i + 16` | 16 inside slot | **template payload pointer** (上面 §14.2 layout 的根) |
| `dword_6A54024 + 60 * i + 48` | 48 | dbno 之类 (写到 `*(_DWORD *)dword_6A54030`) |

`sub_5AECBC0` 是初始化检查 (`return !dword_6A54024 || *(int *)(dword_6A54024 + 8) < 0;`)。

### 14.5 实施建议 (For `e3d-io::record::template`)

1. **第一阶段：模拟 GALFE on default template**
   - 实现一个 `load_default_template(engine, db_handle)` 返回 `NounTemplate { count, attrs: Vec<(hash, aux, stride)> }`。
   - 数据源：`dword_6A54024 + 60 * dword_6A54024[2] + 16` 指向的 template payload。
   - **关键未知**：default template payload 在 DB 文件里的实际 page/byte 位置。需要在 `db_open` 路径里追踪 `dword_6A54028` 的初始化点（候选：`sub_5AECBC0` 上游、DB 文件 type-3/type-7 页面）。

2. **第二阶段：模拟 GALFE on named template**
   - 实现 `load_template(engine, template_type, attr_hash)`：模拟 `sub_5AF6AB0` 的 binary-search + chunk-fetch 流程。
   - 数据源：`dword_6A54198` 数组的 child entries 列出 `(token1, size1, token2, size2, token3, size3)`。
   - 关键依赖：实现 token-based chunk 读取，对应 `sub_5AF0640` → `FHDBRN` 的语义（基本上是 DB 文件 page+offset 读）。

3. **第三阶段：engine 集成**
   - `engine.summarize_element(refno, attlib, max_chain_depth)`：先尝试 `load_template(noun_hash)`；若成功，按 template 的 `(attr_hash, aux, stride)` 列表替换全局 ATGTDF-position 路径；若失败（template 缺失），回退到现有路径。

4. **验证**：fixture 上对 NXTR / STWALL / PIPE 至少跑三个 noun，比较 `before` 与 `after` 的 attribute 列表，应是对齐或新增。

### 14.6 已被 `dword_6423A38` 排除的 attributes

GALFE 输出时会过滤掉 `dword_6423A38[0..dword_6423A40]` 列表中的 hashes（哪些 attributes 在某些上下文不应暴露）。如果实现 template-aware 解码后输出和现有不一致，先检查这个排除列表。

---

### 14.7 Template `.dat` File On-Disk Format (Followup #3b — RESOLVED)

> **新增 2026-05-17，Slice 4 Step 16** — `dword_6A54028` 追踪解开后的最终发现：
> noun template payload **不在主 DB 文件里**，而是在 `%AVEVA_DESIGN_EXE%/<dbname>.dat`
> 一系列虚拟 schema 文件中（如 `desvir.dat`, `padvir.dat`, `catvir.dat`）。
> 这些 `.dat` 文件采用与 `attlib.dat` 相同的 Fortran `"DB,BL 512"` 格式
> （2048 字节/页，大端 u32），由 IDA `sub_5AFA740` (`2OTDB:Name is...`) 加载。

#### 14.7.1 关键发现

**IDA 函数链** (`user-ida-pro-mcp.decompile`, 2026-05-17):

| 函数 | 作用 |
|---|---|
| `DB_DBSchema::openSchema` (`sub_591D510`) | 构造路径 `"%AVEVA_DESIGN_EXE%/" + DB::getFileName(this) + ".dat"`，FHFIND 打开，调 sub_5AAEA80 |
| `sub_5AAEA80(template_type, file_handle)` | thin wrapper → `sub_5AD0730(136, ..., ...)` (dispatch cmd 136) |
| `sub_5AD0730(136, a1, a2)` | dispatcher → `sub_5AFA740(a1, a2)` |
| `sub_5AFA740(template_type, file_handle)` | 真正的 loader：读 header / 验 magic / 加载 children chunked / 装入 `dword_6A54198` |
| `sub_5AF6AB0(template_type, attr_hash, mode, &out, ...)` | 查询：descriptor linear scan + child binary search + chunk 缓存读 |
| `sub_5AF0640(file_handle, token, buf, count)` | 原始 chunk 读取（FHDBRN wrap + retry） |

**`dword_6A54028` 真正语义**：它是**第二个 active context 槽**，由 dispatcher (sub_5AA9270 系列) 在 DAB op 调用前后保存/恢复使用，**不存储 template payload**。`dword_6A54024[active_idx*60 + 16]` 存的是**已加载 template payload 的指针**，但 payload 本身的字节是从 `<dbname>.dat` 的 page 通过 `sub_5AF0640` 读上来的，运行时缓存到 `cache_ptr` 槽位（`dword_6A54198[descriptor].child[i].cache[0..2]`）。

#### 14.7.2 File Header (8 dwords @ page 0 / Fortran token=1)

`sub_5AFA740` 通过 `sub_5AF0640(file_handle, 1, &header[0], 8)` 读 8 dwords：

| 偏移 (dword) | 字段 | 含义 | IDA 验证 |
|---|---|---|---|
| 0 | magic | 必须 == 6（page type marker） | `if (v30 != 6) error 517` |
| 1 | (unnamed) | 未引用 | — |
| 2 | template_type | 必须 == 调用方传入的 `a1` | `if (v31 != a1) error 548`；存入 descriptor[+0] |
| 3 | aux1 | 不明用途 | 存入 descriptor[+4] |
| 4 | (unnamed) | 未引用 | — |
| 5 | child_count | 该文件中 noun template 个数 | 存入 descriptor[+12] |
| 6 | (unnamed) | 未引用 | — |
| 7 | chunk1_token | child array 第一个 chunk 的 Fortran 1-indexed page 号 | 作为 `v34` 初始 token |

**fixture 实测** (`D:\AVEVA\Everything3D3.1\desvir.dat`):
- `magic=6` ✓
- `template_type=0x000B0692` (= 722578，DESIGN DB 类型 id)
- `child_count=1113`（该文件中有 1113 个 noun 模板）
- `chunk1_token=2352`（第一个 chunk 在 Fortran page 2352 = 0-indexed page 2351）

#### 14.7.3 Child Array Format (28 bytes / 7 dwords per child, sorted by hash)

每个 child 描述一个 noun 模板的物理存储位置：

| 偏移 (dword) | 字段 | 用途 |
|---|---|---|
| 0 | hash | 该 noun 的 PDMS hash（binary-search key） |
| 1 | tok1 | 第一个 chunk 的 token |
| 2 | sz1 | 第一个 chunk 字节数（实际是 dword 数） |
| 3 | tok2 | 第二个 chunk 的 token |
| 4 | sz2 | 第二个 chunk dword 数 |
| 5 | tok3 | 第三个 chunk 的 token |
| 6 | sz3 | 第三个 chunk dword 数 |

`sub_5AFA740` 用 `realloc(28 * child_count)` 分配 child 数组，并 calloc 一个 `12 * child_count` 字节的 cache 数组（每 child 3 个 lazy-loaded chunk 指针）。

**重要**：child 数组按 hash 升序排序，`sub_5AF6AB0` 用 binary search 查找。

#### 14.7.4 Chunked-Read Protocol (511 + 1 chain pointer)

`sub_5AFA740` 加载 child 数组（`7 * child_count` dwords）和 `sub_5AF6AB0` 加载 payload chunk 都用同一个 chunked-read 模式：

```
remaining = total_data_dwords
cur_token = initial_token  // header[7] for children, child.tokN for payloads
buf_offset = 0
while remaining > 0:
    if remaining > 511:
        read 512 dwords from cur_token into buf[buf_offset..]
        cur_token = buf[buf_offset + 511]   // chain pointer (Fortran token)
        buf_offset += 511                    // overwrite chain pointer on next iteration
        remaining -= 511
    else:
        read remaining dwords from cur_token into buf[buf_offset..]
        remaining = 0
```

**为什么是 511 + 1**：FHDBRN 一次最多读 512 dwords（= 1 page），其中前 511 是 data，第 512 dword 是下一个 chunk 的 Fortran token（链表式跨页存储）。这避免了要求大型 schema 数据连续存储。

**fixture 实测**：`desvir.dat` 中 NXTR (hash=0x000DBF71) 在 child #178，`tok1=1237 sz1=247 tok2=1238 sz2=21 tok3=0 sz3=0`。247 dwords < 511，单次 chunk 读完。

#### 14.7.5 Template Payload Layout (Within a Single Child)

`sub_5AF6AB0` 加载 child 的 chunk1（最常用），返回字节缓冲。`sub_5B03900` (GALFE) 按 §14.2 的布局解析：

```
payload[9]      = count (number of attribute slots in this noun template)
payload[14+v9]  = attr_hash[i]      // v9 is cumulative dword cursor
payload[15+v9]  = stride[i]         // dword offset to next slot record WITHIN the payload
payload[16+v9]  = aux[i]            // slot-kind hint (14/15/16/17/18 etc. per sub_5AB3620)
v9 += stride[i]                     // advance cursor
```

**关键澄清** (本步骤新发现，修订早期理解)：

`stride` 是 **payload-internal 游标步长**（用于在模板字节流里跳到下一个 attr record），**不是** element data 里的 slot 字节大小。Element data 的 slot 偏移仍由全局 ATGTDF position 决定（per IDA `ATNLOG` `sub_55BC98B`：`value_word = page_cache[base + atgtdf_index]`，其中 `atgtdf_index` 是 attr 在 ATGTDF 全局表里的 1-based 位置）。

所以 noun template 的贡献是**告诉你这个 noun 究竟有哪些 attribute**（一个 ATGTDF 全表的子集），不是告诉你它们在 element 里占多少 byte。

**fixture 实测**：NXTR template chunk1（247 dwords）：
- `count = 21`（不是过去的 9 或 30 — 真实 noun 有 21 个 schema attrs）
- 前 20 个 records 的 stride 为 10 或 11（**非均匀**；老的 `stride=1` 假设错了）
- 前 5 attrs：`0x04D852B8/0x0009C18E/0x000D1FAA/0x000853B1/0x00083787`

#### 14.7.6 实施建议 (Rust loader — 替换 §14.5 第一/二阶段)

1. **新模块 `e3d-io::record::template_file`**：
   - `TemplateFile::open(path)` — 打开 `.dat` 文件
   - `TemplateFile::read_header() -> TemplateHeader { magic, template_type, child_count, chunk1_token }`
   - `TemplateFile::read_children(&header) -> Vec<TemplateChild>` — chunked read 7*child_count dwords，链跳 511+1 模式
   - `TemplateFile::find_child(hash, &[TemplateChild]) -> Option<&TemplateChild>` — binary search
   - `TemplateFile::read_payload(child) -> Vec<u32>` — chunked read chunk1 (+ chunk2/3 if present)

2. **扩展 `e3d-io::record::template`**：
   - `NounTemplate::from_template_payload(payload: &[u32], noun_hash: u32) -> Option<NounTemplate>` — 按 §14.7.5 walk
   - `NounTemplate::load_from_template_file(path, template_type, noun_hash) -> Result<...>` — 端到端 wrapper

3. **更新 engine 集成**：
   - `summarize_element_with_template` 应该用 `attlib.atgtdf_position(hash)` 而不是 cumulative stride，因为 stride 是 payload-internal 不是 element-data slot 大小（见 §14.7.5）。

4. **验证**：在 `desvir.dat` 上加载 NXTR template（count=21），跑 engine.summarize，对比旧 ATGTDF-position path（9 attrs）。

#### 14.7.7 已知 `.dat` 文件清单（fixture：`D:\AVEVA\Everything3D3.1\`）

| 文件 | 大小 | 推测对应 DB 类型 |
|---|---|---|
| `attlib.dat` | 5.8 MB | 属性库（已用，与 noun template 无关） |
| `desvir.dat` | 4.8 MB | DESIGN DB schemas（NXTR/STWALL/PIPE/EQUIPMENT 等） |
| `padvir.dat` | 985 KB | PADDS DB schemas |
| `catvir.dat` | 733 KB | Catalog DB schemas |
| `manvir.dat` | 464 KB | MANufacturing |
| `engvir.dat` | 389 KB | ENGineering |
| `schvir.dat` | 260 KB | SCHematic |
| `sysvir.dat` | 237 KB | SYStem |
| `alyvir.dat` / `dicvir.dat` / `glbvir.dat` / `provir.dat` / ... | 较小 | 其他 DB 类型 |

每个 `.dat` 文件 declare 自己的 `template_type`（header 第 2 个 dword）。Fixture 上 `ams1112_0001` 是 DESIGN DB，所以对应文件是 `desvir.dat`。

