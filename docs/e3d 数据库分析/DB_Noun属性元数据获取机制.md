# DB_Noun 属性元数据获取机制

> 基于 IDA Pro 逆向 core.dll + pdms-io-fork 源码分析
> 分析日期：2026-04-15

## 概述

E3D/PDMS 中，每个元素（Element）都属于某个**类型（Noun）**，如 PIPE、ELBO、TEE、VALV、EQUI 等。类型系统决定了元素拥有哪些属性、属性的数据类型和存储方式。本文档分析属性元数据从"磁盘文件"到"运行时可用"的完整链路。

---

## 1. 核心数据结构

### 1.1 DB_Noun::dictionary_（运行时类型注册表）

```cpp
// core.dll 中的全局静态成员
// 符号: ?dictionary_@DB_Noun@@1V?$map@HPBVDB_Noun@@...@std@@A
protected: static std::map<int, DB_Noun const*> DB_Noun::dictionary_;
```


| 字段    | 说明                                                |
| ----- | ------------------------------------------------- |
| Key   | `int` — Noun 的 hash 值（由 `db1_hash("ELBO")` 等计算得出） |
| Value | `DB_Noun const*` — 指向 DB_Noun 常量对象的指针             |
| 作用域   | 全局静态，进程生命周期内有效                                    |
| 填充时机  | 数据库打开/初始化阶段                                       |


### 1.2 DB_Noun 对象

每个 `DB_Noun` 实例代表一个元素类型，封装以下信息：

- **系统属性列表**：该类型拥有的所有属性（hash 集合）
- **类型层级**：`actualType` → `hardType` → `NOUN_UNKNOWN` 的继承链
- **UDET 信息**：用户自定义元素类型的内容类型（`contentType`）

### 1.3 DB_Attribute::dictionary_（属性定义字典）

```cpp
// 与 DB_Noun::dictionary_ 类似的全局字典
// 地址: DB_Attribute::findAttribute @ 0x1075FB35
static std::map<int, DB_Attribute*> DB_Attribute::dictionary_;
```


| 字段    | 说明                                                         |
| ----- | ---------------------------------------------------------- |
| Key   | `int` — 属性 hash（如 `db1_hash("XLENGTH")`）                   |
| Value | `DB_Attribute*` — 属性定义对象（含 type, dtyp, size, defi, ityp 等） |
| 填充时机  | **按需懒加载**（首次 `findAttribute(hash)` 时从底层读取并缓存）              |


---

## 2. 属性元数据的获取链路

### 2.1 运行时路径（core.dll 原始实现）

```
┌─────────────────────────────────────────────────────────────┐
│  数据库初始化                                                │
│  DB_Noun 对象创建 → dictionary_[noun_hash] = &noun          │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  DB_Noun::getSystemAttributes(std::set<int>& out_hashes)    │
│  地址: 0x107694AD                                           │
│                                                             │
│  调用 db_get_attribute_list(...) 拉取属性 hash 列表          │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  db_get_attribute_list(...)                                  │
│  地址: 0x1093DD60                                           │
│                                                             │
│  薄封装：把 opcode=60 分派给通用调度器                        │
│  内部调用 sub_10953100(60, ...)                               │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  sub_10993900(...)                                           │
│  地址: 0x10993900                                           │
│                                                             │
│  实际填充输出数组：                                           │
│  - 将属性 hash 列表写入调用者传入的两段数组缓冲区              │
│  - 回填实际数量                                              │
│  - 数据源: attlib 中的 ATNAIN 表（NounHash→AttrIndex 映射）   │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 离线路径（pdms-io-fork Rust 实现）

```
┌─────────────────────────────────────────────────────────────┐
│  attlib.dat 文件（E3D 属性库）                               │
│                                                             │
│  Page 1（目录页） → 确定各表起始页号                          │
│  ├── 属性记录区 → AttributeRecord（name, type_code, desc）    │
│  ├── ATNAIN 区 → [NounHash, AttrIndex, TypeCode] 三元组       │
│  ├── ATGTDF 区 → [AttrHash, DataType, DefiKind, ExtIndex]    │
│  ├── ATGTIX 区 → [Code, Page/Offset]                        │
│  └── ATGTSX 区 → [Key, V1, V2]                              │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  AttlibData::parse_attlib_file(path)                         │
│  文件: crates/parse_pdms_db/src/parser/attlib/mod.rs         │
│                                                             │
│  1. 解析属性记录区 → Vec<AttributeRecord>                    │
│  2. 解析 ATNAIN → noun_attr_map: HashMap<u32, Vec<u32>>     │
│  3. 解析 ATGTDF → 属性类型/存储方式                          │
│  4. build_attr_meta_map() → attr_meta_map: HashMap<u32, AttributeMeta> │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  NounSchema::from_attlib(attlib, "ELBO")                     │
│  文件: crates/parse_pdms_db/src/parser/attlib/noun_schema.rs │
│                                                             │
│  db1_hash("ELBO") → noun_hash                               │
│  attlib.get_noun_attributes("ELBO") → Vec<&AttributeMeta>   │
│  输出: NounSchema { noun_name, noun_hash, attributes }       │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. 属性读取时的类型分发

