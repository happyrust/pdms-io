# 历史数据存储方案（REFNO + SESNO 架构）

## 1. 背景与现状

1. **操作抽象已就绪**：`EleOperationData`/`EleOperationDetail` 已能把 refno 在指定 sesno 的状态转换成 SurrealQL，覆盖新增/修改/删除，用于主数据最新态写入。@src/io.rs#374-501
2. **批量写入管线具备基础**：当前 `save_to_surreal` 会将每个 sesno 的元素变更按批次写入 `element_changes` 与 Surreal 主表，为历史审计提供原始数据。@src/io.rs#933-1033
3. **历史扫描能力**：`store_all_refno_sesno_map` 已能遍历 PDMS 会话页，构建 `refno -> {(offset, sesno)}`，并试图写入 `ses`、`pe_ses_h` 等表，但尚未形成稳定的版本化存储与查询接口。@src/io.rs#2647-2773

> **痛点**：缺少正式的版本化数据模型；`c_pe` 与历史快照未强绑定；增量与幂等策略不足，难以支撑常态化历史存储。

## 2. 建设目标

- 以 `[REFNO, SESNO]` 作为历史快照主键，保留每次会话的完整属性、层级与几何关联。
- 维护 `c_pe`（current pe）指向最新 sesno，保证读写一致性。
- 覆盖全量重建 + 增量同步，具备断点续跑能力。
- 提供按 refno、时间、属性等维度的历史查询，并支持 diff 检索与全文搜索。

## 3. 数据模型设计

### 3.1 表/记录设计

| 名称 | 作用 | 关键字段 | 说明 |
| --- | --- | --- | --- |
| `pe_history` | 历史快照，ID = `pe:[refno, sesno]` | `refno`, `sesno`, `noun`, `attmap`, `owner`, `children`, `ses_timestamp`, `dbnum`, `is_deleted` | 每条记录代表 refno 在 sesno 的完整态。 |
| `c_pe` | 最新映射 | `id: refno`, `latest_sesno`, `latest_pe`, `updated_at`, `checksum` | `latest_pe` 指向 `pe_history`，`checksum` 用于幂等校验。 |
| `pe_diff`（复用 `element_changes`） | 语义化 diff | `id:[refno, sesno]`, `operation_type`, `entity_type`, `details`, `ses_id`, `ses_timestamp` | 与 UI/搜索联动，记录 patch。 |
| `ses` | 会话主表 | `id:[dbnum, sesno]`, `timestamp`, `range`, `latest_pgno` | 供时间过滤。 |
| `attr_kv_history`（可选） | 热点属性索引 | `id:[refno, attr, sesno]`, `value`, `value_hash`, `is_geometry_related` | 解决特定属性回溯性能。 |

### 3.2 SurrealQL DDL（示例）

```sql
DEFINE TABLE pe_history SCHEMALESS;
DEFINE INDEX idx_pe_history_refno ON TABLE pe_history COLUMNS refno, sesno DESC;
DEFINE INDEX idx_pe_history_owner ON TABLE pe_history COLUMNS owner;

DEFINE TABLE c_pe SCHEMALESS;
DEFINE INDEX idx_c_pe_latest ON TABLE c_pe COLUMNS latest_sesno DESC;

DEFINE TABLE pe_diff SCHEMALESS;
DEFINE INDEX idx_pe_diff_type ON TABLE pe_diff COLUMNS operation_type, entity_type;
DEFINE INDEX idx_pe_diff_time ON TABLE pe_diff COLUMNS ses_timestamp;

DEFINE TABLE attr_kv_history SCHEMALESS;
DEFINE INDEX idx_attr_kv_ref_attr ON TABLE attr_kv_history COLUMNS refno, attr, sesno DESC;
```

> 实际 DDL 可结合 `DEFINE FIELD` / 权限策略细化，确保 `id` 统一为 `[refno, sesno]`。

## 4. 数据流与同步策略

### 4.1 全量重建流程

1. **扫描会话**：沿用 `store_all_refno_sesno_map`，获取 `refno -> [(offset, sesno)]`。@src/io.rs#2647-2773
2. **解析元素**：对每个 offset 调 `parse_raw_element/auto_get_raw_element` 生成 `EleData`。
3. **写入历史表**：将 `EleData` 转成 JSON，`INSERT IGNORE INTO pe_history [...]`，batch size 100，附带 `ses_timestamp`、`dbnum`。
4. **写 diff**：根据 `EleOperationDetail` 产出的 patch 构建 `pe_diff` 记录，沿用现有 `element_changes` 插入逻辑。@src/io.rs#933-993
5. **更新 c_pe**：当处理到 refno 最新 sesno 时，`UPSERT c_pe`，带上 `latest_pe = type:[refno, sesno]` 与 `checksum`（例如 `xxhash(attmap)`）。
6. **校验**：对比 `c_pe.latest_sesno` 与 `pe_history` 最大 sesno，输出报告。

