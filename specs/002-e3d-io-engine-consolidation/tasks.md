# Tasks: E3D I/O 三引擎收敛

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-06-10

> 标注：`[P1]`=US1/US2 主线必做；`[P2]`=US3/US4；每个 Phase 末尾为闸门任务（GATE）。Q4=C 大爆炸——Phase 2 GATE 过后才允许执行 Phase 3 删除。

## Phase 0 — Research（已完成）

- [x] T000 三引擎现状盘点 + grill-me Q1~Q6 决策入档 → `research.md`

## Phase 1 — `PageSource` 页源抽象（`crates/e3d_io`）

- [x] T101 [P1] 新建 `crates/e3d_io/src/page_source.rs`：`PageSource` trait（契约 C2：`page_size()` + `page(ext_no, pgno)`，std-only）— 提交 `997dbd49`
- [x] T102 [P1] `InMemory` 实现：现有整文件 buffer 路径下沉为该实现；`Edb::from_bytes` 等入口改走 trait（行为零变化）— 完成：`Edb::from_bytes`/`open` 经 `InMemory` 构造，页大小推断单源 `page_size_from_header`；33+1 全绿（附带加固：短于头部的退化输入由兜底 2048 取代旧越界 panic）
- [x] T103 [P1] 格式核心取页点改造：lib.rs 内所有直接 `&buf[off..]` 按页访问处统一经 `PageSource`（保持 chain()/B 树/记录解码逻辑不动，只换取字节的方式）—（**范围勘定 2026-06-10**：lib.rs ~45 处 `db.buf` 直接访问中绝大多数是 COW **写**路径〔整页克隆/追加/page0 补丁〕,其 flat-buffer 模型为 001 验证语义、按设计保留 `InMemory`,写经 `PagedFile` 非 002 需求）**落地** = 新增 `src/read_view.rs`:`Rdb<S: PageSource>` 只读视图(惰性影子页,按需取页),导航算法与 lib.rs 同式(会话链/`latest_root`/word6 界定 walk/主记录定位过滤/`record_bytes` 原始记录窗口),`pages_read()` 可观测;Phase 2 委托清单(C1)所需的导航+原始记录读取全覆盖,`decode_full`(DA 链全属性解码)按设计留 `Edb` 路径
- [x] T104 [P2] `PagedFile` 实现：移植 `src/page_manager.rs` 的 LRU（容量/驱逐/脏页语义读侧裁剪/CacheStats 命中统计），含页大小探测（契约 C3.2）— 提交 `997dbd49`
- [x] T105 [P2] 双页源一致性测试：`sam7200_0001` 经 `InMemory` vs `PagedFile` 全库枚举逐项一致（SC-003）；`ams1112_0001` 探测样本测试（有则跑、缺则优雅跳过，沿用 001 测试惯例）— 三层齐备:① 全文件**页级**字节双源一致 ② **元素级** `Rdb<InMemory>` vs `Rdb<PagedFile>` 全库叶项枚举等值 + 主记录定位等值 + 500 记录原始字节抽查(`read_view.rs`) ③ ams1112 探测自洽(探测页大小取 latest-session 页验 page_type==3,实测非跳过);属性值层等价由"页级字节相等 + 单一解码实现"蕴含
- [x] T106 [P2] 缓存可观测测试：`PagedFile` 增量式读取的读页计数 < 全文件页数（SC-006）— 合成 + sam7200 真实样本双覆盖,命中/驱逐计数断言;`Rdb` 点状导航 pages_read < 10% 全页数
- [x] T107 GATE：`cd crates/e3d_io && cargo test` 全绿；无第三方依赖引入（`cargo tree` 核查）— **2026-06-10 通过**:36 lib + 1 CLI 测试全绿;`cargo tree` 单节点(零依赖)。**Phase 1 收口**

## Phase 2 — `PdmsIO` 换芯（API 冻结）

