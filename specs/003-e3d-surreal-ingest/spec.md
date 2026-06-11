# Feature Specification: E3D 增量 → SurrealDB 落库（单一入口真实化）

**Feature Branch**: `003-e3d-surreal-ingest`

**Created**: 2026-06-11

**Status**: Implemented（2026-06-11;SC-001~006 全核销,见 tasks.md T402。grill-me Q1~Q6 决策收敛产物;决策记录与存量证据见 research.md）

**Input**: User description: "specs/002 收敛完成后,按推荐继续——003 = E3D→SurrealDB 落库"

> 说明：002 之后 `PdmsIO` 已是 `e3d_io` 之上的纯读取门面;「文件 → 增量 → 库」流水线唯一缺口 = 落库:`update_elements_to_database` 为冻结签名的 **no-op 占位**,`to_surql` 为**空串占位**,真实写表代码仅存于 `store_all_refno_sesno_map`(历史批量)一处。本规范把落库收敛为**单一真实入口**。格式/读取规则一律引用 specs/001、引擎架构一律引用 specs/002,本文不重复。

## User Scenarios & Testing *(mandatory)*

### User Story 1 - 增量真实落库 (Priority: P1)

watcher/调用方拿到一批增量(`BTreeMap<u32, Vec<EleOperationData>>`)后,调用 `update_elements_to_database` 即把会话与元素变更**真实写入** SurrealDB——不再是 no-op。

**Independent Test**: kv-mem 内嵌引擎上,对真实样本提取的增量调用入口,逐表核对记录数与内容与入参对应。

**Acceptance Scenarios**:
1. **Given** sam7200/ams1112 的一段增量, **When** 调用入口, **Then** `ses`/`pe_ses_h`(及主数据,除非 `skip_main_data`)记录与 `EleOperationData` 逐项对应。
2. **Given** `skip_main_data=true`, **When** 调用, **Then** 仅会话/索引关系落库,主数据表不写(既有参数语义保留)。

### User Story 2 - 幂等重放 (Priority: P1)

watcher 重启/补扫导致同一增量重复落库时,库内容**不重复、不漂移**:记录 ID 由 `(dbnum, refno, sesno)` 确定性生成,重放为 upsert 覆盖同值;已落会话由水位表快速跳过。

**Acceptance Scenarios**:
1. **Given** 同一增量, **When** 连续落库 N 次, **Then** 各表计数与内容与落库 1 次完全一致。
2. **Given** 水位表已记录 `dbnum → 已落 sesno`, **When** 重放旧会话范围, **Then** 入口跳过已落会话(可观测:跳过计数)。

### User Story 3 - 落库入口单一化 (Priority: P2)

维护者检索"谁在写 SurrealDB"时只命中一处入口路径:`store_all_refno_sesno_map`(历史批量)经盘点并入或明确分工;`to_surql` 空串占位与 `sync_history` 等成片注释坟场清除。

**Acceptance Scenarios**:
1. **Given** 收敛完成, **When** grep 落库语句构造点, **Then** 仅单一入口路径(+经契约声明的历史批量入口,若保留)。

### Edge Cases

- **aios_core `NamedAttrMap` SurQL 接口分支差异**(to_surql 占位的根因):003 不依赖该接口——新语句以 surrealdb 强类型/serde 序列化直接构造。
- **全局 `SUL_DB` 测试隔离**:kv-mem + 每测试独立 ns/db,避免测试间串库。
- **批量失败语义**:分块写入中任一块失败 → 返回错误且不静默吞(允许部分已写,幂等重放可补齐)。
- **`e3d_io` 红线**:003 是纯管道层工作,`crates/e3d_io` **零改动**。

## Requirements *(mandatory)*

### Functional Requirements

**入口真实化(P1)**
- **FR-001**: `update_elements_to_database(&BTreeMap<u32, Vec<EleOperationData>>, skip_main_data)` MUST 真实写库,公共签名不变(002 契约 C1 延续)。
- **FR-002**: 记录 ID MUST 由 `(dbnum, refno, sesno)` 确定性生成;同 ID 重写 MUST 为 upsert 语义。
- **FR-003**: 落库 MUST 幂等:同一增量重放 N 次,表计数与内容不变(SC-002)。
- **FR-004**: MUST 维护落库水位表(`dbnum → 已落 sesno`),重放已落会话可跳过且可观测。
- **FR-005**: 新写语句 MUST 用强类型/参数绑定构造(不做字符串拼接);**存量表结构(`ses`/`pe_ses_h`/`pe`)不做破坏性变更**。
- **FR-006**: `skip_main_data` 既有参数语义 MUST 保留。
- **FR-007**: 批量 MUST 分块写入;块失败 MUST 上抛错误,不静默。

**验收基建(P1)**
- **FR-008**: 测试 MUST 以 kv-mem 内嵌引擎为基准(`mem://`,独立 ns/db 隔离),全离线可重复;真实 ws 服务验收为可选 smoke。

**存量收敛(P2)**
- **FR-009**: `store_all_refno_sesno_map` MUST 经盘点决策(并入新入口 / 保留为历史批量入口并写入契约);`to_surql` 空串占位与 `collect_and_save_latest_data` 的占位保存路径、`sync_history` 注释坟场 MUST 清除或委托新入口。

**范围外(明确不做)**
- **FR-010**: 本规范 MUST NOT 触及:Meilisearch 索引、`sync/` 远端分发、SurrealDB→E3D 写回(留 004)、表结构重设计与 `SECOND_SUL_DB`/`KV_DB` 多库路由、ws 服务部署运维、`crates/e3d_io` 任何改动。

### Key Entities

- **`EleOperationData`**:增量载荷(refno/sesno/detail=Add|Modified|Deleted|None)——落库的唯一输入形态。
- **`ses` 表**:会话记录(id=会话标识,日期等);**`pe_ses_h` 表**:refno×sesno → offset/dbnum 索引关系;**`pe` 表**:元素主数据(VERSION 按会话时间)。
- **`ingest_watermark` 表(新)**:`dbnum → 已落 sesno` 水位。
- **`update_elements_to_database`**:唯一增量落库入口(门面方法,签名冻结)。

## Success Criteria *(mandatory)*

- **SC-001**: 真实样本增量经入口写入 kv-mem,各表记录与入参逐项对应(数量+关键字段)。
- **SC-002**: 同一增量重放 ≥3 次,各表计数与内容不变(逐字节/逐字段相等)。
- **SC-003**: 水位生效:重放已落会话被跳过,跳过行为可观测断言。
- **SC-004**: ams1112(103MB/42 万元素级)一段真实增量端到端入库,耗时与计数可报告。
- **SC-005**: 落库语句构造点 grep 收敛至单一入口路径(契约列外项除外);占位/注释坟场清零。
- **SC-006**: 全部验收在 kv-mem 离线完成;`crates/e3d_io` 零改动(diff 证明);`--features surrealdb` workspace 构建+测试全绿。

## Assumptions

- 表结构沿存量(`ses`/`pe_ses_h`/`pe` + INSERT IGNORE 原始幂等),003 升级其幂等语义为确定性 ID upsert,不重设计 schema。
- `SUL_DB` 为 `Surreal<Any>` 全局连接(rs-core),kv-mem 经 `mem://` 连接即可;不改 rs-core(授权在手但无需)。
- 基线同 001/002:AVEVA Everything3D 2.10;样本 sam7200/ams1112。
