# Feature Specification: 增量读取 DA 链式解析（R5 修复）+ 无名元素首次命名

**Feature Branch**: `005-incr-da-chain-decode`

**Created**: 2026-06-11

**Status**: Implemented（2026-06-12;SC-001~005 全核销,见 tasks.md T402。grill Q1~Q6 决策收敛产物,全按推荐拍板;决策记录见 research.md）

**Input**: User description: "specs/004 闭环后,按推荐继续——005 = R5 回声盲区修复 + 首次命名"

> 说明:004 实测发现 **R5**——e3d_io 把 DA 文本重定位到远页(格式合法),而增量读取的 `EleData` 解析是窗口邻接假设,看不见远页 DA ⇒ **改名等 DA 编辑进不了增量、落不了库**,是「文件 ⇄ 库」双向流水线最后一个真缺口。005 修复它,并顺路补上与之共享 DA 条目机制的「无名元素首次命名」编辑面。格式真相引 001,引擎架构引 002,落库引 003,写回引 004。

## User Scenarios & Testing *(mandatory)*

### User Story 1 - DA 编辑的回声进库 (Priority: P1)

写回管道改名(或其它 DA 编辑)后,watcher 增量提取能看见该元素(op=修改),落库后 `pe.name` 收敛于新名——R5 限定解除。

**Acceptance Scenarios**:
1. **Given** 写回批含 `Rename`, **When** 读写回产物提取增量, **Then** 被改名元素在增量中(Modified)且 `EleData` 携带新名;ingest 后 `pe.name` == 新名。
2. **Given** PDMS 原生写的库(DA 邻接), **When** 增量提取, **Then** 行为与修复前逐项一致(不回归)。

### User Story 2 - 链式 DA 的属性完整性 (Priority: P1)

对任何记录(无论 DA 邻接还是远页链式),增量路径解出的 DA/显式属性与 e3d_io 全量解码(`decode_full`)一致——双实现对齐(宪法 III)。

**Acceptance Scenarios**:
1. **Given** sam7200 全库抽样 + e3d_io 改写过 DA 的元素, **When** 增量路径解析 vs `decode_full`, **Then** DA/显式属性集合逐项相等。

### User Story 3 - 无名元素首次命名 (Priority: P2)

库侧可对无名元素(真实库 ~88%)下发 `SetName` 编辑:写回后元素获得 NAME(DA 新增条目),读回与回声进库均生效。

**Acceptance Scenarios**:
1. **Given** 无名元素 + `SetName`, **When** 写回, **Then** 读回 name 生效、单会话原子、verify 通过;回声进库后 `pe.name` 在位。
2. **Given** 已有 NAME 的元素 + `SetName`, **Then** 行为等同改写(与 `Rename` 殊途同归);`Rename` 对无名元素仍按 004 语义报错(同构性不破坏)。

### Edge Cases

- **链页越界/环**:DA 链跟随 MUST 有界(页数上限 + 环防),坏链 ⇒ 类型化错误而非吞没。
- **members 链(rec[8]/rec[9])**:同属远页链;本轮一并纳入链式读取面(成员列表编辑的回声同样受 R5 影响)。
- **e3d_io 扩展治理**:Q2=A/Q5=B 都触碰 e3d_io——沿 004 决策 A 模式:契约白名单先行,扩展面之外仍是红线。

## Requirements *(mandatory)*

### Functional Requirements

**链式解析(P1)**
- **FR-001**: e3d_io 读视图 MUST 提供 DA/members 链感知的记录读取:跟随 rec[6]/rec[7] 与 rec[8]/rec[9] 链,把链页节点 payload 以 v1 解析器预期的邻接布局重组(重组逻辑 = 格式真相,只在 e3d_io)。
- **FR-002**: 门面增量路径(`parse_raw_element`/`read_element_record_cached` 之上)MUST 消费链式重组流;公共签名零变更(C1 冻结延续)。
- **FR-003**: 邻接库(PDMS 原生)上重组流 MUST 与原窗口流解析结果逐项一致(不回归;C3.1 oracle)。
- **FR-004**: 双实现对齐:重组流经 `parse_pdms_db` 解出的 DA/显式属性 MUST == e3d_io `decode_full` 同元素结果(抽样断言)。
- **FR-005**: 链跟随 MUST 有界且环防;坏链报类型化错误。

**首次命名(P2)**
- **FR-006**: e3d_io MUST 提供 `set_name_at(refno, name)`:无 NAME ⇒ DA 新增条目;已有 NAME ⇒ 改写(内部复用 pack_text/cow_da_set_entry 通路,不新增裸 pub)。
- **FR-007**: 写回管道 MUST 增 `EditOp::SetName`;`Rename`/`rename_at` 语义不变(改写既有,无名报错)。

**范围外**
- **FR-008**: MUST NOT 触及:store_all 历史回填强类型化、UDA 编辑面扩展、真机联动(001-T039)、并发、白名单外的 e3d_io 改动、`PdmsIO` C1 变更。

### Key Entities

- **链式重组流**:记录窗口 + 按链序重组的 DA/members 节点 payload(e3d_io 产出)。
- **`EditOp::SetName`**:`{ refno, name }`——首次命名/改写统一意图。

## Success Criteria *(mandatory)*

- **SC-001**: 回声 Rename 转正:写回改名 → 增量含 Modified(新名在 EleData)→ `pe.name` 收敛;004 回声测试的 R5 限定注记解除。
- **SC-002**: 双实现对齐:sam7200 全库抽样(≥200 元素,含 DA 改写元素)重组流解析 == `decode_full`(DA/显式属性逐项)。
- **SC-003**: 不回归:`diag_ams1112` 5 测试 + 全套件(默认与 `--features surrealdb`)全绿;邻接库解析结果与修复前一致。
- **SC-004**: SetName:无名元素命名 round-trip(读回 + 回声进库)+ 已名元素改写殊途同归 + `Rename` 同构语义保持。
- **SC-005**: 红线审计:e3d_io 改动仅限本 spec 白名单(契约 F4);std-only(cargo tree 单节点);api_freeze 全程绿。

## Assumptions

- DA/members 链节点格式以 001(`chain`/`node_chain_len`/`decode_da_list`)为权威;005 不建立新格式结论。
- 重组布局以 `parse_pdms_db` 现行解析器预期为准(它是 EleData 类型适配层,FR-003/002 定位不变)。
- 基线:sam7200(主)/ams1112(回归);kv-mem 落库验收。
