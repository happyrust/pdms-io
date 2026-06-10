# 开发方案:E3D 离线编辑 — 事务化批量应用 + 安全护栏 + dry-run/diff

> 计划 ID:`2026-06-07-e3d-offline-edit-safety-batch`
> 创建:2026-06-07  状态:**proposed**(待评审/批准;**主题待确认** —— 若你想要的"开发方案"是别的主题,请指出,我改提)
> 前序:`2026-06-05-e3d-db-offline-attr-parser`(读写格式分析,完成)+ `2026-06-07-e3d-offline-rw-productionization`(读写库 `crates/e3d_io` + API + CLI + 去重,基本完成;Phase 1 真机 / Phase 5 / 整 crate 集成受外部资源阻塞)

## 背景 / 动机

`crates/e3d_io` 已提供**离线写原语**:`rename` / `set_pos` / `set_inline` / `set_members` / `delete` / `insert_clone`,每个都做一次 **COW + 新会话提交**(`EdbWriter` API)。但要让离线编辑**生产可用、可安全用于真实工程数据**,还缺"应用/安全层":

1. **无事务化批量** —— 当前 N 个编辑 = N 个新会话(每次 COW + append session)。真实"保存"应是**把若干待定编辑合成一个新会话**(原子、单 sesno 跳变),而非每改一笔一会话。
2. **无写后自校验** —— 提交后未自动核验:B 树不变式(`nav_ok`/balanced/sorted)、其余元素字节不变(COW 不可变)、目标元素读回符合预期、未引入悬挂引用。
3. **无 dry-run / diff** —— 无法在"不落盘"前提下预览一批编辑会改什么(元素级 增/删/改 属性 diff)。
4. **无安全护栏** —— 未阻止危险编辑(删有子元素的父、制造悬挂引用、改键破坏 B 树),无 force 开关分级。

## 目标(Goal)

在 `crates/e3d_io` 写原语之上,做一个**安全的事务化离线 E3D 编辑层**:把多笔编辑合成单会话原子提交、每次提交自动自校验、支持 dry-run + 元素级 diff、带安全护栏。

**完成判据(可验证)**:
- (a) 把 **≥3 笔混合编辑**(如 rename + set_pos + insert)作为**一个新会话**提交(`sesno` 仅 +1),三者均可读回、前会话不变;
- (b) `verify_commit()` 能**捕获**一次故意构造的"坏提交"(如破坏 B 树 / 改动了无关元素字节);
- (c) `dry_run(edits)` 在**不写盘**下输出元素级 diff(改了哪些属性 / 新增 / 删除),与真实提交后的实际变化一致。

## 现状(作为输入,详见 productionization 计划 + `crates/e3d_io`)

- 写原语 + `EdbWriter`(name 导向)+ 类型化 `E3dError` + `e3d-io` CLI + 校验器 `btree_check`(测试内)均已就位、20 测试通过。
- COW 提交核心 `commit_edited_data_page`(单数据页)/ B 树插入分裂 / DA·成员·UDA relocation 均已实现。
- 多版本:每提交 append 新会话、重指 page0;旧会话只读保留。

## 阶段(Phases)

### Phase 1 — 写后自校验 `verify_commit`（doable）  状态:proposed
- [ ] `verify_commit(orig_bytes, edited: &Edb, expect) -> Result<(), Vec<Issue>>`:① B 树不变式(把测试内 `btree_check` 的 `nav_ok`/balanced/sorted/无重复**提为库内非 test API**)② COW 不可变(原页字节仅 page0 会话指针变)③ 目标元素读回 == 预期 ④ 引用完整性(新增/改动的引用 `(dbno,refseq)` 在库内或已声明的 catalogue 内可解,不悬挂)。
- 完成判据:对正常提交通过;对故意坏提交报出对应 Issue。

### Phase 2 — 事务化批量提交（doable,核心)  状态:proposed
- [ ] `Transaction`/`EdbWriter::batch`:累积多笔编辑(rename/set/insert/delete/members/uda),**一次** COW + **单**新会话提交。关键:多笔编辑可能触及**多个数据页/多条 B 树路径**,需把各路径 COW 合并到**同一新根**(逐笔在"当前工作根"上 COW,串联到一个 session)。
- [ ] 语义:同一元素多次编辑合并;插入后再编辑;删除与其它编辑组合的顺序规则。
- 完成判据:3+ 混合编辑 → `sesno` 仅 +1,全部生效,前会话不变,`verify_commit` 通过。

### Phase 3 — dry-run + 元素级 diff（doable)  状态:proposed
- [ ] `dry_run(edits) -> Diff`:在副本上应用批量但**不写盘**,对比 old/new 解码元素,产出 `{added, removed, modified:[{refno, name, attr, old, new}]}`。
- [ ] CLI:`e3d-io plan <edits-file>`(显示 diff)/ `e3d-io apply <edits-file>`(批量提交 + verify)。
- 完成判据:dry-run diff 与真实提交后的解码差异一致。

### Phase 4 — 安全护栏 / 策略（doable)  状态:proposed
- [ ] 默认拒绝:删除仍有成员的父元素(除非级联)、制造悬挂引用、重复 refno;`--force` 分级放行。
- [ ] 仅副本默认;`--inplace` 需二次确认 + 自动 `verify_commit` 通过才落盘。

### Phase 5 — 真机验证 + aios 数据流接入（gated）  状态:proposed
- [ ] 真 running-E3D round-trip 验证批量提交(**需用户 E3D**,阻塞)。
- [ ] 把批量编辑接入 aios/surrealdb 数据流(写回 E3D)(**待整 crate 集成解阻**,项目外)。

## 决策记录(Decisions)
- 全部建立在 `crates/e3d_io` 之上,保持 std-only。
- 批量 = **单会话**(对齐真实 PDMS "save work" 语义)。
- 写默认仅副本;落盘前强制 `verify_commit`。
- `btree_check` 等校验从 test-only 提升为库内 API(供 verify 复用)。

## 风险与回退
| 风险 | 影响 | 回退 |
|---|---|---|
| 多笔编辑合一会话的 B 树多路径 COW 合并 | Phase 2 核心难点 | 先支持"互不相交记录"的批量;相交(同页多记录)单独处理 |
| 引用完整性检查在大库代价 | 性能 | 仅校验本批改动涉及的引用,非全库 |
| 真机/集成 | 阻塞 | Phase 1–4 全 E3D-无关、可独立完成验证;Phase 5 gated |

## 备注
- 执行起点(若批准):**Phase 1**(verify_commit)→ **Phase 2**(事务批量),均 E3D-无关、可立即做并验证。
- 若此主题非你所想的"开发方案",请指明目标(如:E3D→DB 接入、模型 diff/merge、其它子系统),我据此改提。
