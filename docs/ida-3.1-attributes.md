# E3D 3.1 属性类型与 UDA 字典

> 本文件是 goal `e3d31-attribute-parsing` 的对外文档锚点；机器可读副本在 `ida_exports/3.1/attribute_types.json` 与 `ida_exports/3.1/attribute_names.json`。
>
> 状态：Slice 2 third pass + Slice 3 完成 — `DBE_Value` 家族 + `DB_Attribute` 布局 + EXMAP 派发表 + `expType` 枚举命名 + 系统属性名表（6377 条）全部就绪。

## 修订记录

- **2026-05-12** — Slice 3 完成：
  - 从 `core.dll` 全局符号 `?ATT_<NAME>@@3...` 提取 **6377 条系统属性名表**（`attribute_names.json`，1031 KB，0 collision）。
  - 验证 `db1_hash(NAME)` 与已知样本一致（BORE/HBOR/DESP/POSI/ORIE/TYPE/PTYP/RPRO/CDPR/LDPR/TDPR/PROPRE/REALEV/NAME/OBST 全部 OK）。
  - 6375 条落入 base-27 dehash 范围 `[0x81BF2, 0x171FAD39]`；2 条特例 `CASENAME` / `UNKNOWN` 越界（用途特殊）。
  - 名长直方图：1 字符=6 个、2=6 个、3=18 个、4=1117 个、5=808 个、6=4420 个、7=1（UNKNOWN）、8=1（CASENAME）。
- **2026-05-12** — Slice 2 third pass：
  - 完整恢复 `DBE_Base::expType` 枚举命名（`typeAsString` `0x59b6d40` / `toVarType` `0x59b6c50` / `equivalent*Type` `0x59b585x` 四源交叉验证）。
  - **重大修正**：`DB_Attribute::type` 旧 e3d-attlib 启发式 7/12 项错误（type=4/5 Text↔Reference 互换；type=7/8/9 Direction/Position/Orientation 重新指派；type=10/11/12 不是 Array 变体）。
- **2026-05-12** — Slice 2 second pass：
  - 修正 `DBE_Value +8` 字段语义：先前命名 `type_or_subtype` **错误**；实际是 **KCONDM 编码的单位代码**（uomlib，源自 `DB_Attribute::unit`）。证据：`DB_Element::getAtt`（`0x5933ea0`）`*a5 = KCONDM(&v8)` 写入。
  - 恢复 `DB_Attribute` 关键字段：`hash` (+4) / `type` (+40) / `size` (+44) / `unit` (+48) / `ityp` (+164)。
  - 恢复 EXMAP 派发函数（`sub_51D368F`，MTR 名 `exprlib/EXMAP`）的完整规则表，详见下文 §3。
  - 确认 `DB_Attribute::isUDA()` = `hash > 0x171FAD39`（即 base-27 dehash 范围上界 `387_951_929`）。
  - 恢复 `DBE_Base` 布局（12 字节：vtable + kind + expType）和 `DBE_Attribute` 大小（84 字节）。

---

## 1. 属性数据来源

- E3D 3.1 的属性 schema（含 UDA 定义）来自**外部属性数据文件**，由 `core.dll::sub_55F4290`（`ATTOPE`）通过 `FHFIND(filename, "OLD, READ")` 打开。
- 文件名由调用方（项目配置 / 启动参数）传入，**不**硬编码为 `attlib.dat`（该字符串在 3.1 binary 中 0 xref，属于 PDMS 2.10 遗留死代码）。
- 同一文件内包含多张表：**ATTR**（属性记录）、**ATNAIN**（noun → attr 映射）、**ATGTDF**（属性类型定义）等。
- UDA 与系统属性共享同一文件，在 `DB_Attribute::isUDA()` 之类的运行时方法中区分。

详细 IDA 证据见 `goals/e3d31-attribute-parsing/IDA_VERIFICATION.md`。

---

## 1a. 系统属性名表（Slice 3 产出）

机器可读副本：`ida_exports/3.1/attribute_names.json`（6377 条，1031 KB）。

**提取方法**：扫描 `core.dll` 全局符号匹配 `?ATT_<NAME>@@3...` 的 mangled name，`<NAME>` 即属性名。

**总览**：

