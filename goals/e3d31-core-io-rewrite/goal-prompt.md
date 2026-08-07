# Codex Goal Prompt：E3D 3.1 Core IO 重写

当本目录下所有关键文档都通过 Plannotator 审批后，粘贴或设置下面的 goal：

```text
/goal 恢复 E3D 3.1 core.dll 数据库结构，并设计只读 Rust IO 核心。

使用 `goals/e3d31-core-io-rewrite/` 作为持久事实来源：
- 阅读 `brief.md`，了解任务、背景、约束、非目标和需要先询问的规则。
- 按 `plan.md` 执行方案、切片、风险和验收标准。
- 运行 `verification.md` 中的检查并记录证据。
- 将具体进度和证明追加到 `progress.jsonl`。
- 遇到 `blockers.md` 中列出的事项，或任何类似高风险未决决策时，暂停并询问用户。

基线和范围：
- 目标版本是 E3D 3.1，使用 active IDA 数据库 `D:\AVEVA\Everything3D3.1\core.dll.i64`。
- 先从 IDA 证据恢复结构；不要从写 Rust 开始。
- 产出 `docs/ida-3.1-structures.md`、`ida_exports/3.1/db_functions.json`、`ida_exports/3.1/struct_layouts.json`。
- 只设计只读 IO 核心：打开数据库、探测 page size、回溯 session、搜索 RefNo 索引、读取 raw element record、暴露结构化 element view。
- 本 goal 不实现写回、claim/release、refresh、compact、multiwrite merge 或实时 core.dll FFI。

只有当每个验收项都有真实证据支撑，并且必要验证已通过，或剩余阻塞已明确记录给用户时，才能标记 goal 完成。
```
