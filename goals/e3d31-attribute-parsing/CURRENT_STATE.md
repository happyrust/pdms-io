# 现状清点（Slice 1）

> 本文件是 goal `e3d31-attribute-parsing` 的 **Slice 1 产物**：在动手重写之前，先把现有 `e3d-attlib` 与 `e3d-io/src/record/attrs.rs` 的能力、命名、覆盖率、隐式假设全部列出。
>
> 清点日期：2026-05-11

---

## 1. 现有 crate `e3d-attlib`（兄弟仓库）

**位置**：`D:/work/plant-code/e3d-attlib/`

**Cargo.toml**：
- name: `e3d-attlib`
- version: `0.1.0`
- edition: 2021
- description：*Standalone parser for E3D/PDMS attlib.dat attribute library files*
- 依赖：`thiserror = "2"` + `nom = "8"`

**源码规模**：3 个文件
- `src/lib.rs`（~5 行）：仅 re-export
- `src/hash.rs`（~55 行）：PDMS base-27 名字哈希
- `src/parser.rs`（~391 行）：核心解析器

**对外 API（来自 `src/lib.rs` re-export）**：
- `hash::db1_hash` / `hash::db1_dehash`
- `parser::AttlibData` / `AttributeRecord` / `NounAttrEntry` / `AttrDataType`

**`AttlibData` 字段**：
- `attributes: Vec<AttributeRecord>` — 属性表
- `name_map: HashMap<String, usize>` — 名字 → 索引
- `noun_attr_map: HashMap<u32, Vec<u32>>` — noun_hash → 属性索引列表
- `noun_attr_entries: Vec<NounAttrEntry>` — 原始 ATNAIN 表
- `attr_defs: Vec<AttrDefEntry>` — 原始 ATGTDF 表
- `attr_def_map: HashMap<u32, AttrDefEntry>` — 属性 hash → 定义

**`AttrDataType` 枚举的 13 个类型码**（hard-coded，缺 IDA 证据）：
| code | type | 备注 |
| --- | --- | --- |
| 1 | Integer | |
| 2 | Real | |
| 3 | Boolean | |
| 4 | Reference | |
| 5 | Text | |
| 6 | Enum | |
| 7 | Position | |
| 8 | Direction | |
| 9 | Orientation | |
| 10 | IntArray | |
| 11 | RealArray | |
| 12 | RefArray | |
| other | Unknown(v) | 兜底 |

**`db1_hash` / `db1_dehash` 算法**：
- base-27（A=1..Z=26），从字符串末尾向前累加 `val = val*27 + char_val`，最后加 `0x81BF1`
- 有效区间：`[0x81BF2, 0x171FAD39]`
- 已知断言：`db1_hash("USER") == 0xD943A`、`db1_hash("BORE") == ?`（roundtrip pass）

**`AttlibData::parse_file` 流程**：
1. 读 page 1（dir page），从中读取：
   - word[1] → ATTR 表起始页号
   - word[3] → ATNAIN 表起始页号
   - 所有非零非 DELIMITER 字作为 candidate 页号
2. 从 ATTR 起始页扫描，按 `DELIMITER = 0xFFFFFFFF` 切分记录
3. 从 ATNAIN 起始页扫描，按 3-tuple `(noun_hash, attr_idx, type_code)` 解析
4. 在 candidate 中启发式找 ATGTDF 起始页（用范围 `[531_442, 387_951_929]` + 3-tuple 模式判断）
5. 构建 `attr_def_map`

**已知问题与隐式假设**：
1. 工作在 **单独的 `attlib.dat` 文件**上，**不是 DB 文件**——与本 goal 的决策"UDA 字典在 DB 文件内"存在概念冲突，需在 Slice 2 之前澄清（见下方 §4）。
2. `PAGE_SIZE = 2048` 硬编码；与本 goal 已确认的"page_size 由 descriptor 决定"不一致。
3. 多个魔术常量（`0x81BF1`、`0x81BF2`、`0x171FAD39`、`531_442`、`387_951_929`）缺 IDA 证据。
4. `parse_atnain` 仅扫描 `start..start+30` 页；`parse_atgtdf` 仅扫描 `start..start+10` 页——边界固定，未基于 fixture 验证。
5. `extract_string` 把非 0x20-0x7E 字节替换为 `.`——是有损解析，可能丢失编码信息。
6. 没有 `#[cfg(test)]` 集成测试（除 `hash.rs` 的 roundtrip）。
7. ATGTDF 的 `kind == 2` 分支有不对称处理（`w1 == 4` vs 其他），缺注释解释来源。

**测试覆盖**：
- 单元测试：`hash.rs` 中 2 个（roundtrip + known_hashes）
- 集成测试：无（外部由 `e3d-io/tests/attlib_scan.rs` 旁路使用）

---

## 2. `e3d-io/src/record/attrs.rs`（69 行，本 goal 拟移除）

**位置**：`D:/work/plant-code/e3d-io/src/record/attrs.rs`