| 项 | 值 |
|---|---|
| 总条目 | 6377 |
| 在 base-27 dehash 范围（`[0x81BF2, 0x171FAD39]`） | 6375 |
| 特例（越界） | 2（`CASENAME` 8 字符 / `UNKNOWN` 7 字符） |
| 哈希冲突 | 0 |
| 名长 1 字符 | 6（`D`/`E`/`N`/...） |
| 名长 2 字符 | 6 |
| 名长 3 字符 | 18 |
| 名长 4 字符 | 1117 |
| 名长 5 字符 | 808 |
| 名长 6 字符 | 4420 |
| 名长 7 字符 | 1（`UNKNOWN`） |
| 名长 8 字符 | 1（`CASENAME`） |

**已抽样验证**：`BORE`/`HBOR`/`DESP`/`POSI`/`ORIE`/`TYPE`/`PTYP`/`RPRO`/`CDPR`/`LDPR`/`TDPR`/`PROPRE`/`REALEV`/`NAME`/`OBST` 全部 `db1_hash(NAME)` ≡ 表中 hash。

**JSON 结构**（节选）：
```jsonc
{
  "_meta": { "hash_range": {...}, "total_attributes": 6377, ... },
  "by_name": {
    "PTYP": { "hash": 0xD337E, "symbol_addr": "0x...", "mangled": "?ATT_PTYP@@3..." },
    ...
  },
  "by_hash": { "0x000d337e": "PTYP", ... }
}
```

**对 e3d-attlib 重写的意义**：可以把 `db1_dehash` 的 fallback 路径替换成**优先查表**，仅在表里没有时才走纯 base-27 反算（适用于 UDA hash > 0x171FAD39 的情况）。

---

## 2. DBE_Value 类家族（C++ 层运行时表示）

E3D 3.1 在 `core.dll` 内部用 `DBE_Value` 及其子类作为属性值的运行时容器。下列子类已被 IDA 反编译确认：

### 2.1 `DBE_Value`（基类，IDA `0x59ce1c0`）

值容器，主插槽为 `double`：

| 偏移 | 大小 | 类型 | 名称 | 备注 |
|---|---|---|---|---|
| +0 | 8 | f64 | `value_as_double` | 主插槽。Integer / Boolean / Real 都用这个 |
| +8 | 4 | i32 | **`unit_kcondm`** | **KCONDM 编码的单位代码**（`uomlib/KCONDM`，来自 `DB_Attribute::unit`）。**不是**类型标签（first pass 错误已修正） |
| +12 | 1 | u8 | `has_value_flag` | 默认构造时设为 1 |
| +16 | 4 | i32 | `aux_field` | 默认 0；用途待定（暂未观察到写入点） |
| +20 | 1 | u8 | `aux_flag` | 默认 0；用途待定 |

最小占 24 字节。证据：构造函数 `0x59ce1c0` + 两个 caller (`DB_Element::getAtt` `0x5933ea0`、`getAtt` 包装 `0x5937170`) 交叉验证。

### 2.2 `DBE_RealValue`（IDA `0x59c1ea0`）

`DBE_Value` 的薄壳子类，绑定 `DBE_Base*`。`evaluate` 在 `0x59e0100`，`asString2` 在 `0x59c72f0`。

### 2.3 `DBE_PositionValue`（IDA `0x5842ee0`）

3 个 f64 + DB_Element 链接：

| 偏移 | 大小 | 类型 | 名称 |
|---|---|---|---|
| +0 | 8 | f64 | `x` |
| +8 | 8 | f64 | `y` |
| +16 | 8 | f64 | `z` |
| +28 | — | `DB_Element` | `_owner_link` |

自身 24 字节，offset 24–27 为对齐填充。

### 2.4 `DBE_DirectionValue`（IDA `0x5842c50`）

3 个 f64：

| 偏移 | 大小 | 类型 | 名称 |
|---|---|---|---|
| +0 | 8 | f64 | `x` |
| +8 | 8 | f64 | `y` |
| +16 | 8 | f64 | `z` |
| +24 | — | `DB_Element` | `_owner_link` |

自身 24 字节。

### 2.5 `DBE_OrientationValue`（IDA `0x5842e60`）

9 个 f64（72 字节），可能是 3×3 旋转矩阵或扩展四元数：

