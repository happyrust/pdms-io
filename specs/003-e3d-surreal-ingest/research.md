# Research: 落库存量盘点 + grill-me 决策记录

**Date**: 2026-06-11 | **Spec**: [spec.md](./spec.md)

> 固化两件事:① 落库存量代码的**现状证据**(逐条可复查);② grill-me Q1~Q6 决策记录。

## 1. 存量盘点(证据,2026-06-11 实查)

### 1.1 三个入口现状

| 入口 | 现状 | 证据 |
|---|---|---|
| `update_elements_to_database(&BTreeMap<u32,Vec<EleOperationData>>, bool)` | **no-op 占位**,签名被 002 契约 C1 冻结 | `io.rs`(直接 `Ok(())`);C1 注"保留接口并默认 no-op" |
| `store_all_refno_sesno_map` | **真实在写**:flume channel + tokio 后台批写,`INSERT IGNORE INTO ses [json]` / `INSERT IGNORE INTO pe_ses_h (id,refno,sesno,offset,dbnum,ses) VALUES ...` / `INSERT IGNORE INTO pe [json] VERSION dt` | `io.rs:1520+`(`SesSqlType` 三类消息,100 条/块) |
| `collect_and_save_latest_data` | 收集真实、**保存空转**:经 `to_surql` 拼语句,而 `to_surql` 返回**空串占位** | `io.rs:100`(占位注释:"上游 `aios_core::NamedAttrMap` 的 JSON/SurQL 生成接口在不同分支存在差异…避免误写数据库") |

另:`sync_history` 内有成片(数百行)被注释的落库代码(坟场);`demo_latest_data_save`/`test_meilisearch` bins 消费 `collect_and_save_latest_data`。

### 1.2 既有表形态(从 store_all_refno_sesno_map 语句反推)

- `ses`:会话记录,JSON 批插,`INSERT IGNORE`(id 重复即跳过)。
- `pe_ses_h`:`(id=[refno,sesno], refno, sesno, offset, dbnum, ses)` —— refno×会话 → 物理 offset 的索引关系。
- `pe`:元素主数据 JSON,`VERSION <rfc3339>`(SurrealDB 版本化),`INSERT IGNORE`。
- 幂等现状 = `INSERT IGNORE`(有 id 即跳过,**不覆盖**)——003 升级为确定性 ID **upsert**(同值覆盖,内容可修正)+ 水位表。

### 1.3 连接与测试基建事实

- `SUL_DB: Lazy<Surreal<Any>>`(rs-core `rs_surreal/mod.rs:100`),`Any` 引擎 ⇒ 连接串决定引擎;本仓 surrealdb 依赖 features = `protocol-ws` + **`kv-mem`** ⇒ 测试可 `connect("mem://")` 全离线。
- rs-core 另有 `SECOND_SUL_DB`/`KV_DB`/`SUL_MEM_DB`(feature 门控)——多库路由为 003 范围外。
- `EleOperationData { refno, sesno, detail: Add|Modified(ModifiedElement)|Deleted|None }` + `convert_to_operation_data` 已就位 = 落库唯一输入形态。

### 1.4 为什么 `to_surql` 是空的(教训入档)

占位注释言明:上游 `NamedAttrMap` 的 JSON/SurQL 生成接口在分支间漂移,直拼字符串易随上游漂移误写库。003 的对策(Q2 决策):**不依赖该接口**,新语句用 surrealdb 强类型/serde 序列化构造;字符串拼接路径整体退役。

## 2. grill-me 决策记录(2026-06-11,经 Best MCP 桥逐问确认)

| # | 问题 | 决策 | 要点/理由 |
|---|---|---|---|
| Q1 | 003 核心交付 | **A. E3D→SurrealDB 落库** | 冻结占位签名本就在等它;纯软件可验;流水线唯一缺口;dev-3.1 统一刚完成正好兑现。B(Surreal→E3D 写回)终判需真机,留 004 |
| Q2 | 三入口收敛策略 | **A. 单一入口 + 克制强类型** | `update_elements_to_database` 真实化为唯一增量入口;存量按 002 孤岛方法论盘点并入/退役;新语句强类型,存量表结构不动 |
| Q3 | 验收环境 | **A. kv-mem 内嵌为基准** | `mem://` 全离线可重复(本环境可验红线延续);ws 真服务为可选 smoke |
| Q4 | 重放语义 | **A. 幂等 upsert + 水位表** | ID=(dbnum,refno,sesno) 确定性;重放覆盖同值;`ingest_watermark` 跳过已落会话。B(append+查重)库膨胀、C(全量比对)过重 |
| Q5 | 范围外 | Meilisearch / sync / Surreal→E3D 写回(004) / 表结构重设计与多库路由 / ws 运维 / **e3d_io 零改动** | 严格收敛惯例(002 FR-013 同款) |
| Q6 | 产出物 | **精简套件** | spec/plan/research/tasks + contracts/db-contract.md;不写 data-model(表契约即模型)、不写 quickstart |

## 3. 目标态一句话

> **`update_elements_to_database` 是增量进 SurrealDB 的唯一门;门后是确定性 ID 的幂等 upsert 与水位;门外(提取增量)归 002 的读取门面;字节真相仍归 `e3d_io`,003 一行不碰。**