当读取某个元素的属性值时，`DB_Noun::dictionary_` 参与类型分发：

```
L2 层 Handler 分发（sub_5859FC0 @ 0x5859FC0）:

1. actualType = DB_Noun::actualType(element)
     ↓
2. handler = BST_lookup(actualType, attributeId)
     ↓ Handler BST 以 (nounId, attributeId) 为 key
3. handler->vtable[6]->vtable[12](element, qualifier, &value)
```

### 三级类型回退机制

当 Handler BST 在 `actualType` 上未命中时：

```
actualType (精确类型)
  └─ miss → DB_Blob::getContentType(actualType)   // UDET 内容类型
       └─ miss → DB_Noun::hardType(actualType)     // 硬类型（基类型）
            └─ miss → NOUN_UNKNOWN                 // 通配（兜底）
```

### L3 层 internalGetAtt 中的类型解析

```
DB_Element::internalGetAtt(attribute, qualifier, &value)
│
├─ this->noun 为 null → 调用 setType() 从 dictionary_ 重建
├─ this->noun == NOUN_UNKNOWN → 调用 setType()
├─ UDET buildNumber 不匹配 → 调用 setType()
│
├─ isPseudo(contentType)? → 走 PseudoAttPlugger（伪属性，计算得出）
│
├─ isUDA()? → 走 getUda() 路径（用户自定义属性）
│
└─ 主路径: dabGetAtt() → elGotoCPP() → 页面级二进制读取
```

---

## 4. 关键数据类型

### 4.1 AttributeMeta（Rust 侧）

```rust
pub struct AttributeMeta {
    pub hash: u32,              // db1_hash(name)
    pub name: String,           // 如 "XLENGTH"
    pub data_type: AttrDataType,// Integer/Real/Boolean/Text/Position/...
    pub defi: AttrDefiType,     // DAB（物理存储） / Pseudo（伪属性）
    pub size: u32,              // 数据长度（word 数）
    pub unit_type: i32,         // 单位类型码
    pub unit_type_name: String, // Dimensionless/Distance/Temperature/...
    pub description: String,
    pub category: String,
}
```

### 4.2 AttrDataType（数据类型枚举）


| 代码  | 类型          | 说明               |
| --- | ----------- | ---------------- |
| 1   | Integer     | 32 位整数           |
| 2   | Real        | 64 位浮点           |
| 3   | Boolean     | 布尔值              |
| 4   | Reference   | 元素引用（8 字节 RefNo） |
| 5   | Text        | 字符串（4 字节步长编码）    |
| 6   | Enum        | 枚举值              |
| 7   | Position    | 3D 坐标（3×f64）     |
| 8   | Direction   | 3D 方向向量          |
| 9   | Orientation | 3D 方位矩阵          |
| 10  | IntArray    | 整数数组             |
| 11  | RealArray   | 浮点数组             |
| 12  | RefArray    | 引用数组             |


### 4.3 AttrDefiType（存储方式）


| 代码  | 类型     | 说明             |
| --- | ------ | -------------- |
| 1   | DAB    | 物理存储在元素记录的隐式区中 |
| 4   | Pseudo | 伪属性，由计算/回调逻辑生成 |


---

## 5. ATNAIN 表格式

ATNAIN（Attribute-Noun Association INdex）是连接 Noun 与属性的核心映射表，存储在 `attlib.dat` 中。

### 物理格式

```
起始页: 由 attlib.dat Page 1 目录的第 4 个 u32 指定
页大小: 2048 字节（512 个 u32）
记录格式: [NounHash(u32), AttrIndex(u32), TypeCode(u32)] 三元组
终止标记: 0xFFFFFFFF
```

### 解析逻辑