| 偏移 | 大小 | 类型 | 名称 |
|---|---|---|---|
| +0 | 72 | f64[9] | `matrix_or_quaternion_3x3` |
| +72 | — | `DB_Element` | `_owner_link` |

确切几何含义待 `evaluate()` 反编译。

### 2.6 `DBE_StringValue`（IDA `0x59b7710`）

布局与 MSVC `std::basic_string<char>` 一致：

| 偏移 | 大小 | 类型 | 名称 |
|---|---|---|---|
| +0 | 16 | u8[16] | `sso_buffer_or_ptr`（SSO，短串内联；长串改存堆指针） |
| +16 | 4 | u32 | `size` |
| +20 | 4 | u32 | `capacity` |
| +24..36 | — | u32×3 + u16 | 辅助字段（待精确化） |

---

## 3. EXMAP — `DB_Attribute → DBE_Base::expType` 派发表

`DBE_Base::FromAttribute(DB_Attribute*)` 是把磁盘上的 attribute 元数据映射到 C++ 运行时类型的入口。它从 `DB_Attribute` 上读取 4 个字段：

| 输入 | 含义 | IDA 访问器 |
| --- | --- | --- |
| `isUDA` | UDA 标记 = `hash > 0x171FAD39` | `0x58d25c0` |
| `type` | EXMAP 主分支变量（1..12） | `0x58d5150` |
| `size` | 标量/数组判别（1 = 标量；其他 = 数组长度） | `0x58d4e90` |
| `ityp` | 内部类型（特殊值 23/29/38/48 覆盖 `type` 派发） | `0x58d2630` |

派发函数是 `sub_51D368F`（IDA `0x51d368f`，MTR trace name `exprlib/EXMAP`）：

```
if (type == 2) {
    expType = (size == 1) ? 2 : 10;           // Real scalar / RealArray
}
else if (ityp ∈ {23, 29, 38, 48}) {
    expType = 3;                              // special override (irrespective of type)
}
else switch (type) {
    case 1:  expType = (size == 1) ? 2  : 13; // Integer (?) — scalar shares expType 2 with Real
    case 3:  expType = (size == 1) ? 1  : 11; // Boolean (?)
    case 4:  expType = 3;                     // 单 expType
    case 5:  expType = (size == 1) ? 4  : 12; // Text / TextArray
    case 6:  expType = (size == 1 || isUDA) ? 3 : 17; // Enum
    case 7:  expType = 7;                     // Position
    case 8:  expType = (size == 1) ? 2  : 6;
    case 9:  expType = 8;                     // Direction
    case 10: expType = 16;                    // Orientation 候选
    case 11: expType = (size == 1) ? 3  : 17;
    case 12: expType = 19;
    default: out_err = 1;
}
```

**观察到的 `expType` 集合**：`{1, 2, 3, 4, 6, 7, 8, 10, 11, 12, 13, 16, 17, 19}`（共 14 个）。

### `expType` 枚举完整命名（2026-05-12 已恢复）

证据：`DBE_Base::typeAsString`（`0x59b6d40`）+ `DBE_Base::toVarType`（`0x59b6c50`）+ `DBE_Base::equivalentArrayType`（`0x59b5850`）+ `DBE_Base::equivalentScalarType`（`0x59b5890`）。

| expType | 符号名 | `typeAsString` | `toVarType` (PML) | 标量等价 |
|---|---|---|---|---|
| 0 | `INVALID`（sentinel） | (unknown) | (empty) | — |
| 1 | `BOOLEAN` | bool | BOOLEAN | 1 |
| 2 | `REAL` | real/int | REAL | 2 |
| 3 | `TEXT` | text | STRING | 3 |
| 4 | `DBREF` | ref | DBREF | 4 |
| 5 | (unused) | — | — | — |
| 6 | `POSITION` | position | POSITION | 2 |
| 7 | `DIRECTION` | direction | DIRECTION | 2 |
| 8 | `ORIENTATION` | orientation | ORIENTATION | 2 |
| 9 | (unused) | — | — | — |
| 10 | `REAL_ARRAY` | real array | ARRAY | 2 |
| 11 | `BOOLEAN_ARRAY` | (unknown) | (empty) | 1 |
| 12 | `DBREF_ARRAY` | ref array | ARRAY | 4 |
| 13 | `INT_ARRAY` | int array | ARRAY | 2 |
| 16 | (unnamed) | (unknown) | (empty) | — |
| 17 | `TEXT_ARRAY` | text array | ARRAY | 3 |
| 18 | (self-pair) | (unknown) | (empty) | 18 |
| 19 | `BLOB` | blob | BLOB | 3 |

