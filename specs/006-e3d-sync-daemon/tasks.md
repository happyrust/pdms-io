# Tasks: E3D⇄DB 常驻同步链路（e3d-syncd）

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-06-11

> `[P1]`=US1/US2;`[P2]`=US3;每 Phase 末 GATE。红线 G4(零新落库构造点/e3d_io diff 空/C1 冻结)贯穿。**实现排序:005 T103~T402 收口在先,006 实现在后**(spec 先行)。范围外冲动记 007 候选(写回自动化已在册)。

## Phase 0 — Research(已完成)

- [x] T000 存量盘点(六环节可复用/缺口仅两层/静定窗依据)+ grill Q0~Q6 决策入档 → `research.md`、`contracts/sync-daemon-contract.md`

## Phase 1 — 同步核心(`src/sync_core.rs`,feature 门控)

- [x] T101 [P1] `scan_targets(dirs, whitelist) -> Vec<DbTarget>`(G1-A1:`*_0001` 识别,读头取 dbnum,坏文件记跳过)+ 单测(临时目录混入非库文件/坏文件)— **2026-06-12 落地** `src/sync_core.rs`:WalkDir 递归 + 名匹配 + 64B 读头(db_num@0x08)+ 白名单;<64B 文件静默跳过(垃圾头文件进列表由 sync_db Failed 隔离,SC-003 路径)
- [x] T102 [P1] `sync_db(target) -> DbSyncOutcome{Synced|SkippedUpToDate|Baseline|Failed}`(G1-A2/A3:水位比对→范围增量→唯一入口;初见库以最新会话立基线;纯水位无本地状态)— 错误折叠 `Failed(String)` 供守护壳隔离;`bootstrap_db`(G1-A4)委托既有 `collect_and_save_latest_data`;`read_watermark` 提为 pub(crate) 复用(零新落库构造点)
- [x] T103 [P1] kv-mem+临时目录测试:首轮基线 / e3d_io 写新会话替换→捕获→水位前进(SC-001)/ 重复轮零写库+重启(新实例)零重复(SC-002)/ 坏文件隔离健康库照常(SC-003)— 3 测试一次全过:扫描过滤(干扰文件/白名单)/ 全链(Baseline→Skipped→writeback 新会话顶替→Synced{from,to}→重启 Skipped)/ 垃圾头库 Failed 而健康库同轮 Baseline
- [x] T104 GATE:`--features surrealdb` 核心测试全绿;落库构造点 grep 零新增;e3d_io diff 空 — **2026-06-12 通过**:lib 56/0/5;`sync_core.rs` grep 零 SQL/upsert 构造;`crates/e3d_io` diff 为空。**Phase 1 收口,Phase 2(守护壳)解锁**

## Phase 2 — 守护壳(`src/bin/e3d_syncd.rs`)

- [ ] T201 [P1] 事件循环:notify(递归)→ 待同步标记 → 静定窗(默认 500ms)→ 同步轮;轮询兜底(默认 30s);同库单飞串行(G2-A1~A3)
- [ ] T202 [P1] 参数面(--dirs/--dbnums/--settle-ms/--poll-secs/--bootstrap N/--surreal/--ns/--dbname)+ 优雅退出(G2-A4)+ 每轮日志/累计计数(G3-A1)
- [ ] T203 [P2] 静定窗合并单测(合成事件风暴→一轮)+ bin smoke(临时目录实跑:启动→触发→退出)
- [ ] T204 GATE:SC-004;bin 编译 + smoke 通过

## Phase 3 — 端到端与收口

- [ ] T301 [P1] e2e(kv-mem):sam7200 → 基线 → 新会话版本替换 → 捕获入库 → 水位前进 → 模拟重启幂等(SC-001/002 全链)
- [ ] T302 GATE:双特性全量套件全绿;api_freeze 未触发;`crates/e3d_io` diff 为空(SC-005)

## Phase 4 — 文书

- [ ] T401 ARCHITECTURE「监控与同步」段改述(雏形→守护目标态)+ CHANGELOG 条目
- [ ] T402 GATE:SC-001~SC-005 逐条核销;spec Status → Implemented

## 依赖关系

```
T101→T102→T103→T104(GATE)
T104→T201→T202→T203→T204(GATE)
T204→T301→T302(GATE)→T401→T402(GATE)
(实现前置:specs/005 T103~T402 收口)
```

## 范围外提醒(FR-008)

写回自动 apply(007)、`sync/` 远端分发、Meilisearch、ws 部署、多工程并发调度、`crates/e3d_io` 改动、C1 变更——**本清单不含以上任何项**。
