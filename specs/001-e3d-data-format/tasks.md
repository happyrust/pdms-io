---
description: "Task list for E3D / PDMS DABACON 数据格式离线读写规范"
---

# Tasks: E3D / PDMS DABACON 数据格式离线读写规范

**Input**: Design documents from `specs/001-e3d-data-format/`

**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅

**Tests**: 本项目宪法(III/V)要求测试;测试任务为必需(非可选)。

**Organization**: 按 user story 分组。**状态图例**:`[X]` 已完成并验证 · `[ ]` 待办 · `[~]` gated(外部资源阻塞)。

> 说明:US1–US4 的能力**已在前序计划落地并验证**(故多为 `[X]`,引用真实文件/测试名作为证据);新增价值集中在 **Phase 7 / US5**(事务·安全编辑层,FR-019..022 / SC-008..010,= active plan `2026-06-07-e3d-offline-edit-safety-batch`)与 Phase 8(gated)。

## Phase 1: Setup(共享基础)

- [X] T001 spec-kit 脚手架初始化(`.specify/`、`.cursor/skills`、`specs/`)— Cursor + PowerShell
- [X] T002 项目宪法 `.specify/memory/constitution.md`(v1.0.0,5 原则)
- [X] T003 std-only 隔离 crate `crates/e3d_io/Cargo.toml`(edition 2024,无第三方依赖)

---

## Phase 2: Foundational(阻塞性前置)

**⚠️ 所有 user story 依赖本阶段**

- [X] T004 头部 / db-control-block 解析(page0 字段厘定)— `docs/.../E3D_DB_文件格式规范.md §2`,`src/defines.rs` 注释校正
- [X] T005 [P] 模式库 `*vir.dat` 装载 + tlu 二分 + K/I/J skeleton 解析 — `crates/e3d_io/src/lib.rs`(`SchemaSet`)
- [X] T006 [P] 会话链 + B 树遍历(`word6` 界定,上界 4M,有效性过滤)— `crates/e3d_io/src/lib.rs`(`walk`/`index_db`)
- [X] T007 base-27 `db1_hash` / `DEHASH`(含 UDA 有损短码)— `crates/e3d_io/src/lib.rs`(`db1_dehash`/`dehash_uda_code`)

**Checkpoint**: 基础就绪,可枚举 refno→记录偏移。

---

## Phase 3: User Story 1 — 离线读取元素全部属性 (P1) 🎯 MVP

- [X] T008 [US1] 隐式属性解码(type 枚举 + sel + 标量/计数前缀 + Bool 位 + 实数低字在前)— `lib.rs`(`decode_at`/`_decode_one`)
- [X] T009 [US1] DA/显式区解析(节点链 + 条目 + 文本 packing + NAME)— `lib.rs`(`decode_da_list`)
- [X] T010 [US1] owner(头 w4–5)+ 引用 `(dbno,refseq)` 提取 — `lib.rs`
- [X] T011 [US1] UDA 强类型值 + `0xFFF` 派生表达式解码 — `uda_probe.py` / `uda_expr_probe.py`(98.8%)
- [X] T012 [US1] UTF-8 文本(修复 CJK latin1 bug)— `e3d_attr_decoder.py` / `lib.rs`
- [X] T013 [P] [US1] 回归测试:WELD `/WB1` POS/ORI 锚点 — `lib.rs` 测试 `read_counts_and_weld_pos`(SC-001)

**Checkpoint**: US1 可独立验证(单元素全属性离线解出)。

---

## Phase 4: User Story 2 — 离线非破坏写入(CRUD) (P2)

- [X] T014 [US2] COW 提交核心(改后数据页 → 自包含修正 → B 树路径 COW → 新会话 → 重指 page0)— `lib.rs`(`commit_edited_data_page`/`append_session`)
- [X] T015 [US2] S1 改内联值 — `cow_commit_inline`
- [X] T016 [US2] S2/S6 变长 DA 文本(同页 / 跨页·链式·增长重定位)— `cow_commit_da_text`
- [X] T017 [US2] S3/S5 新增元素(最大键 / 任意键 + 节点分裂 + 根长高)— `cow_insert_element`/`cow_insert_element_split`
- [X] T018 [US2] S4 删除元素(叶压实)— `cow_delete_element`
- [X] T019 [US2] S7 成员列表(type-2 链重定位/增删)— `cow_members_set`
- [X] T020 [US2] S8 UDA·DA 条目改/增/删 — `cow_da_set_entry`/`cow_da_remove_entry`
- [X] T021 [US2] 稳定 API `EdbWriter` + 类型化 `E3dError` + `e3d-io` CLI(默认副本,--inplace)
- [X] T022 [P] [US2] 回归测试:CRUD 多版本 + 字节 diff 仅 page0 + B 树 nav_ok — `cow_*` 测试(SC-004)

