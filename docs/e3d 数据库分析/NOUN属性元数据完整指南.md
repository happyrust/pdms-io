# E3D/PDMS NOUN 属性元数据完整指南

> 基于 IDA Pro + x64dbg 逆向分析 + pdms-io-fork 源码
> 分析日期：2026-04-15
> 架构图：[noun_attr_metadata_architecture.svg](./noun_attr_metadata_architecture.svg)

## 1. 总览

E3D/PDMS 的每个数据库元素（Element）都隶属于一个**类型（NOUN）**，例如 PIPE、ELBO、TEE、VALV 等。类型系统定义了元素拥有哪些属性、每个属性的数据类型、在元素记录二进制中的物理偏移以及默认值。

本文档完整描述：
- NOUN 属性元数据的数据格式（`all_attr_info.json`）
- 元数据从"磁盘文件"到"运行时可用"的两条链路
- 元素记录二进制中属性值的物理定位方法
- 完整的解析流程和工具链

---

## 2. 数据格式：`all_attr_info.json`

### 2.1 文件结构

```json
{
  "noun_attr_info_map": {
    "<noun_hash>": {
      "<attr_hash>": {
        "name": "<ATTR_NAME>",
        "hash": <attr_hash_int>,
        "offset": <physical_offset>,
        "default_val": { "<TypeTag>": <default_value> },
        "att_type": "<TYPE_NAME>"
      }
    }
  },
  "named_attr_info_map": {
    "<NOUN_NAME>": { ... }   // 与上面相同，改用名称索引
  }
}
```

### 2.2 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `name` | string | 属性名称（大写），如 `BORE`、`TEMP`、`POS` |
| `hash` | u32 | `db1_hash(name)` 计算结果 |
| `offset` | u32 | 属性在隐式区中的物理偏移（复合编码，见 §2.3） |
| `default_val` | object | 类型化默认值，key 为类型标签 |
| `att_type` | string | 数据类型名称 |

### 2.3 offset 复合编码

offset 字段编码属性在元素记录**隐式区**中的位置：

**规则 1：offset = 0** → **Pseudo 属性**，不直接存储在隐式区中。其值由 `PseudoAttPlugger` 动态计算，或通过显式块/成员块获取（如 NAME、DESC、TYPE、OWNER）。

**规则 2：0 < offset < 0x100000** → **直接 word 偏移**

```
byte_position = offset × 4  (相对于隐式区起始)
```

**规则 3：offset ≥ 0x100000** → **BOOL 位打包编码**

```
bit_index  = offset >> 20       (在 word 中的位索引，1-based)
word_offset = offset & 0xFFFFF  (word 在隐式区中的偏移)

读取方式：
  word_value = read_u32(implicit_area + word_offset * 4)
  bool_value = (word_value >> (bit_index - 1)) & 1  // 或直接 bit_index 对应位
```

**示例（PIPE word[12] 中 3 个 BOOL）**：

| 属性 | raw offset | bit_index | word_offset | 读取方式 |
|------|-----------|-----------|-------------|---------|
| BUIL | 12 (0x0C) | 0 (直接) | 12 | word[12] 整体，但只用 bit0 |
| SHOP | 1048588 (0x10000C) | 1 | 12 | word[12] bit 1 |
| LISS | 2097164 (0x20000C) | 2 | 12 | word[12] bit 2 |

### 2.4 属性类型与存储大小

| att_type | TypeTag | 存储大小 (words) | 字节 | 说明 |
|----------|---------|-----------------|------|------|
| INTEGER | IntegerType | 1 | 4 | 32 位整数 |
| DOUBLE | DoubleType | 2 | 8 | 64 位浮点 |
| BOOL | BoolType | 位打包 | — | 多个 BOOL 共享一个 word |
| WORD | WordType | 1 | 4 | 枚举值（字符串编码为整数） |
| STRING | StringType | 0 (Pseudo) | — | 通常为 Pseudo 属性 |
| ELEMENT | ElementType | 2 | 8 | 元素引用（8 字节 RefNo） |
| POSITION | Vec3Type | 6 | 24 | 3×f64 坐标 |
| ORIENTATION | Vec3Type | 9 | 36 | 方位矩阵（3×3 或压缩） |
| DIRECTION | Vec3Type | 3 | 12 | 3D 方向向量 |
| INTVEC | IntArrayType | 变长 | — | 整数数组 |
| RefU64Vec | — | 变长 | — | 引用数组 |

