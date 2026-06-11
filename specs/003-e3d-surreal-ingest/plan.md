# Implementation Plan: E3D 增量 → SurrealDB 落库

**Branch**: `003-e3d-surreal-ingest` | **Date**: 2026-06-11 | **Spec**: [spec.md](./spec.md)

## Summary

把落库收敛为单一真实入口:`update_elements_to_database` 从 no-op 真实化(确定性 ID 幂等 upsert + 水位表,强类型语句),kv-mem 内嵌引擎为验收基准;存量半成品路径(`to_surql` 占位/`collect_and_save_latest_data` 保存段/`store_all_refno_sesno_map`/注释坟场)按 002 孤岛方法论盘点并入或退役。`crates/e3d_io` 零改动。

## Technical Context

**Language**: Rust(edition 2024);落库代码在 `pdms_io`(`--features surrealdb` 门控,现状不变)

**Dependencies**: 无新增第三方;surrealdb dev-3.1(已统一)+ kv-mem feature(已有);`SUL_DB: Surreal<Any>`(rs-core,不改)

**Testing**: kv-mem `mem://` + 每测试独立 ns/db;真实样本 sam7200/ams1112;ws 真服务 = 可选 smoke

**Constraints**: 入口签名冻结(002 C1);存量表非破坏;幂等可重放;`e3d_io` 零改动;Meilisearch/sync/写回/多库路由范围外

## Constitution Check(v1.0.0)

| 原则 | 状态 |
|---|---|
| I 纯离线/纯文件优先 | ✅ 验收基准 kv-mem 内嵌,无外部服务依赖;ws 仅可选 smoke |
| II 取证式逆向 | ✅ 不涉格式结论;存量证据入 research §1 |
| III 双实现与对齐 | ✅ 不触读取/格式;oracle = 既有增量断言 + 新幂等测试 |
| IV 非破坏性写入 | ✅ 只写 SurrealDB;E3D 文件零写入;存量表非破坏 |
| V 规模与健壮性 | ✅ SC-004 ams1112 端到端;分块/失败上抛/重放补齐 |

**结论**:无违反项。

## Phases(带 GATE)

### Phase 0 — Research(已完成)
存量盘点 + Q1~Q6 决策 → research.md / contracts/db-contract.md。

### Phase 1 — kv-mem 测试基建
1. 测试工具:`mem://` 连接 + 独立 ns/db 隔离 + 表初始化(D1)。
2. 最小冒烟:upsert 重放幂等(I1/I2)合成例。
**GATE**: kv-mem 冒烟绿,不依赖任何外部服务。

### Phase 2 — 入口真实化
1. `EleOperationData` → 强类型落库(D2 I4;ID 按 D2 I1)。
2. `update_elements_to_database` 实现 D3 A1~A4(分块/失败上抛/skip_main_data)。
3. 水位表 + 跳过逻辑(I3)。
4. 幂等重放/水位/真实样本端到端测试(SC-001~004)。
**GATE**: `--features surrealdb` 构建+测试全绿;入口行为契约逐条核对。

### Phase 3 — 存量收敛
D4 处置表逐项执行(to_surql 删除/保存段委托/store_all_refno_sesno_map 盘点回填/坟场清理)。
**GATE**: 落库构造点 grep 单一入口;全绿;`e3d_io` diff 为空。

### Phase 4 — 文书
ARCHITECTURE 数据流更新、CHANGELOG、SC-001~006 核销、spec → Implemented。

## Risks

| 风险 | 缓解 |
|---|---|
| `SUL_DB` 全局单例跨测试串库 | 每测试独立 ns/db + serial 标注(必要时) |
| 旧 `INSERT IGNORE` 记录与新 upsert ID 形态不一致 | D1 兼容核对项,实现期回填;水位表只对新写生效 |
| `Modified(ModifiedElement)` 载荷大(全量属性 diff) | 主数据走 serde 强类型 JSON;分块大小可调;SC-004 量化 |
| surrealdb dev-3.1 nightly API 漂移 | 锁单栈已完成(001 T040);新代码仅用稳定 surface(upsert/query/version) |