**Checkpoint**: US2 全 CRUD 离线可用,多版本可回溯。

---

## Phase 5: User Story 3 — 整库导出与模型重建 (P3)

- [X] T023 [US3] 整库枚举导出 JSON(refno/noun/name/owner/implicit/explicit/refs)— `e3d_export.py` / `tools/e3d_decode_rs`
- [X] T024 [US3] 跨库引用解析(refmap,`--cat`)— `resolve_refs`
- [X] T025 [US3] owner 链层级树重建 — `e3d_tree.py`

**Checkpoint**: US3 整库规模化导出 + 树重建。

---

## Phase 6: User Story 4 — 双实现对齐与可复用 (P3)

- [X] T026 [US4] Rust std-only crate 单一真源 + `tools/e3d_decode_rs` 去重(消费 e3d_io)
- [X] T027 [US4] `pdms_io` 经 `pub use e3d_io as e3d_decode` 接线(path 依赖)
- [X] T028 [P] [US4] 双实现属性级 diff:目录库 100% / 设计库隐式 0 mismatch(SC-002)
- [X] T029 [P] [US4] 规模验证:ams1112 ~42.2 万元素(SC-003);修大库 walk 截断 bug

**Checkpoint**: `cargo test` 全绿(当前 **25 passed**);双实现对齐持续保障。

---

## Phase 7: User Story 5 — 安全的事务化离线编辑 (P2) — 新增价值

> 承接 **US5**(FR-019..022 / SC-008..010);全 E3D-无关、本环境可做可验证。对应 `.planning/2026-06-07-e3d-offline-edit-safety-batch`。

- [X] T030 [US5] 写后自校验 `verify_commit(orig, edited, ss, expects) -> Result<(), Vec<VerifyIssue>>`:① B 树不变式(`btree_check`/`BtreeReport` 已由 test-only 提为库内 pub API)② COW 不可变(原页字节仅 page0 变 + append-only)③ 目标元素读回==`Expect` ④ owner 引用完整性(不悬挂)— `crates/e3d_io/src/lib.rs`(FR-019)
- [X] T031 [P] [US5] 测试:正常提交通过 + 坏提交报出对应 Issue(OriginalMutated/ReadbackMismatch/DanglingRef)— `lib.rs` 测试 `verify_commit_sound_and_bad`(FR-019, SC-009)
- [X] T032 [US5] 事务化批量 `EdbWriter::batch(|w| {...})`:多笔编辑合**单**新会话(`sesno` 仅 +1),逐笔在前一笔 latest root 上串联 COW、结束规范化到同一新根(中间会话成孤儿页);出错整批回滚 — `lib.rs`(FR-020)
- [X] T033 [P] [US5] 测试:≥3 笔混合编辑(set_pos+insert+rename)→ sesno+1、单会话、全部生效、前会话不变、verify 通过 — `lib.rs` 测试 `batch_three_edits_single_session`(FR-020, SC-008 / 完成判据 a)
- [X] T034 [US5] dry-run + 元素级 diff `EdbWriter::dry_run(|w|...)->Diff`:副本应用不写盘(不改 writer),`element_diff` 产出 `{added,removed,modified:[{refno,attr,old,new}]}`(NAME 变化以 hash=NAME_HASH 呈现)— `lib.rs`(FR-021)
- [X] T035 [US5] CLI:`e3d-io plan <edits-file>`(dry-run 显示 diff)/ `apply <edits-file>`(batch + verify + 写副本,--inplace 落盘)— `crates/e3d_io/src/main.rs`(行式 edits 格式 + `parse_edits` 单测;实测 plan/apply on sam7200 通过)(FR-021 / FR-022)
- [X] T036 [P] [US5] 测试:dry-run diff == 真实提交后 `element_diff`(且 dry_run 不改 writer)— `lib.rs` 测试 `dry_run_matches_real_commit`(FR-021, SC-010 / 完成判据 c)
- [X] T037 [US5] 安全护栏:`delete_guards`(有子=HasMembers / 被引用=Referenced)默认拒绝删除,`--force` 放行;`--inplace` 需 `--yes` 二次确认 + 落盘前 `verify_commit` — `lib.rs`/`main.rs`(`delete_guards_block_parent` 测试 + 实测 --inplace 拦截)(FR-022)
- [X] T038 [P] [US5] Python 参考端同步 `verify_commit` + `batch_commit`(对齐 Rust)— `docs/.../e3d_write_full.py`(Slice 9 `_demo_verify_batch` PASS:sesno 36->37 单会话 + verify 无 issue)(FR-019 / FR-020,宪法 III 双实现对齐)

