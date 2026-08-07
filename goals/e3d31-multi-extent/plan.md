# 计划：E3D 3.1 Multi-Extent & Extract（草案）

> **状态**：DRAFT — 尚未经过 Plannotator gate。

## 方案概览

先把 multi-extent 物理层打通：在 IDA 中明确 extent 链与每个 extent 的 header/索引关系，恢复 `descriptor.ext_size` / `descriptor.max_ext` 等字段的精确语义，再让 `e3d-reader` 的 page I/O 层支持把 (extent_no, page_no) 映射到具体文件。在物理层之上叠加 Extract 的语义层：识别 base / extract 的关系链，按可见性规则合并查询。

## 工作切片

| Slice | Purpose | Done when | Risks |
| --- | --- | --- | --- |
| 1 | 取得 multi-extent fixture | 至少 1 个真正多 extent 的样本 + 已知 RefNo 列表 | 现有 fixture 可能是单 extent |
| 2 | 恢复 extent 物理布局 | `struct_layouts.json` 新增 `extent_link`、`file_chain` 字段（高置信度），`ida-3.1-structures.md` 加 §multi-extent 章节 | 字段可能依赖运行时全局状态 |
| 3 | 物理层 multi-extent 寻址 | `e3d-reader::page` 支持 `(extent_no, page_no)` 寻址；现有单 extent 测试不变绿 | 跨 extent 引用解析 |
| 4 | Extract 语义恢复 | 至少识别出 extract 链头 / 父 extract / 可见性优先级；写入 docs | Extract 可能涉及更高层 PDMS 语义 |
| 5 | Extract 可见性合成 | `e3d-reader::engine` 增加"with_extract"读取模式，按 extract 链合成最终 view | 性能：合成可能很贵 |
| 6 | 验证 | oracle 对比：oracle 在 extract 上读到什么，Rust 也读到什么 | oracle goal 必须先就绪 |

## 执行顺序

- 1 是阻塞性的，无样本则不能 ground-truth。
- 2-3 串行（结构 → 实现）。
- 4-5 串行；可在 3 完成后开始。
- 6 全程串联。

## 验收标准

- [ ] multi-extent fixture 至少 1 份，含说明文件。
- [ ] 物理层支持 (extent_no, page_no) 寻址，单 extent 路径无回归。
- [ ] Extract 可见性规则有文档与单元测试。
- [ ] oracle 对比通过。

## 方向控制

- 不允许在没有 fixture 的情况下"先按 2.10 文档假设"。
- 性能优化延后。
- 任何"猜测可见性规则"的提交必须显式标注待 oracle 验证。