### 2.5 default_val 类型标签

```json
{ "IntegerType": 0 }
{ "DoubleType": 0.0 }
{ "BoolType": false }
{ "WordType": "unset" }
{ "StringType": "" }
{ "ElementType": "" }
{ "Vec3Type": [0.0, 0.0, 0.0] }
{ "IntArrayType": [] }
```

---

## 3. 两条获取链路

### 3.1 运行时链路（core.dll 原始实现）

```
数据库初始化
  ↓ 创建 DB_Noun 对象
DB_Noun::dictionary_  [static map<int, DB_Noun*>]
  │ 地址: 0x5ADD359C
  │ 大小: 1931 entries (MSVC RB-tree)
  │
  ├─ DB_Noun::getSystemAttributes()  @ 0x107694AD
  │    └─ db_get_attribute_list()    @ 0x1093DD60
  │         └─ opcode=60 → sub_10993900 @ 0x10993900
  │              (从 attlib 内存缓存读取 ATNAIN)
  │
  └─ DB_Attribute::dictionary_  [static map<int, DB_Attribute*>]
       └─ findAttribute(hash) → 懒加载 → ReadData()
            (读取 type, dtyp, size, defi, offset 等)
```

**三级类型回退**（Handler BST 分发时）：

```
actualType → (miss)
  └─ DB_Blob::getContentType() → (miss)
       └─ DB_Noun::hardType() → (miss)
            └─ NOUN_UNKNOWN (通配)
```

### 3.2 离线链路（pdms-io-fork Rust 实现）

```
attlib.dat 文件
  ↓ AttlibData::parse_attlib_file()
  ├─ 属性记录区 → Vec<AttributeRecord>
  ├─ ATNAIN     → noun_attr_map: HashMap<u32, Vec<u32>>
  ├─ ATGTDF     → 属性类型/存储方式
  └─ build_attr_meta_map() → attr_meta_map: HashMap<u32, AttributeMeta>
       ↓
  NounSchema::from_attlib("ELBO") → 完整属性列表
```

**两者差异**：

| 维度 | 运行时 | 离线 |
|------|--------|------|
| NOUN 覆盖 | 1931 个 | 取决于 attlib 版本 |
| **物理 offset** | **有**（all_attr_info.json 339个） | **无**（仅 hash 映射） |
| 默认值 | 有 | 无 |
| 类型回退 | 三级回退 | 扁平映射 |

---

## 4. 元素记录二进制布局

### 4.1 完整记录结构

```
┌─────────────────────────────────────────┐
│ 对齐填充（可选）                          │ 0x00000007 / 0x00000000
├─────────────────────────────────────────┤
│ 隐式区头部（固定 11 words = 44 bytes）    │
│   [0]    impl_len_words (i32, 大端)      │ 隐式区总长度
│   [1..2] refno (8B)                      │ 元素参考号
│   [3]    noun_hash (4B)                  │ 类型 hash
│   [4..5] owner (8B)                      │ 父元素 refno
│   [6..10] (其他头部字段)                  │
├─────────────────────────────────────────┤
│ 隐式区 payload                           │
│   按 all_attr_info.json 中的 offset 定位  │
│   每个属性按 att_type 决定的大小读取       │
├─────────────────────────────────────────┤
│ Members 块（可选，flag=0x0002）           │
│   子元素列表（children refno array）      │
├─────────────────────────────────────────┤
│ 显式属性块（可选，0..N 个，flag=0x0001）   │
│   [attr_hash(4B)] [type_code(2B)]        │
│   [data_len_words(2B)] [data...]         │
├─────────────────────────────────────────┤
│ UDA 块（可选）                            │
├─────────────────────────────────────────┤
│ 结束标记 0x00000000 + 0x00000007          │
└─────────────────────────────────────────┘
```

### 4.2 PIPE 隐式区 DAB 属性布局

