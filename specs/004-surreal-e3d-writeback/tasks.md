# Tasks: SurrealDB → E3D 写回（安全管道）

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-06-11

> 标注:`[P1]`=US1/US2 主线;`[P2]`=US3;每 Phase 末为 GATE。范围外(FR-010)出现"顺手做"冲动时记 005 候选,不动手。**e3d_io 红线**:任何 API 缺口=停下上报,不改。

## Phase 0 — Research(已完成)

- [x] T000 存量盘点(写能力六件套/库侧形态/R1~R4 风险)+ grill Q1~Q6 决策入档 → `research.md`、`contracts/writeback-contract.md`

## Phase 1 — 写回核心(纯函数,默认特性可测)

- [x] T101 [P1] `EditOp` 强类型枚举(契约 E1:六原语 + InlineValue + schema_version,serde)— 新模块 `src/writeback_core.rs`(默认特性;不依赖 surrealdb):`EditOp` 六变体 + `InlineValue`(Reals/Ints/Refs,禁裸字节)+ `EditBatch{schema_version}` + `EditResult`/`DiffSummary`/`WritebackReport` 全 serde
- [x] T102 [P1] refno 寻址映射核查(R1):六原语逐一确认 refno→寻址组合面(`record_off_via_root`+`EdbWriter`/`cow_*` pub API);缺口即停上报 — **核查结论 2026-06-11(证据=lib.rs 实读)**:① 低层 `cow_*` 全 pub 且 offset/refno 导向(inline@915/da_text@1112/members@1182/insert@1459,1514/delete@1531 refno 原生),`record_off_via_root`@767 pub,组合面在"裸 Edb"上充分 ② **但单会话原子只能经 `EdbWriter::batch`**(@1712:base 快照→闭包→私有 `collapse_session` 收敛/私有 `buf` 回滚),闭包内仅 `&mut EdbWriter`,其全部修改方法 name 导向(@1669~1704),`db()` 只读 ③ ⇒ **无名元素(sam7200 10392 中 9183 个,~88%)在现有 pub 面上无法进入 batch 单会话编辑 = E1-I1 不可满足**。**已停,红线决策上报用户**(选项 A=e3d_io 增 6 个 refno 导向 `*_at` 薄变体〔推荐〕/ B=pub db_mut 逃生舱 / C=004 砍无名支持);决策记录见 research.md R1
- [x] T102a **决策 A 落地**:e3d_io 增 refno 导向薄变体(`set_inline_at`/`set_pos_at`/`rename_at`/`set_members_at`/`delete_at`/`insert_clone_at` + `offset_of_refno`/`element_at`,严格同构 name 方法)+ 测试 `edbwriter_refno_oriented_unnamed`(无名元素 batch 单会话 sesno+1 / 读回 / ElementNotFound 类型化 / 坏批整批回滚 / rename_at 同构语义拦截)— **39+1 全绿**;契约 E4 红线已按决策修订
- [x] T103 [P1] `apply_writeback(db_bytes, ss, edits)`(契约 E3-A1~A4):batch 单会话原子 + verify_commit 强制 + element_diff 摘要 + InsertClone 新 refno 回带 — 落地:`*_at` 薄变体驱动六原语;Delete force=false 先过 `delete_guards`;verify 后追加**逐笔 refno 读回核验**(Expect 为 name 导向,无名目标以 refno 路径补强);Err ⇒ 零输出字节。**实测边界入契约 E3-A1 注**:同批先编辑模板再 InsertClone 触发克隆 DA 布局前置拒绝 ⇒ 克隆应排在模板编辑之前
- [x] T104 [P1] 文件包装:默认副本 `<db>.e3dout`(原文件零字节变化)/ inplace 显式 opt-in(E3-A5)— `apply_writeback_file` + `WriteMode::{Copy, InPlace{confirmed}}`,未确认 in-place 直接拒绝
- [x] T105 [P1] 测试(sam7200,缺样本跳过):六原语逐一 round-trip(SC-001)/ 混合 ≥3 笔单会话 sesno+1(SC-002 前半)/ 坏批整批回滚字节不变(SC-002 后半)/ verify 拦截零产出(SC-003)/ 无名元素编辑(E1-I1)— 4 测试:① 五原语混合一批(含两个无名元素)单新会话 + 独立重载读回 + 第二批 delete 克隆(六原语全覆盖)② 坏批 Err 零输出 ③ 护栏拦 force=false 删父 + verify(DanglingRef)拦 force=true 删父 ④ 文件包装副本模式原文件逐字节不变 + 未确认 in-place 拒绝
- [x] T106 GATE:默认特性 `cargo test` 全绿;`crates/e3d_io` diff 为空;C1 冻结锁未触发 — **2026-06-11 21:14 通过,exit 0**:lib 43/0/5(39 基线 + 4 写回)、api_freeze_c1 ✓、diag 5✓(396s)等集成全绿;`git diff crates/e3d_io` 为空(决策 A 改动已入 47672eed,此后零触碰)。**Phase 1 收口,Phase 2(队列层)解锁**

## Phase 2 — 队列层(kv-mem,`--features surrealdb`)

- [x] T201 [P2] `writeback_queue` 行类型(契约 E2:确定性批次 id/状态机/回执字段,强类型构造)— `src/surreal_writeback.rs`(feature 门控):`WritebackQueueRow`(SurrealValue;`edits_json`=EditBatch serde_json 单一定义含 schema_version;空串/0 哨兵替代 Option 规避派生面风险);`enqueue_writeback` 确定性 id upsert(同批重复入队=覆盖不重复,实测断言)
- [x] T202 [P2] `apply_queue(dbnum, db_path)`:取 pending 批次(created_at 序)→ EditOp → 写回核心 → 回执回写(applied_sesno/new_refnos/diff_summary;失败写 error+failed)— 落地 + **顺序语义注**:同 dbnum pending 按 (created_at,batch_id) 升序,**首败即停**(该批记 failed 后上抛,后续批保持 pending——批间或有依赖,不越过失败点盲跑;修复后幂等续跑);schema_version 失配 = failed(E1-I2 闸)
- [x] T203 [P2] 幂等与状态机测试(kv-mem):入队→apply→applied + 回执在位;applied 重放跳过可观测、文件 sesno 不增长(SC-004);failed 不自动重试(E2 语义)— 3 测试(kv-mem + sam7200 临时副本):① apply→applied+回执(applied_sesno/diff/applied_at)→重复 apply 全跳过且零文件产出、源文件逐字节不变 ② 坏批 Err+failed+error 在位,重 apply skipped_failed 不重试 ③ schema_version=999 → failed 且错误可读
- [x] T204 GATE:`--features surrealdb` lib + surreal 套件全绿 — **2026-06-11 通过**:lib **52/0/5**(45 基线 + 4 写回核心 + 3 队列)2.64s

## Phase 3 — 回声闭环 + CLI

- [x] T301 [P2] 回声收敛测试:写回副本 → `collect_increment_eles`(副本)→ `ingest_increments` → `pe` 内容 == 写回意图;再 ingest 幂等(SC-005;Q4 语义证明)— `writeback_echo_converges_into_pe`:队列写回(无名元素 SetPos)→ watcher 视角读副本提取新会话增量(含被编辑元素,op=修改)→ 门面唯一入口入库 → `pe` 行 sesno==写回会话、非墓碑、attrs 携带写回 POS 数值 → 再 ingest 被水位拦截逐表计数不变。**途中实测发现 R5 回声盲区**(DA 文本编辑被 v1 窗口邻接解析漏检,文件级真相不受影响;入档 research R5,005 候选),回声编辑面据此限定内联属性
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
