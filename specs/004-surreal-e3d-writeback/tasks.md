# Tasks: SurrealDB → E3D 写回（安全管道）

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-06-11

> 标注:`[P1]`=US1/US2 主线;`[P2]`=US3;每 Phase 末为 GATE。范围外(FR-010)出现"顺手做"冲动时记 005 候选,不动手。**e3d_io 红线**:任何 API 缺口=停下上报,不改。

## Phase 0 — Research(已完成)

- [x] T000 存量盘点(写能力六件套/库侧形态/R1~R4 风险)+ grill Q1~Q6 决策入档 → `research.md`、`contracts/writeback-contract.md`

## Phase 1 — 写回核心(纯函数,默认特性可测)

- [ ] T101 [P1] `EditOp` 强类型枚举(契约 E1:六原语 + InlineValue + schema_version,serde)— 新模块 `src/writeback_core.rs`(默认特性;不依赖 surrealdb)
- [x] T102 [P1] refno 寻址映射核查(R1):六原语逐一确认 refno→寻址组合面(`record_off_via_root`+`EdbWriter`/`cow_*` pub API);缺口即停上报 — **核查结论 2026-06-11(证据=lib.rs 实读)**:① 低层 `cow_*` 全 pub 且 offset/refno 导向(inline@915/da_text@1112/members@1182/insert@1459,1514/delete@1531 refno 原生),`record_off_via_root`@767 pub,组合面在"裸 Edb"上充分 ② **但单会话原子只能经 `EdbWriter::batch`**(@1712:base 快照→闭包→私有 `collapse_session` 收敛/私有 `buf` 回滚),闭包内仅 `&mut EdbWriter`,其全部修改方法 name 导向(@1669~1704),`db()` 只读 ③ ⇒ **无名元素(sam7200 10392 中 9183 个,~88%)在现有 pub 面上无法进入 batch 单会话编辑 = E1-I1 不可满足**。**已停,红线决策上报用户**(选项 A=e3d_io 增 6 个 refno 导向 `*_at` 薄变体〔推荐〕/ B=pub db_mut 逃生舱 / C=004 砍无名支持);决策记录见 research.md R1
- [ ] T103 [P1] `apply_writeback(db_bytes, ss, edits)`(契约 E3-A1~A4):batch 单会话原子 + verify_commit 强制 + element_diff 摘要 + InsertClone 新 refno 回带
- [ ] T104 [P1] 文件包装:默认副本 `<db>.e3dout`(原文件零字节变化)/ inplace 显式 opt-in(E3-A5)
- [ ] T105 [P1] 测试(sam7200,缺样本跳过):六原语逐一 round-trip(SC-001)/ 混合 ≥3 笔单会话 sesno+1(SC-002 前半)/ 坏批整批回滚字节不变(SC-002 后半)/ verify 拦截零产出(SC-003)/ 无名元素编辑(E1-I1)
- [ ] T106 GATE:默认特性 `cargo test` 全绿;`crates/e3d_io` diff 为空;C1 冻结锁未触发

## Phase 2 — 队列层(kv-mem,`--features surrealdb`)

- [ ] T201 [P2] `writeback_queue` 行类型(契约 E2:确定性批次 id/状态机/回执字段,强类型构造)— `src/surreal_writeback.rs`(feature 门控)
- [ ] T202 [P2] `apply_queue(dbnum, db_path)`:取 pending 批次(created_at 序)→ EditOp → 写回核心 → 回执回写(applied_sesno/new_refnos/diff_summary;失败写 error+failed)
- [ ] T203 [P2] 幂等与状态机测试(kv-mem):入队→apply→applied + 回执在位;applied 重放跳过可观测、文件 sesno 不增长(SC-004);failed 不自动重试(E2 语义)
- [ ] T204 GATE:`--features surrealdb` lib + surreal 套件全绿

## Phase 3 — 回声闭环 + CLI

- [ ] T301 [P2] 回声收敛测试:写回副本 → `collect_increment_eles`(副本)→ `ingest_increments` → `pe` 内容 == 写回意图;再 ingest 幂等(SC-005;Q4 语义证明)
- [ ] T302 [P2] CLI 入口(独立 bin `e3d-writeback`,不动 e3d_io):`plan`(dry-run diff 预览)/`apply`(队列或 edits 文件;默认副本,--inplace + --yes)
- [ ] T303 GATE:`--features surrealdb` workspace 构建+全部测试全绿;api_freeze 未触发;e3d_io diff 为空(SC-006)

## Phase 4 — 文书

- [ ] T401 ARCHITECTURE 数据流(写回段)+ CHANGELOG 条目
- [ ] T402 GATE:SC-001~SC-006 逐条核销;spec Status → Implemented

## 依赖关系

```
T101→T102→T103→T104→T105→T106(GATE)
T106→T201→T202→T203→T204(GATE)
T204→T301/T302→T303(GATE)→T401→T402(GATE)
```

## 范围外提醒(FR-010)

真机 E3D 联动(001-T039)、并发/分布式写、多 extent、跨库 refno 重映射、UI/服务化、UDA/DA 任意文本编辑面扩展、`crates/e3d_io` 改动、`PdmsIO` C1 变更——**本清单不含以上任何项**。