```rust
// crates/parse_pdms_db/src/parser/attlib/mod.rs
fn parse_atnain(file, start_page, data) {
    for page_idx in start_page..start_page + 30 {
        let page_data = read_page(file, page_idx);
        for i in (0..page_data.len() - 2).step_by(3) {
            let noun_hash = page_data[i];     // Noun 的 db1_hash
            let attr_idx  = page_data[i + 1]; // 属性在记录区的索引
            let type_code = page_data[i + 2]; // 关联的类型代码
            
            // 跳过无效数据
            if noun_hash == 0 || noun_hash == 0xFFFFFFFF { continue; }
            
            // 建立映射: noun_hash → [attr_idx, ...]
            data.noun_attr_map.entry(noun_hash).push(attr_idx);
            
            // 保留完整三元组
            data.noun_attr_entries.push(NounAttrEntry {
                noun_hash, attr_index: attr_idx, type_code
            });
        }
    }
}
```

---

## 6. Hash 计算

NOUN 名称和属性名称均通过 `db1_hash` 函数映射为 32 位整数：

```rust
// aios_core::tool::db_tool
let hash = db1_hash("ELBO") as u32;    // → 某个 32 位值
let name = db1_dehash(hash);           // → "ELBO"（可逆）
```

`db1_hash` 是 E3D/PDMS 特有的哈希算法，可通过 `db1_dehash` 反向还原。这是整个属性系统的基石——所有索引、查找、分发都围绕 hash 值展开。

---

## 7. 两条路径的对比


| 维度   | core.dll 运行时路径                                | pdms-io-fork 离线路径                                        |
| ---- | --------------------------------------------- | -------------------------------------------------------- |
| 数据源  | attlib + DB 文件                                | attlib.dat 文件                                            |
| 加载时机 | 数据库打开时 + 按需懒加载                                | 调用 `parse_attlib_file()` 时一次性解析                          |
| 存储结构 | `std::map<int, DB_Noun*>`                     | `HashMap<u32, Vec<u32>>` + `HashMap<u32, AttributeMeta>` |
| 属性查询 | `getSystemAttributes()` → `std::set<int>`     | `get_noun_attributes("ELBO")` → `Vec<&AttributeMeta>`    |
| 类型回退 | actualType → contentType → hardType → UNKNOWN | 无（扁平映射）                                                  |
| 伪属性  | PseudoAttPlugger 动态计算                         | 标记为 `AttrDefiType::Pseudo`，不读取值                          |
| UDA  | `getUda()` 专用路径                               | 从显式属性块中解析                                                |


### 最终结果一致性

两条路径构建的 "NounHash → 属性列表" 映射在数据内容上应当一致，区别仅在于：

- **加载方式**：运行时动态 vs 文件静态解析
- **属性值获取**：DAB/Fortran 层 vs 二进制记录直接解析
- **类型继承**：core.dll 有三级回退，Rust 侧目前是扁平映射

---

## 8. 关键函数地址速查（core.dll）


| 函数                             | 地址                                      | 说明                    |
| ------------------------------ | --------------------------------------- | --------------------- |
| `DB_Noun::getSystemAttributes` | `0x107694AD`                            | 获取 Noun 的系统属性 hash 集合 |
| `db_get_attribute_list`        | `0x1093DD60`                            | 属性列表查询（opcode=60）     |
| `sub_10993900`                 | `0x10993900`                            | 属性列表实际填充              |
| `DB_Attribute::findAttribute`  | `0x1075FB35`                            | 属性字典查找/懒加载            |
| `DB_Attribute::ReadData`       | `0x1075E22A`                            | 属性定义读取                |
| `DB_Element::internalGetAtt`   | `0x107CEA1E` (2.10) / `0x593E9C0` (3.1) | 属性值读取分流               |
| `DB_Element::dabGetAtt`        | `0x107BC6A8` (2.10) / `0x592C670` (3.1) | DAB 层属性读取             |
| `DB_Element::elGotoCPP`        | `0x107C1723` (2.10) / `0x59316D0` (3.1) | 元素游标定位                |


---

## 9. 待深入分析

1. `**DB_Noun` 对象的内存布局**：需要通过 ida-pro-mcp 反编译确认 `DB_Noun` 类的成员变量结构
2. `**dictionary_` 的填充函数**：哪个初始化函数负责创建 `DB_Noun` 对象并插入 `dictionary_`
3. `**db_get_attribute_list` 内部的 ATNAIN 访问**：`sub_10993900` 是直接读 attlib 页面，还是从内存缓存取
4. **类型继承链的构建**：`hardType()` 和 `contentType()` 如何确定，是否有单独的映射表

