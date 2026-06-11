# Contract: 链式重组 / SetName / e3d_io 扩展白名单

**Spec**: [../spec.md](../spec.md) | **Date**: 2026-06-11

> 005 验收依据:F1 链式重组不变量;F2 门面接线;F3 SetName 语义;F4 e3d_io 扩展白名单(红线)。

## F1. 链式重组(e3d_io read_view)

- **F1-I1**: 重组流 = 记录窗口字节 + rec[6]/rec[7](DA)与 rec[8]/rec[9](members)链按链序重组的节点 payload,布局对齐 `parse_pdms_db` 现行解析预期。
- **F1-I2**: 邻接布局(PDMS 原生)输入 ⇒ 重组流与原窗口流**逐字节等价**(或解析结果逐项等价,以测试可行者为准)。
- **F1-I3**: 链跟随有界(节点数上限 128,沿 `node_chain_len` 惯例)+ 已访环防;坏链 ⇒ 类型化错误,禁止静默截断。
- **F1-I4**: 双实现对齐:重组流经 `parse_pdms_db` 解出的 DA/显式属性 == 同元素 `decode_full` 结果(sam7200 抽样 ≥200 + 全部 e3d_io 改写元素)。

## F2. 门面接线

- **F2-A1**: 增量路径(`parse_raw_element`/`auto_get_raw_element`/`collect_increment_eles` 所及)消费重组流;公共签名零变更(C1 冻结延续,api_freeze 为闸)。
- **F2-A2**: `cache_hit_rate` 等观测语义不漂移;链页读取计入读视图页计数(SC-006 口径自然延续)。

## F3. `SetName` 语义

- **F3-A1**: `EditOp::SetName { refno, name }`:无 NAME ⇒ DA 新增条目;已有 ⇒ 改写。两路均单会话原子 + verify 强制(004 E3 语义延续)。
- **F3-A2**: `Rename`/`rename_at` 语义不变(改写既有;无名报错)——同构性(004 契约 E4 注)不破坏。
- **F3-A3**: 写回报告/队列回执沿 004 形态,`SetName` 计入 results。

## F4. e3d_io 扩展白名单(红线修订 v2)

004 决策 A 白名单(八方法)基础上,**005 批准新增**:

| 项 | 内容 |
|---|---|
| `Rdb` 链式记录读取面 | DA/members 链跟随 + 重组(具体命名实现期定,如 `element_record_chained`);仅读,不解释属性语义 |
| `EdbWriter::set_name_at` | 内部复用既有 `pack_text`/`cow_da_set_entry`,不新增其它 pub 面 |

- 白名单之外的任何 e3d_io 改动仍为红线(停下上报)。
- std-only 不变(cargo tree 单节点);e3d_io 既有测试全绿为闸。
- 实现落地后回填本表(方法名/提交号)。