**职责**：**字节级 typed reader**，不涉及 schema：
- `read_int(data, word_offset) -> Option<i32>`
- `read_f64(data, word_offset) -> Option<f64>` —— **Fortran 双精度交换 word 序**：`bytes = [hi, lo]` 反成 `[hi3210, lo3210]`
- `read_f32(data, word_offset) -> Option<f32>`
- `read_refno(data, word_offset) -> Option<RefNo>`
- `read_pos_f64(data, word_offset) -> Option<[f64; 3]>`
- `dump_words(data, max_words) -> Vec<(usize, u32)>`

**评估**：
- 这些读取原语**本身有价值**（含 Fortran 双精度的特殊处理），但与"属性 schema 解析"是两件事。
- 它们更适合留在 `e3d-io` 内（毕竟是 byte-level 读取），或迁移到 `e3d-attlib` 重写后的"低层 reader 模块"。**建议留在 `e3d-io` 内**作为公共字节读取工具；重写后的 `e3d-attlib` 通过 `e3d-io` 暴露的 typed reader 完成 schema 解码。

---

## 3. 配套测试 `e3d-io/tests/attlib_scan.rs`（96 行）

**位置**：`D:/work/plant-code/e3d-io/tests/attlib_scan.rs`

- 读 `D:/work/plant-code/pdms-io-fork/test-file/attlib.dat`（**外部文件**，不是 DB）。
- 字节级扫描前 20 页，找 `ATNAIN` / `ATAINT` 这两个 Fortran 风格的表名串。
- 现状结果未在本仓库 progress 中记录；该测试是探索性的，不依赖任何 `e3d-io::*` 类型（已删除 `e3d_reader` import 后的旁路测试）。

**评估**：探索性测试，建议在 Slice 5（UDA 字典）落地后，要么改为基于 `e3d-attlib` 新 API 的断言型测试，要么归档到 `attic/`。

---

## 4. **关键冲突**：attlib.dat vs DB-embedded UDA dict

本 goal 的 plannotator gate 决策（2026-05-11，blockers.md 第 2 条）明确：

> UDA 字典：**嵌入在数据库文件内**（不是外部 lexicon），需要从 IDA + fixture 中定位字典区段。

但现有 `e3d-attlib` 完全工作在**单独的 `attlib.dat` 文件**上。两种可能：

- **可能 A**：`attlib.dat` 是 **系统属性库**（per-installation 共享 schema），而 **UDA 才是 per-DB 嵌入**。即两者是不同层次，本 goal 应支持两者：系统侧读 `attlib.dat`（或等价的内嵌区段），UDA 侧读 DB。
- **可能 B**：`attlib.dat` 是早期 PDMS 的遗留，E3D 3.1 已经把属性库整体内嵌到 DB；现有 `e3d-attlib` 只是基于 PDMS 2.10 假设的逆向探索，本 goal 应当**完全抛弃** `attlib.dat` 路径，从 DB 内读取。

**需要在 Slice 2（系统属性类型表恢复）之前先用 IDA 确认**：
- E3D 3.1 的 `db_*` API 中是否仍引用 `attlib.dat` 路径？
- `core.dll` 启动时是否读取外部 attlib 文件？
- DB descriptor / file_info 中是否有指向"内嵌属性库区段"的字段？

→ 把该问题作为新 blocker 追加（已在本文件 §6 列出）。

---

## 5. 与新 goal 计划的对应关系

| Slice | 输入 | 现有可复用 | 需新增 |
| --- | --- | --- | --- |
| 1 现状清点 | — | 本文件 | — |
| 2 系统属性类型表恢复 | IDA `DBE_Value` 子类 | 13 个类型枚举的代码骨架可参考 | type_tag → 字节布局映射、字段布局 |
| 3 属性名表恢复 | `core.dll` 内属性名表 | `db1_hash` / `db1_dehash` | ID → 名表（attlib 的 ATTR 表可能就是答案，需 IDA 确认） |
| 4 record → (id, name, value) 解析 | `e3d-io::ElementRecordView` | `record/attrs.rs` 的 typed reader | schema → 偏移映射 |
| 5 UDA 字典恢复（DB 内） | DB 文件本身 | 现有 attlib 解析逻辑作为 hint，但需重写 | 内嵌位置 + 字段语义 |
| 6 UDA 解析 + 名表合成 | 解析后的 UDA 字典 | `db1_hash` / `db1_dehash` | UDA-specific 类型扩展、命名空间合成 |
| 7 重写 `e3d-attlib` 并集成 | 全部前置切片 | — | crate 重组、`e3d-io` 依赖切换 |

---

## 6. Slice 1 产生的新阻塞 / 后续工作

- 新阻塞：**确认 `attlib.dat` 与 "DB 内嵌 UDA 字典" 的关系**（A or B）。在 Slice 2 之前必须解决。
- 新阻塞：所有现有魔术常量（`0x81BF1`、`0x171FAD39`、`531_442`、`387_951_929`、`PAGE_SIZE=2048`）需要 IDA 证据回填。
- 新待办：评估 `e3d-io/src/record/attrs.rs` 的去留位置（建议留在 `e3d-io` 内）。
- 新待办：评估 `e3d-io/tests/attlib_scan.rs` 的归档方式。

清点结束。Slice 2 开工前需要把上述阻塞回到用户做决策。