### 4.2 增量同步

1. 记录 `last_synced_sesno`（持久化在 `sync_metadata` 表或本地文件）。
2. 每次从 `get_latest_sesno` 读取当前最大值 @src/io.rs#1134-1147，与 `last_synced` 做差。
3. 仅遍历新增 ses 范围，复用步骤 2-5，写入历史/最新映射。
4. 成功后更新 `last_synced_sesno`，并将批次信息写入监控日志。
5. 若中断，可通过 `last_synced` 实现幂等重放。

### 4.3 c_pe 维护要点

- `UPSERT c_pe SET latest_sesno = $sesno, latest_pe = pe_history:$id, updated_at = time::now(), checksum = $hash`
- 引入乐观锁：`WHERE latest_sesno <= $sesno`，防止乱序写覆盖。
- 定期审计任务：`SELECT refno FROM c_pe WHERE latest_sesno < (SELECT max(sesno) FROM pe_history WHERE refno = c_pe.refno)` 用于发现缺失。

## 5. 查询与索引

| 场景 | 示例语句 | 说明 |
| --- | --- | --- |
| 按 refno 回溯 | `SELECT * FROM pe_history WHERE refno = $ref ORDER BY sesno DESC LIMIT 50;` | 利用 `idx_pe_history_refno`。 |
| 按时间窗口 | `SELECT * FROM pe_history WHERE ses_timestamp BETWEEN $t1 AND $t2` | 借 `ses` 表将时间转 sesno，再过滤。 |
| 最新态读取 | 1) `SELECT latest_pe FROM c_pe WHERE id = $ref` 2) `SELECT * FROM type::thing(latest_pe)` | 避免全表扫描。 |
| 属性 diff 检索 | `SELECT * FROM pe_diff WHERE details CONTAINS '"NAME"' LIMIT 100;` | 可同步索引至 Meilisearch（`attributes_text`）。 |
| 属性历史（可选表） | `SELECT value FROM attr_kv_history WHERE refno=$ref AND attr='NAME' ORDER BY sesno;` | 热点属性快速响应。 |

## 6. 实施路线

1. **Schema & 文档**：固化 DDL、字段含义、索引策略（当前文档即蓝图）。
2. **全量历史落地**：完善 `store_all_refno_sesno_map` 输出通路，加入批写 `pe_history`、`pe_diff`、`c_pe`，并提供 CLI 入口（如 `cargo run --bin sync_history`).
3. **增量与监控**：实现 `last_synced` 记录、断点续跑、告警日志，输出统计（耗时、记录数）。
4. **查询接口**：新增 `history_query` 工具或 API，封装常用查询；对 `pe_diff` 建全文索引。
5. **可选优化**：引入 `attr_kv_history`、压缩存储（如只存差异字段）、Cold Storage（导出 JSON/LTSV）。

## 6.1 实用运行指引（CLI）

当前仓库提供了一个用于历史数据全量同步的示例 CLI：`src/bin/sync_history.rs`。

### 6.1.1 启动前准备

- 确保本地 `DbOption.toml` 已配置好 SurrealDB 连接信息（与 `demo_latest_data_save` 一致）。
- SurrealDB 服务已启动，并且具备写入 `pe_history`、`c_pe` 等表的权限。

### 6.1.2 全量同步命令示例

在 `pdms-io` 仓库根目录执行：

```bash
cargo run --bin sync_history -- \
  "D:\\AVEVA\\Projects\\E3D2.1\\AvevaMarineSample\\ams000\\ams8000_0001"
```

说明：

- 第一个 `--` 之后的参数为 PDMS 数据库路径。
- 如不传参数，程序会使用内置的示例路径（适合本地 demo）。
- 程序会：
  - 初始化日志与 SurrealDB 连接；
  - 读取 PDMS 文件，构建 `refno -> (offset, sesno)` 索引；
  - 解析所有历史版本并写入 `pe_history`；
  - 统计每个 `refno` 的最新 `sesno`，更新 `c_pe` 最新映射。

> 当前版本 CLI 实现的是 **全量同步**，增量/断点续跑将在后续版本中通过额外参数（如 `--start-ses/--end-ses/--resume-from`）扩展。

## 7. 风险与 TODO

- **性能**：全量扫描 I/O 密集，需分批/限速，必要时多线程解析 + 异步写库。
- **空间**：`pe_history` 体积与 sesno 成正比，需评估压缩或生命周期策略。
- **一致性**：必须保证 `pe_history` 写成功后再更新 `c_pe`，否则最新指针会指向不存在版本。
- **错误恢复**：为每批写入记录 checksum/hash，方便回溯与重跑。
- **安全**：SurrealDB 权限需要限制仅允许内部服务写入这些历史表。
