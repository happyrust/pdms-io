# 计划：E3D 3.1 属性解析 / UDA（草案）

> **状态**：DRAFT — 尚未经过 Plannotator gate。

## 方案概览

分两层推进：先把"系统属性"的解析路径建稳——即在 `core.dll` 里有显式 `DBE_Value` 子类支撑、值类型已知的属性；再处理 **属性数据文件内的 UDA 区段**（与系统属性字典共享同一外部文件，2026-05-11 IDA 修订）。整个 goal 的产物落到**重写后的 `e3d-attlib` crate**（位于 `D:/work/plant-code/e3d-attlib/`），完整替换旧实现；同时还原 `core.dll` 内的属性符号名表（基于 `PDMS_Hash::String` `0x588cb87` 的 base-27 dehash），对外提供 `attribute_id <-> name` 双向 API，供 `e3d-io`（原 `e3d-reader`）与下游工具消费。

## 工作切片

| Slice | Purpose | Done when | Risks |
| --- | --- | --- | --- |
| 1 | 现状清点 + 旧 `e3d-attlib` 归档 | 现有 `e3d-attlib` 与 `e3d-reader/src/record/attrs.rs` 能力 / 命名 / 测试覆盖被完整列出；旧代码标记为待重写，迁移路径明确 | 现有代码可能有未文档化的隐式假设 |
| 2 | 系统属性类型表恢复 | 从 IDA 找出 `DBE_Value` 类层级的所有具体子类、type tag、字节布局，写入 `ida_exports/3.1/attribute_types.json` + `docs/ida-3.1-attributes.md` | 类层级深，可能涉及 C++ vtable 重建 |
| 3 | 属性名表恢复 | 从 `core.dll` 找到系统属性 ID → 符号名映射，写入 `ida_exports/3.1/attribute_names.json` | 名表可能分散在多处 / 经过 hash |
| 4 | record → (id, name, value) 解析 | Rust 能在 `ElementRecordView::raw_bytes` 上跑出 attribute map，含可读符号名 | record segment 跨页机制尚未完全确定 |
| 5 | UDA 字典恢复（属性数据文件内） | 在外部属性数据文件内定位 UDA 区段（与系统属性表共享文件，但 UDA 用 `DB_Attribute::isUDA()` 标记区分），恢复字段语义与符号名 | UDA 字典版本管理 |
| 6 | UDA 解析 + 名表合成 | Rust 在 fixture 上解出全部 UDA，并通过 `id <-> name` API 暴露 | UDA 命名冲突策略 |
| 7 | 重写 `e3d-attlib` 并集成 | 旧 `e3d-attlib` 被新实现完全替换；`e3d-io` 引用 `e3d-attlib`；旧 `attrs.rs` 删除 | 跨仓库依赖切换需要谨慎 |

## 执行顺序

- 1 必须先做（避免重复工作）。
- 2 → 3 → 4 串行（先类型表，再名表，再 record 解析）。
- 5 → 6 串行；可在 4 后开始。
- 7 是收口（重写 + 集成）。

## 验收标准

- [ ] `attribute_types.json` + `attribute_names.json` + `docs/ida-3.1-attributes.md` 存在，每个类型 / 名字都有 IDA 证据。
- [ ] fixture 上系统属性 100% 解析（含可读符号名）；UDA 90%+ 解析（含可读符号名）。
- [ ] 旧 `e3d-attlib` 完整重写；旧 `src/record/attrs.rs` 已删除；`e3d-io` 通过 `e3d-attlib` 暴露 attribute map。
- [ ] 至少 1 个端到端测试，从 `e3d-io::find_element` 一直走到 `(id, name, value)` 元组列表。

## 方向控制

- 属性 schema 演进会牵动很多上层工具，**改动前必须验证 oracle**。
- 不允许"先猜类型再 hack 字节序"：每个 type tag 必须先在 IDA 看到，再写代码。
- UDA 与系统属性的边界必须清晰，避免 ID 空间冲突。
