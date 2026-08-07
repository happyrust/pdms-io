# E3D 3.1 Multi-Extent & Extract 语义（草案）

> **状态**：DRAFT — 尚未经过 Plannotator gate。

## 目标结果

恢复并实现 E3D 3.1 数据库的 **multi-extent**（一个逻辑库由多个磁盘 extent 组成）与 **Extract**（同一项目下的工作副本 / 派生层）语义。在 `e3d-reader`（只读）与 `e3d31-writeback`（写入）的基础上，让 Rust 能正确处理多 extent 的页面寻址、跨 extent 引用、extract 层叠加的可见性规则。

## 背景

- 前期只读 goal 显式把 multi-extent 列为非目标。
- IDA 已观察到 `FLPAGE` / `FLDELE` 文件层函数与 `db_open_*` 中的 `max_ext` / `ext_size` 字段；session page / descriptor 中含 `ext_no` 字段（置信度中）。
- "Extract" 是 PDMS / E3D 的典型协作模型，工作副本与主库通过覆盖层进行合并；具体二进制布局尚未恢复。

## 约束

- 仍以 IDA 证据驱动；任何"假设主库 + 增量"的猜测必须先在 IDA / fixture 中确认。
- 必须能与单 extent 模式向后兼容，不破坏现有只读路径。
- 字节序、page_size 等基线常量沿用 3.1。

## 非目标

- 写回 multi-extent 上的复杂操作（先证明只读可见性，再考虑写）。
- 完整 UDA 在 extract 间的差异化（→ `e3d31-attribute-parsing`）。
- 与新版本 E3D（3.2+）的兼容。

## 需要先询问

- 是否优先取得包含 multi-extent 的 fixture（目前 `ams1112_0001` 是否就是多 extent？）。
- Extract 与 multi-extent 是否要在同一 goal 内处理；若工作量过大，应拆为两个 goal。

## 完成定义

- `e3d-reader` 能正确遍历 multi-extent 数据库的所有页面，按 RefNo 找到任一元素，跨 extent 引用解析正确。
- Extract 层的可见性规则有文档与至少 1 个 fixture 验证用例。
- 已恢复字段（`ext_no`、`ext_size`、`max_ext`、文件链等）的置信度提升到"高"或显式记录"无法验证"。
