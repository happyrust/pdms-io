# Codex Goal Prompt：E3D 3.1 属性解析 / UDA

当本目录下所有关键文档都通过 Plannotator 审批后，粘贴或设置下面的 goal：

```text
/goal 为 E3D 3.1 在 e3d-reader 之上实现完整的属性 schema 解析（系统属性 + UDA）。

使用 `goals/e3d31-attribute-parsing/` 作为持久事实来源：
- 阅读 `brief.md`：本 goal 合并了"attrs.rs 越界拆出"与"完整 UDA 解析"两件事。
- 按 `plan.md` 的 6 个 slice 推进：先清点现有 attlib + attrs.rs，再恢复系统类型表，再做 record → 属性 map，最后处理 UDA。
- 跑 `verification.md` 中的命令；解析覆盖率必须可量化，oracle 对照一致。
- 把进度与覆盖率追加到 `progress.jsonl`。
- 触及 `blockers.md` 中的"停下并询问"事项时立即暂停。

基线和范围：
- 基线版本 E3D 3.1；字节序 BE；page_size 单位 = words × 4。
- 类型恢复必须 IDA 证据驱动；任何 type_tag 必须先观察到再解码。
- 已决策（2026-05-11 plannotator gate + Slice 1.5 IDA 修订）：
  - 旧 `e3d-attlib` **重写**（算法骨架保留，所有偏移 / 常量补 IDA 引用）。
  - UDA 字典**嵌入在外部属性数据文件内**，与系统属性字典共享同一文件，文件名由 `ATTOPE` 调用方传入（字符串 `attlib.dat` 0 xref，是 PDMS 遗留死代码）。
  - `attribute_id` 要还原为**可读符号名**，基于 IDA `PDMS_Hash::String` (`0x588cb87`) 的 base-27 dehash，对外提供 `id <-> name` 双向 API。
  - 本 goal 的代码产物**全部落到重写后的 `e3d-attlib` crate**，由 `e3d-io` 引用消费。
  - 6 个魔术常量（`0x81BF1`、`0x81BF2`、`0x171FAD39`、`531442`、`387951929`、`PAGE_SIZE`）的 IDA 证据已在 `IDA_VERIFICATION.md` 集齐。
- 显式非目标：属性写入（→ `e3d31-writeback`）、表达式 / rule 求值、UI / 浏览器、属性级二进制兼容。

只有当系统属性 100% 解析（含可读符号名）、UDA 90%+ 解析（含符号名）、`e3d-attlib` 完整重写并集成、且所有 oracle 对照通过时，才能标记 goal 完成。
```
