# 验证：E3D 3.1 写回支持

## 命令

| Command | Purpose | Expected pass condition | Evidence location |
| --- | --- | --- | --- |
| `python -m json.tool ida_exports/3.1/db_write_functions.json` | 验证写入侧函数映射 JSON | exit 0；合法 JSON；含 save_work / claim / release / FHDBWN / FHSPLT 等 | `progress.jsonl` |
| `cargo test -p e3d-io --test inplace_update` | 不触发 B+tree 变更的 attribute 修改往返 | oracle 对照 0 差异 | `progress.jsonl` |
| `cargo test -p e3d-io --test create_delete` | 元素 create + delete 往返 | oracle 对照 0 差异；fixture 副本完整性校验 pass | `progress.jsonl` |
| `cargo test -p e3d-io --test btree_split` | 至少 1 个用例触发 B+tree split | oracle 对照 0 差异；split 前后索引一致性手动可验证 | `progress.jsonl` |
| `cargo test -p e3d-io --test interruption_recovery` | 模拟中断后的恢复策略行为 | 按 brief 中"rollback / fail-fast"决策项执行 | `progress.jsonl` |

## 人工检查

- 写入侧每个函数 / 字段均有 IDA 证据与置信度，未恢复字段不可被赋予具体语义。
- 写流程中所有跨页 / 跨 extent / 跨 session 边界都被显式标注，并被测试覆盖。
- `core.dll` 通过 `e3d31-coredll-ffi-oracle` 加载 Rust 写出的库无异常报错。
- B+tree split / merge 实现的边界条件（空树、只有 root、临界容量）有显式测试。
- 失败语义被记录在 brief 中（rollback 还是 fail-fast），并与实现一致。
- 对 5 个 goal 文档（brief / plan / verification / blockers / goal-prompt）均跑 Plannotator gate。

## 证据规则

- 任何不通过 oracle 验证的写入用例**不得**视为完成。
- 对于 IDA 中尚未恢复的字段，写回路径必须显式拒绝触碰，而非"按默认值写"。
- 失败回滚后的 fixture 必须能被 oracle 重新打开；不允许留下半提交状态。
