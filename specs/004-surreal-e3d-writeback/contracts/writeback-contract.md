# Contract: EditOp / 写回语义 / 队列表

**Spec**: [../spec.md](../spec.md) | **Date**: 2026-06-11

> 004 验收依据:E1 编辑操作契约;E2 队列表契约;E3 写回入口语义;E4 红线。违反任一条即视为写回管道失败。实现期变更 MUST 回填并注明日期。

## E1. `EditOp` 契约(强类型,refno 第一寻址)

| 变体 | 载荷(最小集) | 映射目标(001) | 语义要点 |
|---|---|---|---|
| `Rename` | refno, new_name | rename / cow DA 文本 | NAME 哈希条目改写 |
| `SetPos` | refno, pos: [f64; 3] | set_pos | 内联 POS 三元组 |
| `SetInline` | refno, attr_hash: u32, value: InlineValue | set_inline / cow_commit_inline | `InlineValue = Reals(Vec<f64>) \| Ints(Vec<u32>) \| Ref((u32,u32))`(R3:禁裸字节) |
| `SetMembers` | refno, children: Vec<(u32,u32)> | set_members / cow_members_set | 全量替换语义 |
| `Delete` | refno, force: bool | delete + delete_guards | force=false 时 HasMembers/Referenced 必拦 |
| `InsertClone` | template_refno, new_name: Option<String> | insert_clone | 新 refno 自动分配并 MUST 回带报告(R2) |

**不变量**
- E1-I1: 寻址以 refno 为准(无名元素可编辑);name 仅辅助。refno 不存在 ⇒ 该批失败(原子)。
- E1-I2: `EditOp` serde 可序列化 + `schema_version` 字段;队列 payload 与内存类型同源(单一定义)。
- E1-I3: 操作面 = 上表六原语,**不扩**(UDA 条目/DA 任意文本等留 005,FR-010)。
  **specs/005 修订(2026-06-12)**:经 005 契约 F3 批准增补第七原语 `SetName { refno, name }`(无 NAME 新增/已有改写,映射 `set_name_at`);`Rename` 语义不变。serde 向后兼容,schema_version 维持 1。

## E2. `writeback_queue` 表契约

| 字段 | 语义 |
|---|---|
| `id` | 确定性批次键:`{dbnum}_{batch_id}`(batch_id 由提交方给定或内容哈希;同批重复入队=同 id upsert) |
| `dbnum` / `db_path_hint` | 目标库 |
| `edits` | `Vec<EditOp>` serde payload(E1-I2) |
| `status` | `pending → applied \| failed`(状态机;applied 终态不可回退) |
| `applied_sesno` / `new_refnos` / `diff_summary` | 回执(成功时回写) |
| `error` | 失败原因(失败可重试 ⇒ 状态回 pending 由人工/调用方决定,管道不自动重试) |
| `created_at` / `applied_at` | rfc3339 |

**不变量**
- E2-I1: 行构造强类型(003 D2 I4 延续,零拼接)。
- E2-I2: **幂等**:status==applied 的批次重复 apply MUST 跳过且可观测(文件 sesno 不增长)= SC-004。
- E2-I3: 队列层零格式知识(只搬运 EditOp;字节语义全在写回核心之下的 e3d_io)。

## E3. 写回入口语义

```rust
pub fn apply_writeback(
    db_bytes: Vec<u8>,          // 输入文件字节(调用方读取)
    ss: &SchemaSet,
    edits: &[EditOp],
) -> anyhow::Result<(Vec<u8>, WritebackReport)>   // 输出字节(单新会话) + 报告
```

- E3-A1: 一批 = **单新会话**(sesno 仅 +1),`EdbWriter::batch` 原子语义;任一笔失败 ⇒ Err,无输出字节。批内为**顺序语义**(后笔在前笔结果之上);**已知边界(2026-06-11 实测)**:`InsertClone` 的模板若在同批**之前**被编辑过,会触发 e3d_io 克隆的 DA 布局前置拒绝——调用方应把克隆排在其模板编辑之前(队列层 Phase 2 据此排序或拆批)。
- E3-A2: 返回前 MUST `verify_commit` 通过(四类校验);失败 ⇒ Err。
- E3-A3: 纯函数:不触盘、不连库(文件/队列包装在外层)。
- E3-A4: `WritebackReport` MUST 含 new_sesno、逐笔结果、`element_diff` 摘要、InsertClone 新 refno。
- E3-A5: 文件包装层默认写副本(`<db>.e3dout`),原文件零字节变化;inplace 显式 opt-in + 二次确认。
- E3-A6: 回声 = 既定语义(Q4):写回产物经 watcher/ingest 再入库时,幂等 upsert 收敛于写回意图,不建抑制状态。

## E4. 红线(violations = 收敛失败)

- `crates/e3d_io` 改动 MUST 仅限 **2026-06-11 决策 A 批准的寻址扩展**:`EdbWriter` 六个 refno 导向薄变体(`set_inline_at`/`set_pos_at`/`rename_at`/`set_members_at`/`delete_at`/`insert_clone_at`)+ 两个解析助手(`offset_of_refno`/`element_at`),各为既有 name 方法的严格同构(寻址换 refno,后接相同 `cow_*` 路径);格式/事务核心零改动,std-only 红线不变(cargo tree 单节点)。**其余任何 e3d_io 改动仍为红线**(缺口 ⇒ 停下上报)。
- `PdmsIO` C1 冻结面零变更(`tests/api_freeze_c1.rs` 全程不触发)。
- 同一文件并发写回不支持(调用方互斥;契约声明而非实现)。

**决策 A 落地注记(2026-06-11)**:薄变体 + 无名元素 batch 测试已入 e3d_io(39+1 全绿);语义澄清——`rename_at` 与 name 路径同构,要求目标**已有** NAME 条目(改写);无名元素**首次命名**属 DA 新增条目(`cow_da_set_entry` 面),不在 E1 `Rename` 范围内,如需求出现记 005 新 EditOp。