**标量↔数组配对**（`equivalentArrayType`）：BOOLEAN(1)↔BOOLEAN_ARRAY(11)、REAL(2)↔REAL_ARRAY(10)、TEXT(3)↔TEXT_ARRAY(17)、DBREF(4)↔DBREF_ARRAY(12)、18↔18（self-pair sentinel）。

**注意**：POSITION/DIRECTION/ORIENTATION 的"标量等价"都是 REAL(2)——证明它们底层都是 f64 向量（3 doubles / 3 doubles / 9 doubles）。INT_ARRAY 的标量等价也是 REAL(2)，因为 Integer 标量与 Real 标量共享同一存储槽。

**`expType = 16` 与 `18`** 用途尚未确定：16 是 EXMAP 在 `type=10` 时唯一的出口，18 是 `equivalentArrayType/ScalarType` 中的自配对哨兵（可能是 "any" / "expression-typed" 通配）。

### `DB_Attribute::type` 语义（2026-05-12 修正）

把 EXMAP 输出再用 §3 上面的 `expType` 命名回填，得到 `DB_Attribute::type` 的**实际**语义。这与 e3d-attlib 的原启发式映射存在**多处冲突**——下表的右栏标注了 e3d-attlib 的旧 guess，可以看出哪些需要纠正：

| `DB_Attribute::type` | 标量 expType | 数组 expType | 实际语义 | e3d-attlib 旧 guess |
|---|---|---|---|---|
| 1 | 2 (REAL) | 13 (INT_ARRAY) | **Integer** | Integer ✓ |
| 2 | 2 (REAL) | 10 (REAL_ARRAY) | **Real** | Real ✓ |
| 3 | 1 (BOOLEAN) | 11 (BOOLEAN_ARRAY) | **Boolean** | Boolean ✓ |
| 4 | 3 (TEXT) | 3 (TEXT) | **Text**（不分 scalar/array） | Reference ✗ |
| 5 | 4 (DBREF) | 12 (DBREF_ARRAY) | **Reference** | Text ✗ |
| 6 | 3 (TEXT) | 17 (TEXT_ARRAY) | text-like（含 UDA 强制 text 路径） | Enum（接近） |
| 7 | 7 (DIRECTION) | 7 (DIRECTION) | **Direction** | Position ✗ |
| 8 | 2 (REAL) | 6 (POSITION) | scalar=real，array=position（用途特殊） | Direction ✗ |
| 9 | 8 (ORIENTATION) | 8 (ORIENTATION) | **Orientation** | Orientation ✓ |
| 10 | 16 | 16 | 未命名（expType=16 语义 TBD） | IntArray ✗ |
| 11 | 3 (TEXT) | 17 (TEXT_ARRAY) | text-like（与 type=6 类似） | RealArray ✗ |
| 12 | 19 (BLOB) | 19 (BLOB) | **Blob** | RefArray ✗ |

**重要冲突**：`type=4` 与 `type=5` 在 e3d-attlib 中被互换了（Reference ↔ Text）；`type=7/8/9` 整体被偏移；`type=10/11/12` 的"Array 变体"假设完全不成立——真实的 Array 变体由 `(type, size>1)` 在 EXMAP 内派生，**不**靠独立的 type code。

→ **对 `e3d-attlib` 重写的影响**：`AttrDataType::from_code` 的 1-12 → 名称表必须从 EXMAP + typeAsString 重新推导，**不**沿用旧 enum。Slice 1 清点记的"算法方向正确"只适用于 hash/parser 层，**不**适用于类型代码标签。

---

## 3a. `DB_Attribute` 布局

