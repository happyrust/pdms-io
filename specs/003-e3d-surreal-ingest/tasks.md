# Tasks: E3D 增量 → SurrealDB 落库

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-06-11

> 标注:`[P1]`=US1/US2 主线;`[P2]`=US3;每 Phase 末为 GATE。范围外(FR-010)出现"顺手做"冲动时记 004 候选,不动手。

## Phase 0 — Research(已完成)

- [x] T000 存量盘点(三入口/表形态/连接基建/to_surql 教训)+ grill Q1~Q6 决策入档 → `research.md`、`contracts/db-contract.md`

## Phase 1 — kv-mem 测试基建

- [x] T101 [P1] 测试工具模块:`mem://` 连接、每测试独立 ns/db、D1 表初始化(`--features surrealdb` + `#[cfg(test)]`)— `src/tests/surreal_mem.rs`:共享静态运行时 `rt()`(**实测陷阱 ×2 入档**:① surrealdb 3.x 内嵌引擎在单线程 tokio rt 上**卡死**〔上轮 20 分钟无输出根因〕② 全局 `SUL_DB` 连接绑定首个连接所在运行时,`#[tokio::test]` 每测私有 rt 会拖死连接"closed channel"→ 共享 rt + 串行锁 + 独立 ns/db 解决)+ `isolated()` 守卫 + `table_count`(3.x 对不存在表报 NotFound 按 0 处理);表 schemaless 无需 DEFINE。新增直接依赖 `surrealdb-types`(SurrealValue 派生宏要求 `::surrealdb_types` 路径,与 rs-core 同款,feature 门控)
- [x] T102 [P1] 幂等冒烟(合成):确定性 ID upsert 重放 ≥3 次,count/内容不变(D2 I1/I2 最小例)— `kv_mem_upsert_replay_is_idempotent`(重放 3 次 count 恒 1 + 同 ID 改值 = 覆盖非跳过,区别于旧 INSERT IGNORE)+ `kv_mem_isolation_between_test_dbs`(隔离自检)
- [x] T103 GATE:kv-mem 冒烟绿;无外部服务依赖;`e3d_io` 零改动 — **2026-06-11 通过**:2 测试绿(0.98s,纯 mem://);默认特性套件 39/0/5 不受影响;`crates/e3d_io` 未触碰

## Phase 2 — 入口真实化

- [ ] T201 [P1] `EleOperationData` → 强类型落库构造(serde 序列化;ID=(dbnum,refno,sesno)/主数据 (dbnum,refno);禁字符串拼接,D2 I4)
- [ ] T202 [P1] `update_elements_to_database` 实现 D3 A1~A4:sesno 升序、ses→pe_ses_h(分块 100)→pe VERSION(skip_main_data 跳过)、块失败上抛、未连接明确报错
- [ ] T203 [P1] `ingest_watermark` 水位:单调升、`sesno ≤ 水位` 跳过 + 跳过计数可观测(D2 I3)
- [ ] T204 [P1] 幂等重放测试:同增量 3 次,逐表 count+内容等值(SC-002);水位跳过断言(SC-003)
- [ ] T205 [P1] 真实样本端到端:sam7200 增量 → kv-mem 逐项对应(SC-001);ams1112 一段增量入库,计数/耗时报告(SC-004);skip_main_data 语义测试(FR-006)
- [ ] T206 GATE:`--features surrealdb` workspace 构建+测试全绿;D3 契约逐条核对;api_freeze 锁未触发

## Phase 3 — 存量收敛(仅在 T206 过后)

- [ ] T301 [P2] `to_surql` 删除;`collect_and_save_latest_data` 保存段改委托新入口(收集段保留);消费 bins(demo_latest_data_save/test_meilisearch)随迁
- [ ] T302 [P2] `store_all_refno_sesno_map` 盘点决策落地(并入退役 or 保留为全库历史回填入口),回填 D4
- [ ] T303 [P2] `sync_history` 等成片注释坟场删除
- [ ] T304 GATE:落库语句构造点 grep 单一入口(+D4 声明项);全绿;`crates/e3d_io` diff 为空(SC-005/006)

## Phase 4 — 文书

- [ ] T401 ARCHITECTURE 数据流(持久化段)更新 + CHANGELOG 条目
- [ ] T402 GATE:SC-001~SC-006 逐条核销;spec Status → Implemented

## 依赖关系

```
T101→T102→T103(GATE)
T103→T201→T202→T203→T204/T205→T206(GATE)
T206→T301/T302/T303→T304(GATE)→T401→T402(GATE)
```

## 范围外提醒(FR-010)

Meilisearch、`sync/`、SurrealDB→E3D 写回(004)、表结构重设计、`SECOND_SUL_DB`/`KV_DB` 多库路由、ws 服务运维、`crates/e3d_io` 任何改动——**本清单不含以上任何项**。