```
word[0]      impl_len_words
word[1..2]   refno (8B)
word[3]      noun_hash = 0x000463E9 (PIPE)
word[4..5]   owner (8B)
word[6..10]  (header fields)
─── DAB payload 起始 ───
word[11]     PURP      WORD       — 用途枚举
word[12]     BUIL(b0) SHOP(b1) LISS(b2)  — 3 个 BOOL 位打包
word[13..14] BORE      DOUBLE     — 口径 (mm)
word[15..16] TEMP      DOUBLE     — 温度 (°C)
word[17..18] PRES      DOUBLE     — 压力 (kPa)
word[19..20] PSPE      ELEMENT    — 管件规格引用
word[21..22] ISPE      ELEMENT    — 绝缘规格引用
word[23..24] TSPE      ELEMENT    — 保温规格引用
word[25..26] MATR      ELEMENT    — 材料引用
word[27..28] FLUR      ELEMENT    — 流体引用
word[29..30] CASR      ELEMENT    — 外壳引用
word[31]     CCEN      INTEGER    — 组件中心编号
word[32]     CCLA      INTEGER    — 组件类编号
word[33]     LNTP      WORD       — 管线类型枚举
word[34]     EREC      INTEGER    — 安装区域
word[35]     REV       INTEGER    — 修订号
word[36]     SAFC      INTEGER    — 安全系数
word[37]     WMAX      INTEGER    — 最大焊缝数
word[38]     PMAX      INTEGER    — 最大管件数
word[39]     SMAX      INTEGER    — 最大支架数
word[40]     JMAX      INTEGER    — 最大接头数
word[41..42] DRRF      ELEMENT    — 图纸引用
```

---

## 5. db1_hash 算法

```python
def db1_hash(name: str) -> int:
    """E3D/PDMS 标准名称哈希函数"""
    h = 0
    for ch in name.upper():
        h = (h * 26 + ord(ch) - 0x40) & 0xFFFFFFFF
    return h

def db1_dehash(h: int) -> str:
    """逆向还原名称（可逆哈希）"""
    chars = []
    while h > 0:
        r = h % 26
        h = h // 26
        if r == 0:
            r = 26
            h -= 1
        chars.append(chr(r + 0x40))
    return ''.join(reversed(chars))
```

---

## 6. 统计数据

| 指标 | 值 |
|------|-----|
| NOUN 常量总数（IDA 导出） | 1932 |
| dictionary_ 运行时条目 | 1931 |
| all_attr_info.json 覆盖 NOUN | 339 |
| 总属性数 | 6555 |
| DAB 属性（有 offset） | ~2400 |
| Pseudo 属性（offset=0） | ~4155 |
| dictionary_ 运行时地址 | 0x5ADD359C |
| sentinel 节点 | 0x05D06410 |

---

## 7. 关键函数地址（core.dll）

| 函数 | 2.10 地址 | 3.1 地址 | 说明 |
|------|----------|---------|------|
| DB_Noun::getSystemAttributes | 0x107694AD | — | 获取属性 hash 集合 |
| db_get_attribute_list | 0x1093DD60 | — | opcode=60 调度 |
| sub_10993900 | 0x10993900 | — | 属性列表填充 |
| DB_Attribute::findAttribute | 0x1075FB35 | — | 属性字典查找 |
| DB_Element::internalGetAtt | 0x107CEA1E | 0x593E9C0 | 属性值读取分流 |
| DB_Element::dabGetAtt | 0x107BC6A8 | 0x592C670 | DAB 层读取 |

---

## 8. 输出文件清单

| 文件 | 内容 |
|------|------|
| `all_noun_types.json` | 1932 个 NOUN 名称列表 |
| `noun_hash_table.json` | NOUN 名称 + hash + IDA 全局地址 |
| `noun_dictionary_dump.bin` | 二进制格式的 NOUN hash 表 |
| `noun_attr_metadata_architecture.svg` | 架构图 |
| `DB_Noun属性元数据获取机制.md` | dictionary_ 详细分析 |
| `DB_Noun_dictionary运行时数据解析.md` | 运行时数据 dump 方法 |
