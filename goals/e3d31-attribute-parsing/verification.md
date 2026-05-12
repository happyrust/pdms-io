# 验证：E3D 3.1 属性解析 / UDA

## 命令

| Command | Purpose | Expected pass condition | Evidence location |
| --- | --- | --- | --- |
| `python -m json.tool ida_exports/3.1/attribute_types.json` | 验证恢复出的属性类型表 JSON | exit 0；含全部系统类型 type_tag、字段布局 | `progress.jsonl` |
| `python -m json.tool ida_exports/3.1/attribute_names.json` | 验证属性 ID → 符号名映射 | exit 0；合法 JSON；含系统属性全名 | `progress.jsonl` |
| `cargo test -p e3d-attlib --test system_attrs` | 系统属性 100% 解析（含符号名） | fixture 上所有元素的所有非 UDA 属性 oracle 对照一致 | `progress.jsonl` |
| `cargo test -p e3d-attlib --test uda_dictionary` | 属性数据文件内 UDA 区段恢复 | 至少 1 个 fixture 上 UDA 字典完整 + 符号名可读（与系统属性同文件） | `progress.jsonl` |
| `cargo test -p e3d-attlib --test uda_values` | UDA 值 90%+ 解析（含符号名） | 报告解析覆盖率；oracle 对照一致 | `progress.jsonl` |
| `cargo test -p e3d-io --test integration_attrs` | e3d-io → e3d-attlib 端到端 | 从 find_element 一直走到 (id, name, value) 列表 | `progress.jsonl` |

## 人工检查

- 每个系统属性的 type_tag、字节布局、IDA 证据都记录在 `ida_exports/3.1/attribute_types.json`。
- 每个属性 ID 的符号名在 `ida_exports/3.1/attribute_names.json` 中有 IDA 证据来源。
- `docs/ida-3.1-attributes.md` 解释系统属性 vs UDA 的分界、命名空间策略，以及 UDA 区段在外部属性数据文件内的位置（与系统属性同文件，按 `DB_Attribute::isUDA()` 标记区分）。
- 旧 `D:/work/plant-code/e3d-attlib/` 的代码被完全替换；旧 `src/record/attrs.rs` 已删除；归档（如有）放在仓库 attic 或单独标记。
- `e3d-io` ↔ `e3d-attlib` 的依赖方向有 ADR 风格的简短说明。
- 对 5 个 goal 文档跑 Plannotator gate。

## 证据规则

- 每个 type_tag 都必须先在 IDA 中观察到（`DBE_Value` 子类的 vtable / RTTI），再写解码代码。
- UDA 字典如果是外部 lexicon 文件，必须先记录文件格式 + 路径来源。
- 解析覆盖率必须可量化（属性总数 vs 解析成功数 vs 失败原因分布）。
- 任何"先猜后改"的 hack 不得通过验收。
