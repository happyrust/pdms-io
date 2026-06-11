# Research: R5 机理与修复决策记录

**Date**: 2026-06-11 | **Spec**: [spec.md](./spec.md)

## 1. R5 机理(004-T301 实测,证据链)

1. **写侧**:e3d_io `cow_commit_da_text`(改名等)经 `relocate_da_payload` 把 DA 列表搬到**文件尾新页**,记录 rec[6]/rec[7] 指针指向远页——格式合法(001 链式解码 `decode_da_list`/`chain` 读回正常,写回核心读回核验通过)。
2. **读侧**:增量路径 `parse_raw_element` → `Rdb::element_record`(002-T205 对 v1 `ElementRecordReader` 的 1:1 移植)= **窗口邻接读取**(自记录偏移连续读 16~64K);`parse_pdms_db::parse_raw_ele_data` 在窗口字节流中解析"紧随记录的邻接 DA 区"。
3. **后果**:远页 DA 不在窗口 ⇒ 解析出的 `EleData` 无新名/无 DA 变化 ⇒ 增量对比定性"无变化" ⇒ 改名既不进增量也不落库(004 实测:批内 Rename 元素整体从增量消失)。
4. **边界**:PDMS 原生写的库 DA 与记录邻接 ⇒ 历史路径一直正确;只有 e3d_io 写过 DA 的元素踩中。members 链(rec[8]/rec[9])同构同险。

## 2. grill 决策记录(2026-06-11,全按推荐)

| # | 问题 | 决策 | 要点 |
|---|---|---|---|
| Q1 | 范围 | **A. R5 主线 + 首次命名次线** | 共享 DA 条目机制;store_all 留 006 |
| Q2 | 修复层位 | **A. e3d_io read_view 扩展** | DA 链语义=格式真相,归 e3d_io 单源;否 B(门面伪相邻拼接,脆)否 C(parse_pdms_db 直连页源,违 002 适配层定位) |
| Q3 | EleData 属性来源 | **B. 仅 DA/members 区改链式,隐式区窗口不动** | 最小翻动;隐式窗口解析多年验证 |
| Q4 | 回归基线 | 回声 Rename 转正 + diag_ams1112 不回归 + 双套件全绿 | C3.1 oracle 纪律 |
| Q5 | 首次命名 | **B. 独立 `SetName`** | 显式优于隐式;`Rename` 同构语义(004 已钉)不破坏 |
| Q6 | 范围外 | store_all/UDA 面/真机/并发 全排除 | 留 006+ |

## 3. 设计要点(实现期细化,契约 F 系列锁不变量)

- **重组形态**:e3d_io 产出「记录窗口 + 链序重组的 DA/members payload」,布局对齐 `parse_pdms_db` 现行预期——伪相邻由**格式真相方**(e3d_io)重组并以双实现对齐测试背书,与 Q2-B 的门面盲拼有本质区别。
- **门面接线**:增量路径换链式重组;`read_element_record_cached` 公共签名不变(C1)。
- **set_name_at**:内部复用 `pack_text` + `cow_da_set_entry`(均既有),不新增裸 pub;白名单仅 `set_name_at` 本体。
- **风险**:① 重组布局与 parse_pdms_db 预期错位 ⇒ 以 FR-003 邻接库等价 + FR-004 对齐测试双闸 ② 链异常 ⇒ FR-005 有界环防 ③ e3d_io 扩展蔓延 ⇒ 契约 F4 白名单审计。
