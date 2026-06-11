# Implementation Plan: SurrealDB → E3D 写回（安全管道）

**Branch**: `004-surreal-e3d-writeback` | **Date**: 2026-06-11 | **Spec**: [spec.md](./spec.md)

## Summary

在 001 验证过的写能力(e3d_io COW/batch/verify/护栏)与 003 的库侧形态之间架一条**安全写回管道**:强类型 `EditOp` 六原语 → 纯函数写回核心(单会话原子 + verify 强制 + 默认副本)→ `writeback_queue` 队列层(确定性批次 + 状态机 + 幂等)→ 回声经既有 ingest 自然收敛。`crates/e3d_io` 与 `PdmsIO` C1 冻结面零改动。

## Technical Context

**Language/Version**: Rust(edition 2024;新模块 `src/surreal_writeback.rs` + 既有特性门控)

**Primary Dependencies**: 零新增;复用 e3d_io(写)/surrealdb-types(行类型)/003 kv-mem 测试基建

**Testing**: 写回核心=默认特性可测(纯函数,sam7200 副本);队列层=`--features surrealdb` kv-mem;两道 GATE 全绿准入

**Constraints**: e3d_io 零改动(红线);C1 冻结锁;默认副本;同文件单写回(调用方互斥)

**Scale/Scope**: 新增 ~2 模块 + CLI 入口;不删不改存量读取/落库路径

## Constitution Check

| 原则 | 闸门 | 状态 |
|---|---|---|
| I. 纯离线/纯文件优先 | 写回核心纯函数,kv-mem 全离线验收 | ✅ 无外部服务依赖 |
| II. 取证式逆向 | 不新增格式结论,写语义全引 001 | ✅ 管道层工作 |
| III. 双实现与对齐/测试先行 | 每 Phase 测试先行,oracle=001 读回 | ✅ tasks 标注 |
| IV. 非破坏性写入 | 默认副本 + verify 强制 + COW 多版本 | ✅ Q3 决策即宪法 IV |
| V. 规模与健壮性 | sam7200 主线,ams1112 可选量级 | ✅ 不退化(管道不碰读路径) |

**结论**:无违反项。

## Phases

### Phase 0 — Research(已完成)
存量盘点 + Q1~Q6 决策入档 → research.md。

### Phase 1 — 写回核心(纯函数,默认特性)
1. `EditOp` 强类型枚举(六原语,refno 第一寻址,serde 可序列化——供队列 payload 复用)。
2. `apply_writeback(db_bytes, ss, edits) -> (out_bytes, WritebackReport)`:EdbWriter::batch 组合 + refno→寻址映射(R1 重点核查)+ verify_commit 强制 + element_diff 摘要。
3. 文件包装:默认写 `<db>.e3dout` 副本;inplace 显式 + 确认。
**闸门**: sam7200 六原语 round-trip + 混合原子 + 坏批回滚 + verify 拦截全绿;e3d_io diff 为空。

### Phase 2 — 队列层(kv-mem,`--features surrealdb`)
1. `writeback_queue` 行契约(确定性批次 ID + 状态机)+ 强类型构造。
2. 出队 → `Vec<EditOp>` → 写回核心 → 回执回写(applied_sesno/新 refno/diff 摘要/错误)。
3. 幂等:applied 批次重放跳过(可观测);失败批次可重试。
**闸门**: kv-mem 全套绿(入队/apply/状态流转/幂等/失败重试)。

### Phase 3 — 回声闭环 + CLI
1. 回声测试:写回副本 → 增量提取 → ingest → `pe` 内容 == 写回意图;再 ingest 幂等(SC-005)。
2. CLI 入口(独立 bin 或 e3d-io 子命令外挂,不动 e3d_io):queue-apply/plan 模式。
**闸门**: `--features surrealdb` workspace 构建+测试全绿;C1 冻结锁未触发。

### Phase 4 — 文书
ARCHITECTURE(写回数据流)+ CHANGELOG + SC-001~006 核销 + spec → Implemented。

## Risks

| 风险 | 缓解 |
|---|---|
| R1 refno 寻址组合面不足(e3d_io 红线) | Phase 1 第一笔即核查;缺口=停下上报决策,不顺手改 |
| InsertClone 新 refno 回带 | WritebackReport 显式字段 + 队列回执测试 |
| 回声时序误判 | 测试用"读副本 ingest"显式模拟;接受回声为既定语义(Q4) |
| 队列 payload 漂移 | EditOp serde 版本字段 + 契约 E2 锁定 |
