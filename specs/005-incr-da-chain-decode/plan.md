# Implementation Plan: 增量 DA 链式解析（R5）+ 首次命名

**Branch**: `005-incr-da-chain-decode` | **Date**: 2026-06-11 | **Spec**: [spec.md](./spec.md)

## Summary

修复 R5:e3d_io 读视图增「DA/members 链感知的记录重组」,门面增量路径换链式重组流——改名等 DA 编辑的回声从此进库;顺路补 `SetName`(无名元素首次命名)。e3d_io 触碰限定契约 F4 白名单(沿 004 决策 A 治理);`PdmsIO` C1 零变更。

## Technical Context

**Language**: Rust(edition 2024;e3d_io std-only 红线不变)

**Dependencies**: 零新增

**Testing**: e3d_io 独立套件 + workspace 双套件(默认/`--features surrealdb`);oracle = 邻接库等价(F1-I2)+ decode_full 对齐(F1-I4)+ diag_ams1112 不回归

**Constraints**: e3d_io 改动仅 F4 白名单;C1 冻结;重组布局以 parse_pdms_db 现行预期为准

## Constitution Check

| 原则 | 状态 |
|---|---|
| I. 纯离线 | ✅ 纯解析/管道工作 |
| II. 取证式逆向 | ✅ 链格式引 001 既有结论(chain/decode_da_list),不立新真相 |
| III. 双实现对齐/测试先行 | ✅ F1-I4 对齐测试是核心验收;每阶段测试先行 |
| IV. 非破坏写 | ✅ SetName 走既有 COW 通路 + verify |
| V. 规模健壮 | ✅ 链有界环防;ams1112 回归在闸 |

**结论**:无违反项。

## Phases

### Phase 0 — Research(已完成)
R5 机理 + Q1~Q6 决策 → research.md、contracts/chain-decode-contract.md。

### Phase 1 — e3d_io 扩展(契约 F4 白名单)
1. 契约红线修订入档(本计划即含)。
2. `Rdb` 链式记录重组(F1-I1~I3)+ 邻接等价/坏链测试。
3. `EdbWriter::set_name_at`(F3)+ 无名命名/已名改写测试。
**闸门**: e3d_io 独立套件全绿;cargo tree 单节点;白名单外零改动。

### Phase 2 — 门面接线(R5 修复本体)
1. 增量路径换链式重组流(F2-A1,公共签名零变更)。
2. 双实现对齐测试(F1-I4,sam7200 抽样 + e3d_io 改写元素)。
3. 邻接库不回归(F1-I2 oracle)。
**闸门**: 默认特性全套件绿(含 diag_ams1112)。

### Phase 3 — 写回侧收口
1. `EditOp::SetName` + 队列/报告沿 004 形态。
2. 回声转正:Rename/SetName 回声进库测试(SC-001/004;解除 004 测试的 R5 限定注记)。
**闸门**: `--features surrealdb` 全量绿;api_freeze 未触发。

### Phase 4 — 文书
ARCHITECTURE(R5 注记更新)+ CHANGELOG + SC 核销 + spec → Implemented;004 research R5 条目回填"已修复(005)"。

## Risks

| 风险 | 缓解 |
|---|---|
| 重组布局与 parse_pdms_db 预期错位 | F1-I2 邻接等价 + F1-I4 对齐双闸;先读 parse 端布局假设再定重组形态 |
| 链异常(坏指针/环) | F1-I3 有界环防 + 类型化错误 |
| e3d_io 扩展蔓延 | F4 白名单审计;白名单外停下上报 |
| 性能(链页额外读) | 仅 DA/members 指针非零时追链;读视图页计数可观测 |
