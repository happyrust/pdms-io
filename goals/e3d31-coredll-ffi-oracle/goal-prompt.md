# Codex Goal Prompt：E3D 3.1 core.dll FFI Oracle

当本目录下所有关键文档都通过 Plannotator 审批后，粘贴或设置下面的 goal：

```text
/goal 为 E3D 3.1 构建一个 core.dll FFI oracle，用于校验 Rust 自实现的 e3d-reader / e3d31-writeback。

使用 `goals/e3d31-coredll-ffi-oracle/` 作为持久事实来源：
- 阅读 `brief.md`，了解任务、背景、约束、非目标与"需要先询问"。
- 按 `plan.md` 的 6 个 slice 推进，先恢复 FFI 表面，再做子进程 harness、协议、用例与对照。
- 跑 `verification.md` 中的命令与人工检查并记录证据。
- 把进度与差异 diff 追加到 `progress.jsonl`。
- 触及 `blockers.md` 中列出的"停下并询问"事项时立即暂停。

基线和范围：
- 基线 core.dll：`D:/AVEVA/Everything3D3.1/core.dll`（IDA: `D:/AVEVA/Everything3D3.1/core.dll.i64`）。
- 子进程方案默认优先；如选 in-process，必须先用 brief / plan 修订记录理由。
- 仅作为测试基础设施，不进入 e3d-reader 生产路径，不进入任何最终产物的依赖图。
- 永远不修改原始 fixture；任何写测试都在副本上跑。

只有当 5 类只读操作与 1 类写回操作均通过 oracle 对照，并且异常路径与 fixture 隔离策略经过审阅时，才能标记 goal 完成。
```
