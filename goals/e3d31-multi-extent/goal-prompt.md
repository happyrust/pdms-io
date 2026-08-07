# Codex Goal Prompt：E3D 3.1 Multi-Extent & Extract

当本目录下所有关键文档都通过 Plannotator 审批后，粘贴或设置下面的 goal：

```text
/goal 为 E3D 3.1 在 e3d-reader 之上支持 multi-extent 寻址与 Extract 可见性合成。

使用 `goals/e3d31-multi-extent/` 作为持久事实来源：
- 阅读 `brief.md`，确认 multi-extent 与 Extract 的范围与拆分边界。
- 按 `plan.md` 的 6 个 slice 推进：先取得真正多 extent fixture，再恢复物理 layout，再补 Rust 寻址，最后做 Extract 可见性。
- 跑 `verification.md` 中的命令；所有可见性规则必须有 oracle 对照（依赖 `e3d31-coredll-ffi-oracle`）。
- 把进度追加到 `progress.jsonl`。
- 触及 `blockers.md` 中的"停下并询问"事项时立即暂停。

基线和范围：
- 基线版本 E3D 3.1。
- 物理层先于语义层；先证明 (extent_no, page_no) 寻址正确，再做 Extract 合成。
- 显式非目标：multi-extent 写回（→ `e3d31-writeback` 后续扩展）、UDA 跨 extract 差异化（→ `e3d31-attribute-parsing`）。
- 不允许单 extent 路径出现回归。

只有当物理层、语义层、oracle 对照三者都通过验收，且不确定字段全部消除或显式记录后，才能标记 goal 完成。
```
