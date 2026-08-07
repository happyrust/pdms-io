# Codex Goal Prompt：E3D 3.1 写回支持

当本目录下所有关键文档都通过 Plannotator 审批后，粘贴或设置下面的 goal：

```text
/goal 在 e3d-reader 只读基础上为 E3D 3.1 实现兼容 core.dll 的写回路径。

使用 `goals/e3d31-writeback/` 作为持久事实来源：
- 阅读 `brief.md`：目标、约束、非目标、依赖、需要先询问。
- 按 `plan.md` 的 7 个 slice 推进，先做 IDA 写入侧函数家族恢复，再分别打通 page write / claim+release / in-place update / create+delete / save_work / round-trip。
- 跑 `verification.md` 中的命令与人工检查，所有写入用例必须通过 `e3d31-coredll-ffi-oracle` 对照。
- 把进度与 oracle diff 追加到 `progress.jsonl`。
- 触及 `blockers.md` 中的"停下并询问"事项时立即暂停。

基线和范围：
- 基线版本 E3D 3.1；字节序 BE；page_size 单位 = words × 4。
- 所有写入路径偏移 / 常量 / 调用图必须追溯到 `ida_exports/3.1/db_write_functions.json`（本 goal 的 Slice 1 产物）。
- 显式非目标：multi-extent 写入、UDA 完整 schema、性能优化。
- 任何 fixture 写入操作只能在副本上执行；oracle goal 未就绪前不得标记 round-trip 验收完成。

只有当 5 类写操作均通过 oracle 对照、至少 1 个 B+tree split 用例通过验证、且中断恢复策略经过审阅时，才能标记 goal 完成。
```