- [x] T201 [P1] 按契约 C1 冻结公共 API 清单（编译期核查：保留签名的门面骨架先行）— `tests/api_freeze_c1.rs`:C1 全清单签名"调用+显式类型绑定"编译期锁(泛型具体化/生命周期独立锁/async 性经 Future 断言),`cargo test --test api_freeze_c1` 链接+运行通过;契约 C1 回填冻结落地与全 pub 面盘点备注(~80 fn,清单外项由 workspace 测试+编译保障)
- [x] T202 [P1] 头部/页大小探测委托：`read_pdms_header`/`detect_page_size_by_probe` → `e3d_io`（删除 `io.rs` 内私有重复实现）— 探测核心收敛为 `e3d_io::page_source::probe_page_size` 单源（候选 2K→4K→512 外层 × 探测点 0x30→0x28 内层,page_type==Session(3),与原 v1 实现逐项同语义;`PagedFile` 同步改用同一单源+双探测点）;`io.rs` 私有探测循环已删,兜底 2K 语义保留。**注**:`read_pdms_header` 保留 deku 类型化解析(它是门面冻结 API 的 `PdmsHeader` 类型视图,非逻辑重复;页大小权威=探测,已单源)。验证:e3d_io 36+1 全绿 + pdms_io check 过 + `test_open_smoke`(ams1112 说谎头,探测 2K)实跑通过
- [x] T203 [P1] 会话链委托：`init_ses_maps`/`read_ses_data`/`get_sesno*` 族 → `e3d_io` 会话解析（契约 C3.3 语义等价）— 链回溯单源化:`Rdb::session_chain()`(newest-first,SesInfo{pgno,sesno,last,end,root},字段偏移与 SessionPageData 同源;终止/环防与 v1 等价,负/越界 last 由页数界止〔v1 在越界链上会报错中断,新实现界止为部分链——仅损坏文件路径的差异,已注记〕);`init_ses_maps` 走 `Rdb<PagedFile>` 委托,**保留** v1 编排(oldest→newest 重放 + C3.3 范围推导 prev_end+1/end.max(start))。`read_ses_data`/`get_sesno*` 族保留 deku 类型视图与映射查询(门面职责,沿 T202 模式)。验证:e3d_io 37+1(新增链双源等值+roots 互证+链自洽测试)+ smoke 新增 `test_ses_maps_smoke`(ams1112 实跑,latest sesno + 映射在位)全绿
- [x] T204 [P1] B 树/查找委托：`read_index_data`/`search_in_leaf_node`/`build_index_map*`/`search_latest_refno` 族 → `e3d_io` B 树（经 `PagedFile` 页源）— **完成 2026-06-11**:`build_index_map*` 整树枚举亦已委托 `Rdb::leaves`(word6 界定 walk;v1 BFS + null 终止 deku 读取退役,`process_leaf_node` 过滤语义〔跳 (0,0)/pg==0/off==0,偏移 pg*ps+off*2,sort+dedup〕原样保留;oracle 复跑 76/1〔既有〕/5 与基线逐位一致,含 `test_index_map_cache_roundtrip_pim1`)。（**点查热路径已换芯 2026-06-11**:① e3d_io 侧 `Rdb::btree_find` 目标式 O(log n) 点查,与 `btree_descend` 同式,sam7200 10392 键穷举双源等值 ② io.rs 侧 `search_latest_refno_optimized` 改经持久 `Rdb<PagedFile>` 委托(`PdmsIO.rdb` 私有字段,LRU 1024 页同容量,文件增长按页数失配自动重建,页大小变更时重置);v1 `btree_search_fixed`/`btree_search_optimized_recursive` 标记退役注记待 Phase 3 删除——parity 结论:v1 的去重/哨兵特判/选最后启发式是 **null 终止读取**(`read_refno_data_loc` 读到 0 停,即 findings §16 已证伪的解读)的补偿,word6 界定下同语义不需要 ③ oracle:lib 全量 **76 过/1 失/5 忽略**,唯一失败 `page_manager::test_dirty_eviction_writes_back` 经 git stash 复测**HEAD 上同样失败**=既有红灯(v1 写侧孤岛,T303 退役对象),api_freeze 锁未触发。**余项**:`build_index_map*` 枚举族换线待做;`read_index_data`/`search_in_leaf_node` 保留(deku 类型视图/纯函数,冻结面)）
- [ ] T205 [P1] 元素读取委托：`parse_raw_element`/`auto_get_raw_element`/`read_element_record_cached` → `e3d_io` 记录解码；`EleData` 适配策略落地（FR-003：评估 `parse_pdms_db` 保留为类型适配层 or 并入）
- [ ] T206 [P1] 增量/历史迁移：`collect_increment_eles` 族 / `collect_ele_history` / `get_refno_*_status` 跑在新芯上，现有断言不动（契约 C3.1）
- [ ] T207 [P2] 索引缓存文件兼容决策落地（契约 C3.4 二选一），回填契约文档
- [ ] T208 GATE：`cargo build --workspace` + 全部现有测试全绿（`src/test/`、`src/tests/`、`tests/`、bins 编译）；调用方零修改 —（**基线注记 2026-06-11**:`page_manager::tests::test_dirty_eviction_writes_back` 在换芯**之前**即失败〔git stash 复测确认〕,属 v1 写侧孤岛(T303 删除对象);T208 验收口径 = 不劣于该基线,或 Phase 3 先删除该孤岛后全绿）

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