**Checkpoint**: 安全的事务化离线编辑层(批量单会话 + 自校验 + dry-run/diff + 护栏);US5 验收判据 SC-008..010 通过。

---

## Phase 8: Gated / 外部资源阻塞

- [~] T039 真实 running-E3D round-trip 取证(写侧最终判据)— **需用户侧可用 E3D**(research B1)
- [X] T040 `pdms_io` 整 crate 端到端构建 — **2026-06-10 解阻并验收**:旧账面阻塞全部消除——① surrealdb 统一 **dev-3.1 单栈**(提交 `5f27c6fb`,lock 仅 3.2.0-nightly @f01470af 一套)② rs-core 升 **0.3.2**、8 处 `from_value` 已改 `Result<_, surrealdb::Error>`(API 漂移已修)③ aws-lc-sys 正常构建(旧"NASM panic"实为 sccache 瞬时网络错)。**实测**:`cargo check`(默认 / `--features surrealdb`)均 0 错误;`cargo build -j2` 默认 1m41s + surrealdb 特性 2m27s 均 Finished。注:全并行 build 曾因内存+页面文件耗尽(os error 1455,12 bin 并行 codegen)假性失败,`-j2` 即过;建议调大 Windows 页面文件根治(research B2)
- [~] T041 UDA 真名/类型/单位解析 — 需 udalib 字典库文件(research B3)
- [~] T042 更多 catalogue 库(dbno 15206/15207/15213…)以解析全部规格/材料引用

---

## Phase 9: Polish & 文档

- [X] T043 [P] 格式规范 `E3D_DB_文件格式规范.md`(§2/§7/§8/§12)+ 总结 + 索引
- [X] T044 [P] 计划三件套(task_plan/findings/progress)`.planning/*`
- [X] T045 [P] spec-kit feature 与 `docs/.../E3D_DB_索引.md` 交叉链接:索引交付物表新增 specs/001 行 + 更新计数(20→24 测试 / 13→14 demo);规范 ↔ specs/001 互为入口
- [X] T046 `docs/e3d 数据库分析/README.md` 顶部导航指向 specs/001 + 最新索引(早期快照标注);`/speckit-analyze` 跨工件一致性已于本会话执行
- [X] T047 [P] [US3] 整库导出 JSON 经解析器校验合法 + 双实现 `element_count`/`refmap` 计数一致 — `e3d_export.py` / `tools/e3d_decode_rs`(SC-007;证据:导出产物 `e3d_sam7200_export.json` 可被 JSON 解析器解析 + T028/C3.3 计数一致)
- [X] T048 [P] [US2] Python 写侧自检 demo 全 PASS(`e3d_write_full.py` S1–S9,14 demo;含 Slice 9 verify+batch)— `docs/e3d 数据库分析/e3d_write_full.py`(SC-005)

---

## Dependencies & Execution Order

- **Phase 1–2** 前置,阻塞所有 US。
- **US1(P3 phase)** 是 MVP;**US2** 依赖 US1 作读校验;**US3/US4** 依赖 US1;**US5** 依赖 US2 写原语。
- **Phase 7(US5)** 依赖 US2 写原语(已具备)→ 可立即开工(T030→T032→T034→T037 顺序;[P] 测试随对应实现)。
- **Phase 8** 全 gated,不阻塞 1–7、9。

## Implementation Strategy

- **现状**:US1–US4 已交付并验证;US5(Phase 7)本环境任务 T030–T038 **全部完成**(`verify_commit` + 事务 `batch` + dry-run/`element_diff` + CLI `plan`/`apply` + 安全护栏 + Python 端对齐)(`cargo test` lib **25** + CLI `parse` 1,全绿 + Python **14** demo PASS)。
- **下一增量(推荐)**:本环境任务**全部完成**(Phase 1–7 + 9;T040 整 crate build 已于 2026-06-10 完成验收);余 **T039/T041/T042** gated(真机 round-trip / udalib / 更多 catalogue),需外部资源解阻。
- **Gated**:T039/T041/T042 待用户提供 E3D / udalib / 更多 catalogue 库后解阻(T040 已完成;rs-core 授权已给、暂无需改动)。

## Notes

- `[P]` = 不同文件、无依赖,可并行。
- 已完成项的"证据"= 对应测试名 / 脚本 / 规范章节(见 `contracts/decode-contract.md §C4` 锚点)。
- 测试先行(宪法 III):Phase 7 新代码 MUST 先写失败测试再实现。
- 加固:事务批量**原子回滚**回归 `batch_rollback_on_error`(一笔失败→整批回滚、会话数不变、追加页丢弃、字节复原),保障 FR-020 原子性。
