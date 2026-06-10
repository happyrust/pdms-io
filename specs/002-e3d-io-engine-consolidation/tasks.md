# Tasks: E3D I/O 三引擎收敛

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-06-10

> 标注：`[P1]`=US1/US2 主线必做；`[P2]`=US3/US4；每个 Phase 末尾为闸门任务（GATE）。Q4=C 大爆炸——Phase 2 GATE 过后才允许执行 Phase 3 删除。

## Phase 0 — Research（已完成）

- [x] T000 三引擎现状盘点 + grill-me Q1~Q6 决策入档 → `research.md`

## Phase 1 — `PageSource` 页源抽象（`crates/e3d_io`）

- [x] T101 [P1] 新建 `crates/e3d_io/src/page_source.rs`：`PageSource` trait（契约 C2：`page_size()` + `page(ext_no, pgno)`，std-only）— 提交 `997dbd49`
- [x] T102 [P1] `InMemory` 实现：现有整文件 buffer 路径下沉为该实现；`Edb::from_bytes` 等入口改走 trait（行为零变化）— 完成：`Edb::from_bytes`/`open` 经 `InMemory` 构造，页大小推断单源 `page_size_from_header`；33+1 全绿（附带加固：短于头部的退化输入由兜底 2048 取代旧越界 panic）
- [ ] T103 [P1] 格式核心取页点改造：lib.rs 内所有直接 `&buf[off..]` 按页访问处统一经 `PageSource`（保持 chain()/B 树/记录解码逻辑不动，只换取字节的方式）—（**实现注记 2026-06-10**：lib.rs ~45 处 `db.buf` 直接访问中绝大多数是 COW **写**路径〔整页克隆/追加/page0 补丁〕,其 flat-buffer 模型为 001 验证语义、按设计保留 `InMemory`;T103 实际范围 = **读侧**取页点〔decode/walk/index_db/B 树下降/会话链〕抽出经 `PageSource` 的读视图,使 Phase 2 `PdmsIO` 能在 `PagedFile` 上**只读**委托。写经 `PagedFile` 非 002 需求）
- [x] T104 [P2] `PagedFile` 实现：移植 `src/page_manager.rs` 的 LRU（容量/驱逐/脏页语义读侧裁剪/CacheStats 命中统计），含页大小探测（契约 C3.2）— 提交 `997dbd49`
- [ ] T105 [P2] 双页源一致性测试：`sam7200_0001` 经 `InMemory` vs `PagedFile` 全库枚举逐项一致（SC-003）；`ams1112_0001` 探测样本测试（有则跑、缺则优雅跳过，沿用 001 测试惯例）—（已落:sam7200 全文件**页级**双源一致 + ams1112 探测自洽〔探测页大小取 latest-session 页验 page_type==3,实测通过非跳过〕;**元素级**全库枚举对比待 T103 后升级）
- [x] T106 [P2] 缓存可观测测试：`PagedFile` 增量式读取的读页计数 < 全文件页数（SC-006）— 合成 + sam7200 真实样本双覆盖,命中/驱逐计数断言
- [ ] T107 GATE：`cd crates/e3d_io && cargo test` 全绿；无第三方依赖引入（`cargo tree` 核查）

## Phase 2 — `PdmsIO` 换芯（API 冻结）

- [ ] T201 [P1] 按契约 C1 冻结公共 API 清单（编译期核查：保留签名的门面骨架先行）
- [ ] T202 [P1] 头部/页大小探测委托：`read_pdms_header`/`detect_page_size_by_probe` → `e3d_io`（删除 `io.rs` 内私有重复实现）
- [ ] T203 [P1] 会话链委托：`init_ses_maps`/`read_ses_data`/`get_sesno*` 族 → `e3d_io` 会话解析（契约 C3.3 语义等价）
- [ ] T204 [P1] B 树/查找委托：`read_index_data`/`search_in_leaf_node`/`build_index_map*`/`search_latest_refno` 族 → `e3d_io` B 树（经 `PagedFile` 页源）
- [ ] T205 [P1] 元素读取委托：`parse_raw_element`/`auto_get_raw_element`/`read_element_record_cached` → `e3d_io` 记录解码；`EleData` 适配策略落地（FR-003：评估 `parse_pdms_db` 保留为类型适配层 or 并入）
- [ ] T206 [P1] 增量/历史迁移：`collect_increment_eles` 族 / `collect_ele_history` / `get_refno_*_status` 跑在新芯上，现有断言不动（契约 C3.1）
- [ ] T207 [P2] 索引缓存文件兼容决策落地（契约 C3.4 二选一），回填契约文档
- [ ] T208 GATE：`cargo build --workspace` + 全部现有测试全绿（`src/test/`、`src/tests/`、`tests/`、bins 编译）；调用方零修改

## Phase 3 — 孤岛退役（仅在 T208 通过后执行）

- [ ] T301 [P2] 删除 `src/engine_v2/**` + `src/bin/verify_engine_v2.rs` + `Cargo.toml` 对应 `[[bin]]`；`lib.rs` 摘除声明
- [ ] T302 [P2] 删除 `src/writer.rs` + `src/element_serializer.rs` + `src/test/test_write_integration.rs`；`lib.rs` 摘除声明
- [ ] T303 [P2] 删除 `src/page_manager.rs` + `src/paged_reader.rs` + `src/element_record_reader.rs`（确认 Phase 1/2 已吸收其全部在用能力）
- [ ] T304 [P2] 知识归档 `docs/engine-v2-archaeology.md`：db1~db5 ↔ core.dll 函数对照、`INDEX_PAGE_HEADER_SIZE=0x1C`、`START_MARKER 0x80000001` 等陷阱（来源：engine_v2 注释 + writer.rs 注释 + research.md §1.4）
- [ ] T305 GATE：workspace 构建通过；`grep` 无残留引用（契约 C4 验收）

## Phase 4 — 清理与文档

- [ ] T401 更新 `docs/ARCHITECTURE.md`：单核心 + 门面目标态架构（替换三引擎现状描述）
- [ ] T402 实现盘点核查（SC-001）：B 树/元素记录/页缓存各仅一份的 grep 证明，结果附入 PR
- [ ] T403 `CHANGELOG.md` 记录收敛；spec 状态 Draft → Implemented
- [ ] T404 GATE：SC-001~SC-006 逐条核对勾销

## 依赖关系

```
T101→T102→T103→T107(GATE)
T104→T105/T106→T107
T107→T201→T202~T207→T208(GATE)
T208→T301/T302/T303→T304→T305(GATE)
T305→T401~T404
```

## 范围外提醒（FR-013）

落库（`update_elements_to_database` 保持 no-op）、Meilisearch、`sync/`、`PdmsIO` 透出写/事务 API、多 extent——**本任务清单不含以上任何项**；出现相关"顺手做"冲动时记 003 候选，不动手。
