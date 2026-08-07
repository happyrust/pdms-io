# 计划：E3D 3.1 写回支持（草案）

> **状态**：DRAFT — 尚未经过 Plannotator gate。

## 方案概览

在已稳定的只读 IO 之上，分阶段构建写回路径。先把现有 `e3d-reader` crate **重命名为 `e3d-io`**（已决策，2026-05-11 plannotator gate），再完成 IDA 写入侧函数映射（与只读时的恢复方法对称），随后分别打通 page write + session journal + B+tree split + header commit 四条主线。最后用 `core.dll` oracle 做端到端 round-trip 验证。

## 工作切片

| Slice | Purpose | Done when | Risks |
| --- | --- | --- | --- |
| 0 | crate 重命名 `e3d-reader` → `e3d-io` | `Cargo.toml` 包名、模块路径、跨仓库引用全部切换；`e3d-reader` README / `goals/e3d31-rust-readonly-io/CRATE_LOCATION.md` 互链已更新 | 跨仓库引用同步需要小心；旧名称软重定向期 |
| 1 | 恢复写入侧函数家族 | `ida_exports/3.1/db_write_functions.json` 与 `struct_writeback.json` 含 `db_save_work` / `FHDBWN` / `FHSPLT` / `db_claim_element` 等的层级、常量、调用链与证据 | 写路径函数可能比读路径更深、call graph 更复杂 |
| 2 | Page write + journal | Rust 能写入单页并保持 header 一致性；session journal 条目格式被恢复并能写入 | journal 字段意义不确定时不可猜测 |
| 3 | Claim / release | claim 在 in-memory navigation stack 上标记锁；release 清理；并发由 `core.dll` 自身规则托管 | 与全局 `dword_6A54024` 栈交互复杂 |
| 4 | In-place attribute update | 不触发 B+tree 变更的纯 record 修改 + commit；oracle 校验通过 | record 跨页时改长度需要 split，先回避 |
| 5 | Create / delete | 元素 create / delete 路径打通；B+tree insertion + leaf split | `FHSPLT` 内部布局未恢复时风险高 |
| 6 | Save work + session commit | `db_save_work` 等价路径完整：page flush → journal close → header update | 失败恢复策略未定 |
| 7 | Round-trip 验证 | 用 oracle 读 Rust 写出的库，断言一致；至少 1 个 split 用例 | 一致性面广，可能发现前置 slice 缺陷 |

## 执行顺序

- Slice 0 必须最先做（crate 重命名是后续所有改动的前提）。
- Slice 1 紧随其后，否则后续全是猜测。
- Slice 2-3 在 Slice 1 通过后可并行。
- Slice 4 阻塞 5 之前的简单回归基线。
- Slice 5 是最大风险点，需要更多 IDA / fixture 证据。
- Slice 6 在 4/5 之前可先做"无变更 commit"的基础形态。
- Slice 7 全程串联，必须等 oracle goal 可用。

## 验收标准

- [ ] 写入侧函数与结构有 IDA 证据与置信度，列入 `ida_exports/3.1/db_write_functions.json`。
- [ ] Rust 写 → `core.dll` 读，至少 5 类操作一致。
- [ ] 至少 1 个 B+tree split 用例通过 oracle 验证。
- [ ] 中断恢复策略在 brief 中明确（rollback / fail-fast）。

## 方向控制

- 永远先在 oracle 上对照，再扩大覆盖。
- 任何"看起来更优雅"的偏离 IDA 行为的实现都必须先做 oracle 验证。
- 失败时优先保留原文件；写到临时文件 + 原子重命名作为最低安全网。