| 偏移 | 大小 | 类型 | 名称 | 证据 |
|---|---|---|---|---|
| +0 | 4 | ptr | `vtable` | — |
| +4 | 4 | i32 | `hash` | `isUDA = hash > 0x171FAD39` |
| +8 | 1 | u8 | `lazy_init_flag` | 0 → 下次字段访问触发 `vtable[5]`（`+20`）populator |
| +40 | 4 | i32 | `type` | EXMAP 主分支变量 |
| +44 | 4 | i32 | `size` | scalar=1 / array=N |
| +48 | 4 | i32 | `unit` | KCONDM 单位代码 |
| +164 | 4 | i32 | `ityp` | 内部类型（特殊值表见 §3） |

---

## 3b. `DBE_Base` / `DBE_Attribute` 运行时表示

`DBE_Base` 是表达式树节点基类（12 字节）：

| 偏移 | 大小 | 类型 | 名称 |
|---|---|---|---|
| +0 | 4 | ptr | vtable |
| +4 | 4 | i32 | `kind`（DBE_Attribute = `106`） |
| +8 | 4 | i32 | `expType`（EXMAP 输出） |

`DBE_Attribute`（IDA ctor `0x5866c60`）= **84 字节**，子类化 `DBE_Base`。布局：
- +0..+11：`DBE_Base`
- +12：`DB_Attribute*`（指向元数据）
- +16..+80：多个 i32 / u8 标志位（构造时全部清零，部分由 setter 后填）
- vtable 在构造尾段从 `DBE_AttributeClasses::vftable` 切换到 `DBE_Attribute::vftable`

**与 `DBE_Value` 的关系**：两条并行类层级——`DBE_Base`/`DBE_Attribute` 用于表达式树；`DBE_Value` 用于求值结果的值容器。`getAtt` API 返回 `DBE_Value` 系；`evaluate(... DBE_Value&)` 在 `DBE_Base::evaluate` 系列上分发。

---

## 4. 与 `e3d-attlib` 重写的关系

- 既有 `e3d-attlib/src/parser.rs::AttrDataType::from_code` 的 13 个枚举值与本文 §3 一致；重写时该 enum 设计可保留。
- ATGTDF 表中每条记录的 `(hash, type_code, kind, size)` 结构来自 IDA `sub_55F53B8`，重写时需在每个字段旁添加 `// IDA: sub_55F53B8 v18/v19/v20/v21` 引用。
- `db1_hash` / `db1_dehash` 算法来源于 `PDMS_Hash::String` (`0x588cb87`)，重写时附该 IDA 地址作为证据。
- 硬编码 `PAGE_SIZE = 2048` 需移除，改为从属性数据文件的 descriptor word[0x34] × 4 派生（与 DB 文件一致），或在打开属性文件时通过 file_info 派生。

---

## 5. 已知不确定项

- `DBE_Value` 中 `aux_field` (+16) 与 `aux_flag` (+20) 的用途（构造为 0，暂未观察到写入点）。
- `DBE_OrientationValue` 的 9 doubles 几何含义（旋转矩阵 / 四元数扩展 / 欧拉角扩展）—— `evaluate()` 反编译尚未做。
- `DBE_StringValue` 在 +24 之后的辅助字段。
- 数组类型（INT_ARRAY/REAL_ARRAY/BOOLEAN_ARRAY/DBREF_ARRAY/TEXT_ARRAY）的运行时容器（推测为 `std::vector<T>`，待对应的 `getAtt(... vector<T>&)` 验证）。
- ~~`type_code → DBE_class` 的精确 dispatch 函数地址~~。**已解决**：通过 EXMAP（`sub_51D368F`） + `DBE_Base::FromAttribute` 完成派发。
- ~~`expType` 枚举值的符号名~~。**已解决**：通过 `typeAsString` / `toVarType` / `equivalent*Type` 恢复（详见 §3）。
- `DB_Attribute::ityp` 特殊值 23/29/38/48 的语义（在 EXMAP 中强制路由到 `TEXT`）。
- `expType = 16` 的语义（EXMAP 在 `type=10` 时唯一出口，但 `typeAsString`/`toVarType` 不识别）。
- `expType = 18` 的语义（`equivalent*Type` 中的 self-pair sentinel，可能是 wildcard）。
- `DB_Attribute::type = 8` 的具体业务含义（scalar=REAL，array=POSITION 的混合行为不常见）。
- `DB_Attribute::type = 10/11` 的具体业务含义。

每项的解决都需要进一步反编译 + fixture 字节比对。
