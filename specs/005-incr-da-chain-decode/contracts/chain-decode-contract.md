# Contract: 链式重组 / SetName / e3d_io 扩展白名单

**Spec**: [../spec.md](../spec.md) | **Date**: 2026-06-11

> 005 验收依据:F1 链式重组不变量;F2 门面接线;F3 SetName 语义;F4 e3d_io 扩展白名单(红线)。

## F1. 链式重组(e3d_io read_view)

- **F1-I1**: 重组流 = 记录窗口字节 + rec[6]/rec[7](DA)与 rec[8]/rec[9](members)链按链序重组的节点 payload,布局对齐 `parse_pdms_db` 现行解析预期。
  
  **T101 字节级裁决回填(2026-06-11,锚点测试 `members_node_layout_anchor` 固化)**:
  - 链节点统一形状 = **5 词头** `[(which<<16)|total_words][refno0][refno1][next_pg][next_loc]` + payload(自 node+20);which=1 DA/显式、which=2 members;total_words 含头。
  - **PDMS 原生(邻接)布局 = 同一节点形状的物理邻接摆放**:rec[8]/[9] 所指 members 节点恰好紧随记录隐式区(+0/7 padding);多节点间以 `0x00000007` 分隔字相连(v1 的"追加段"= [07 marker][下一个 5 词节点],SEGMENT_PAYLOAD_OFFSET=24=4+20 为证);explicit 块的 `[00 01][len]` 头 = which=1 节点 w0 的高/低半词。
  - **v1 解析器语义注记**:主段载荷取 node+12 起(把 w3/w4 链指针计入载荷;邻接单节点时为 (0,0)),追加段载荷取 +24(正确跳过节点头)——此为既有 oracle 行为,005 重组**不纠正**它,只还原邻接形态。
  - **重组规范**:`[记录窗口 bo..impl_end(+padding)] ++ adjacentize(members 链) ++ adjacentize(DA 链)`,其中 `adjacentize(chain) = node0(5 词头+payload 原样字节) ++ ([0x00000007] ++ node_k(原样))*`——节点字节一律原样(含 w3/w4 原值),与 PDMS 邻接写法 byte-faithful;链 words 预算分别取 w10 高/低 14 位(da=(w10>>14)&0x3FFF,memb=w10&0x3FFF)。
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
| `EdbWriter::set_name_at` | 内部复用既有 `pack_text`/`cow_da_set_entry`,不新增其它 pub 面 — **落地回填(2026-06-11)**:实际为私有机件组装(`pack_text`+`set_entry_in_payload`+`relocate_da_payload`),支持 DA 首链创建;`cow_da_set_entry` 公共语义零变化(优于预想) |

- 白名单之外的任何 e3d_io 改动仍为红线(停下上报)。
- std-only 不变(cargo tree 单节点);e3d_io 既有测试全绿为闸。
- 实现落地后回填本表(方法名/提交号)。
