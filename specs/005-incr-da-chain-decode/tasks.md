# Tasks: 增量 DA 链式解析（R5）+ 首次命名

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-06-11

> `[P1]`=US1/US2;`[P2]`=US3;每 Phase 末 GATE。e3d_io 触碰仅限契约 F4 白名单,白名单外停下上报。范围外冲动记 006 候选。

## Phase 0 — Research(已完成)

- [x] T000 R5 机理证据链 + grill Q1~Q6 决策入档 → `research.md`、`contracts/chain-decode-contract.md`

## Phase 1 — e3d_io 扩展(F4 白名单)

- [x] T101 [P1] 先读后写:核对 `parse_pdms_db` 对记录后 DA/members 区的布局假设(邻接形态/终止条件),确定重组流精确形状,回填契约 F1-I1 — **2026-06-11 完成,真实字节裁决**:读通 `parse_raw_ele_data_with_info`/`parse_members_block`/`collect_segmented_payload`(+12 主载荷/+24 追加段/0x07 marker)与 e3d_io `list_payload_words`/`decode_da_list`(+20 payload/5 词头)两套假设的表面冲突后,以 sam7200 实测字节裁决:**邻接布局 = 同一 5 词节点形状的物理邻接摆放**(rec[8]/[9] 节点 == 窗口 membs_pos,逐词核对)。重组规范定稿入契约 F1-I1;锚点测试 `members_node_layout_anchor` 固化结论(e3d_io 40+1 全绿)
- [x] T102 [P1] `Rdb` 链式记录重组(F1-I1~I3:DA rec[6]/[7] + members rec[8]/[9] 链跟随,有界 128 + 环防,坏链类型化错误)+ 测试:邻接库等价(F1-I2)/ e3d_io 改写元素重组含远页 payload / 坏链报错 — **2026-06-11 落地**:`Rdb::element_record_chained`(隐式区 ++ adjacentize(members) ++ adjacentize(DA),节点 5 词头+payload 原样字节、节点间 0x07 分隔字,按 F1-I1 规范)。测试三类:① 原生库 50 元素抽查重组节点区与磁盘节点逐字节一致 ② **R5 核心证明**:改名重定位后窗口流不含新名字节、链式流含(盲区复现+修复双断言)③ 合成坏链(节点类型错位)类型化报错。e3d_io **43+1 全绿**
- [x] T103 [P2] `EdbWriter::set_name_at`(F3-A1:无名新增/已名改写,内部复用 pack_text+cow_da_set_entry)+ 测试:无名命名 batch 单会话 round-trip / 已名改写与 rename_at 殊途同归 / rename_at 同构语义不变 — **2026-06-11 落地**:实现为既有私有机件组装(`pack_text`+`set_entry_in_payload`+`relocate_da_payload`),**支持 DA 区从无到有的首链创建**;`cow_da_set_entry` 的空区拒绝语义未动(比契约预想更干净,零 pub 行为变化)。测试:DA 全空无名元素首次命名(与已名改写同批单会话)/ 读回 / rename_at 对无名仍拒 / 首次命名后可被 name 路径寻址
- [x] T104 GATE:e3d_io 独立套件全绿;cargo tree 单节点;diff 仅白名单项 — **2026-06-11 通过**:**44+1 全绿**;cargo tree 单节点;白名单审计=`element_record_chained`/`adjacentize`(T102)+`set_name_at`(T103)+锚点/链式/命名测试,无其它触碰。**Phase 1 收口,Phase 2(门面接线)解锁**

## Phase 2 — 门面接线(R5 修复本体)

- [x] T201 [P1] 增量路径换链式重组流(`parse_raw_element` 一族;公共签名零变更,C1/api_freeze 为闸)— **2026-06-12 落地**:`read_element_record_cached` 改经 `element_record_chained`(前导 0/7 padding 先跳,链式流以 w0 为锚)。**附带两项同根修正(对齐契约逮出的 v1 长期隐性 bug)**:① `EleData.name` 提取挪到显式解析之后(显式优先,implicit 兜底)——原先在显式解析**之前**取 implicit "NAME" ⇒ 增量管道 name 恒空 ② `MEMBERS_BASE_PAYLOAD_OFFSET` 12→20——主段载荷把 w3/w4 链指针计入 ⇒ 单节点混入 (0,0) 伪成员、多节点链混入 `(next_pg,next_loc)` 伪成员(sam7200 (23584,5443) 实测);合成夹具/count_members 同步修正
- [x] T202 [P1] 双实现对齐测试(F1-I4):sam7200 抽样 ≥200 + e3d_io 改写元素,重组流 parse == `decode_full`(DA/显式逐项)— `tests/chained_incr_sam7200.rs`:① 具名 150+无名 60 抽样,门面(链式流)refno/name/children 与 e3d_io 真相逐项一致 ② **R5 门面级终证**:writeback 改名后门面解析读出新名。**对齐又逮出第三个隐性 bug**:链尾节点带 **slack 词**(节点声明长度 > rec[10] 预算余量,(23584,5535) 实测)⇒ v1 合并载荷 %8 失败 ⇒ children 静默为空——修复=重组器预算导向(payload 取 min(声明,预算),头 low16 收紧;e3d_io 解码同语义),契约 F1-I1 已回填。三测全绿
- [x] T203 GATE:默认特性全套件绿(含 `diag_ams1112` 5 测试不回归)— **2026-06-12 00:34 通过,exit 0**:lib 47/0/5 + 全部集成绿;`diag_ams1112_full_parse` 5✓(342s,103MB 全库跑在链式流+三项修正上零回归);api_freeze 未触发。**Phase 2 收口,Phase 3(写回侧收口)解锁**

## Phase 3 — 写回侧收口

- [ ] T301 [P2] `EditOp::SetName` + 写回核心/队列/CLI 沿 004 形态接入
- [ ] T302 [P1] 回声转正:Rename(及 SetName)回声进库测试——增量含 Modified、`pe.name` 收敛;**解除 004 回声测试的 R5 限定注记**(SC-001/SC-004)
- [ ] T303 GATE:`--features surrealdb` workspace 全量绿;api_freeze 未触发

## Phase 4 — 文书

- [ ] T401 ARCHITECTURE R5 注记更新 + CHANGELOG;004 research R5 回填"已修复(005)"
- [ ] T402 GATE:SC-001~SC-005 逐条核销;spec Status → Implemented

## 依赖关系

```
T101→T102→T104(GATE);T103→T104
T104→T201→T202→T203(GATE)
T203→T301/T302→T303(GATE)→T401→T402(GATE)
```

## 范围外提醒(FR-008)

store_all 历史回填强类型化、UDA 编辑面、真机联动(001-T039)、并发、白名单外 e3d_io 改动、C1 变更——**本清单不含以上任何项**。
