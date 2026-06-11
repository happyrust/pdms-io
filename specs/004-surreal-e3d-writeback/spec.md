# Feature Specification: SurrealDB → E3D 写回（安全管道）

**Feature Branch**: `004-surreal-e3d-writeback`

**Created**: 2026-06-11

**Status**: Draft（grill Q1~Q6 决策收敛产物,全按推荐拍板;决策记录与存量证据见 research.md）

**Input**: User description: "specs/003 落库闭环后,按推荐继续——004 = SurrealDB→E3D 写回"

> 说明:001 已交付**验证过的写能力**(COW 原语全集 + batch 单会话原子 + dry-run/diff + verify_commit + 护栏),003 已交付库侧最新态(`pe`)与幂等机制。004 只做两者之间的**管道**:把库侧编辑意图安全地写回 `.e3d` 文件。格式/写语义引用 001,引擎架构引用 002,落库表引用 003,本文不重复。

## User Scenarios & Testing *(mandatory)*

### User Story 1 - 编辑意图写回文件 (Priority: P1)

调用方提交一批强类型编辑(`Vec<EditOp>`:改名/位置/内联值/成员/删除/克隆插入),写回管道将其作为**单一新会话**原子落到 E3D 文件副本,读回逐项一致。

**Acceptance Scenarios**:
1. **Given** sam7200 与一批混合 EditOp, **When** 调用写回, **Then** 输出副本 `sesno+1`、全部编辑读回一致、前会话字节不变(多版本保留)。
2. **Given** 批中任一笔非法(目标不存在/护栏拦截), **When** 调用, **Then** 整批回滚、不产出副本、错误上抛。

### User Story 2 - 安全闸门 (Priority: P1)

写回默认**只写副本**;落盘前 `verify_commit` 四类校验(B 树不变式/COW 不可变/读回等值/引用完整)强制通过,不过即失败。`--inplace` 仅显式 opt-in 且二次确认。

**Acceptance Scenarios**:
1. **Given** verify 失败的构造场景, **When** 写回, **Then** 无副本产出、原文件零字节变化。
2. **Given** 默认参数, **When** 写回成功, **Then** 原文件零字节变化,副本可独立打开读取。

### User Story 3 - 队列化与幂等 (Priority: P2)

库侧 `writeback_queue` 表承载编辑意图(确定性批次 ID + 状态机 pending→applied/failed);同一批次重复执行不重复写文件(sesno 不再增长),回执(新 sesno/diff 摘要)回写队列行。

**Acceptance Scenarios**:
1. **Given** 已 applied 的批次, **When** 重复 apply, **Then** 跳过且可观测,文件不变。
2. **Given** 队列含 pending 批次, **When** apply, **Then** 状态流转 applied + 回执字段在位。

### Edge Cases

- **回声(Q4=接受)**:写回产生新 sesno,watcher/ingest 会把它再读为增量入库——幂等 upsert 下库内容收敛于写回意图,回声即"生效证明",不建抑制标记。
- **无名元素寻址**:EditOp 以 refno 为第一寻址(库侧真相),不依赖 NAME(无名元素必须可编辑);name 仅作辅助选择器。
- **`e3d_io` 红线**:004 为纯管道层,`crates/e3d_io` **零改动**;若发现写原语 API 缺口,记录并上报决策,禁止顺手改。
- **并发写**:范围外(FR-010);同一文件同时仅允许一个写回流程(文件级互斥由调用方保证,契约声明)。

## Requirements *(mandatory)*

### Functional Requirements

**写回核心(P1)**
- **FR-001**: 强类型 `EditOp` MUST 覆盖 001 已验证原语全集:Rename/SetPos/SetInline/SetMembers/Delete/InsertClone(Q2=A)。
- **FR-002**: 一批 EditOp MUST 经 `EdbWriter::batch` 合为**单新会话**原子提交;任一笔失败整批回滚(001 FR-020 语义)。
- **FR-003**: 落盘前 MUST `verify_commit` 强制通过;失败则不产出任何文件变更。
- **FR-004**: 默认 MUST 写副本(原文件零字节变化);in-place MUST 显式 opt-in + 二次确认(Q3)。
- **FR-005**: 写回核心 MUST 为纯函数面(输入字节 + EditOp → 输出字节 + 报告),不依赖 SurrealDB 连接(Q1=C 层,kv-mem 之外亦可测)。
- **FR-006**: EditOp MUST 以 refno 为第一寻址;无名元素 MUST 可编辑。
- **FR-007**: 写回报告 MUST 含:新 sesno、逐笔结果、dry-run 元素级 diff 摘要(复用 001 `element_diff`)。

**队列层(P2)**
- **FR-008**: `writeback_queue` 表行 MUST 强类型构造(D2 I4 延续);批次 ID 确定性生成;状态机 pending→applied/failed,回执回写。
- **FR-009**: 同一批次重复 apply MUST 幂等跳过(可观测计数;文件 sesno 不增长)。

**范围外(明确不做)**
- **FR-010**: MUST NOT 触及:真机 E3D 联动(001-T039)、并发/分布式写、多 extent、跨库 refno 重映射、UI/服务化、Meilisearch、`crates/e3d_io` 任何改动、`PdmsIO` C1 冻结面变更。

### Key Entities

- **`EditOp`**:强类型编辑意图(六原语);写回的唯一输入形态。
- **`WritebackReport`**:新 sesno + 逐笔结果 + diff 摘要 + verify 结论。
- **`writeback_queue` 表(新)**:批次 id(确定性)、dbnum、edits payload、status、applied_sesno、error、时间戳。

## Success Criteria *(mandatory)*

- **SC-001**: 六原语逐一 round-trip:EditOp 写回副本后 e3d_io 读回逐项相等(sam7200)。
- **SC-002**: 混合 ≥3 笔单会话原子:`sesno` 仅 +1;故意坏批整批回滚、文件字节不变。
- **SC-003**: verify 强制:构造坏提交场景,写回失败且零文件变更。
- **SC-004**: 队列幂等:同批次重放 apply,文件 sesno 不增长、状态不漂移(kv-mem)。
- **SC-005**: 回声收敛:写回 → 增量提取 → ingest 后,`pe` 表内容 == 写回意图;再 ingest 幂等。
- **SC-006**: 红线:`crates/e3d_io` diff 为空;C1 冻结锁未触发;默认与 `--features surrealdb` 套件全绿。

## Assumptions

- e3d_io 写原语与事务层(001 US2/US5)是充分的;若实测出 API 缺口 → 停下上报,不扩 004 范围。
- 文件级互斥(同一 .e3d 同时一个写回)由调用方保证;004 不实现锁。
- 基线同 001/002/003:AVEVA Everything3D 2.10;样本 sam7200(主)/ams1112(可选量级)。
