# Contract: 落库表 / 记录 ID / 幂等 / 入口语义

**Spec**: [../spec.md](../spec.md) | **Date**: 2026-06-11

> 本契约是 003 的验收依据:D1 表契约;D2 记录 ID 与幂等;D3 入口 API 语义;D4 存量路径处置。违反任一条即视为落库收敛失败。实现期若有必要变更,MUST 回填本文并注明日期。

## D1. 表契约(沿存量形态,非破坏性)

| 表 | 键 | 字段(最小集) | 写语义 |
|---|---|---|---|
| `ses` | `id` = 会话标识(含 dbnum/sesno) | sesno、dbnum、日期时间(rfc3339)、end_pgno | upsert(同 id 覆盖同值) |
| `pe_ses_h` | `id` = `[dbnum, refno, sesno]` | refno、sesno、offset、dbnum、ses(指向 `ses`) | upsert |
| `pe` | `id` = `[dbnum, refno]`(主数据,最新态) | 元素属性 JSON(serde 强类型序列化) + `VERSION <会话时间>` | 版本化 upsert(`skip_main_data=true` 时不写) |
| `ingest_watermark`(新) | `id` = `dbnum` | high_sesno(已落最高会话)、updated_at | 单调递增 upsert(只升不降) |

- 存量库兼容:既有 `INSERT IGNORE` 写入的旧记录,id 形态 MUST 兼容或经一次性核对说明(实现期回填)。
- 禁止:删除/重命名存量表与字段;`SECOND_SUL_DB`/`KV_DB` 路由(范围外)。

**实现期回填(2026-06-11,T201)**:
1. 记录 ID 取**字符串复合键**(`ses: {dbnum}_{sesno}`、`pe_ses_h: {dbnum}_{r0}_{r1}_{sesno}`、`pe: {dbnum}_{r0}_{r1}`、`ingest_watermark: {dbnum}`)——数组键在 3.x SDK 的 `RecordIdKey` 类型面更绕,确定性等同;
2. `pe_ses_h` 以 `op`(add/modify/delete/none)字段取代 offset——增量载荷 `EleOperationData` 不含物理 offset(offset 属 `store_all_refno_sesno_map` 历史回填路径的字段);
3. `pe` 为**最新态 upsert**(Deleted ⇒ `deleted: true` 墓碑行);`VERSION` 版本化留给历史回填入口(T302 盘点时一并决策);
4. 物理写为**逐记录 upsert**(幂等优先;3.x SDK 无批量 upsert content 原语)——D3 A1 的"分块"体现为处理批次粒度。

## D2. 记录 ID 与幂等判据

- **I1**: 全部记录 ID 由 `(dbnum, refno, sesno)`(主数据为 `(dbnum, refno)`)**确定性**生成——同一输入任意次重放产生同一 ID 集。
- **I2**: 重放幂等:同一增量落库 N≥3 次,逐表 `count()` 与逐记录内容与 1 次完全一致(SC-002 oracle)。
- **I3**: 水位单调:`ingest_watermark.high_sesno` 只升不降;入口对 `sesno ≤ 水位` 的会话 MUST 跳过且暴露跳过计数(SC-003 oracle)。
- **I4**: 新语句 MUST 强类型/参数绑定(surrealdb-types/serde),禁止格式化字符串拼接 SQL(`to_surql` 教训,research §1.4)。

## D3. 入口 API 语义(签名冻结,002 C1 延续)

```rust
pub async fn update_elements_to_database(
    &mut self,
    range_eles: &BTreeMap<u32, Vec<EleOperationData>>,  // sesno → 该会话增量
    skip_main_data: bool,                               // true ⇒ 不写 pe 主数据
) -> anyhow::Result<()>
```

- **A1**: 按 sesno 升序处理;每会话:写 `ses` → 批写 `pe_ses_h`(分块,默认 100/块)→(除非 skip)批写 `pe` VERSION → 升水位。
- **A2**: 块失败 ⇒ 上抛错误,不静默;已写部分依赖 I2 幂等可重放补齐(不要求跨会话原子)。
- **A3**: 连接未初始化(`SUL_DB` 未 connect)⇒ 明确报错,不隐式连接生产地址。
- **A4**: 入口不做增量提取(那是读取门面的职责)——只消费 `EleOperationData`。

## D4. 存量路径处置(Phase 3 验收)

| 路径 | 处置 |
|---|---|
| `to_surql`(空串占位) | 删除(强类型序列化取代);`EleOperationData` 上不留字符串拼接面 |
| `collect_and_save_latest_data` 的保存段 | 改为委托 `update_elements_to_database`(收集段保留) |
| `store_all_refno_sesno_map` | 盘点决策(实现期回填):能力并入新入口 ⇒ 退役;或保留为"全库历史回填"专用入口并在此声明分工 |
| `sync_history` 等成片注释坟场 | 删除(git 历史可查) |

**验收**:落库语句构造点 grep 仅命中新入口模块(+D4 声明保留项);kv-mem 全套测试绿;`crates/e3d_io` diff 为空。
