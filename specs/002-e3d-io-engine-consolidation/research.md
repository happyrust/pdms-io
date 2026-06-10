# Research: 三引擎现状证据 + grill-me 决策记录

**Date**: 2026-06-10 | **Spec**: [spec.md](./spec.md)

> 本文固化两件事：① 收敛决策所依据的**代码现状证据**（逐条可复查）；② grill-me 会话的 **Q1~Q6 决策记录**（含用户拍板的大爆炸切换风险声明）。格式字节级结论一律见 001，本文不重复。

## 1. 三引擎现状盘点（证据）

### 1.1 实现矩阵

| 能力 | v1 (`PdmsIO` 族) | `engine_v2` | `e3d_io` |
|---|---|---|---|
| 页 I/O + 缓存 | `page_manager.rs`（LRU/脏页/预读/统计，对照 db1_get_page）+ `paged_reader.rs` | `db1/page_cache.rs` + `db1/page_io.rs` + `io_layer/*` | 整文件 buffer（`fs::read` → `Edb::from_bytes`） |
| 头部/会话 | `io.rs::read_pdms_header` / `init_ses_maps`（沿 `last_ses_pageno` 回溯） | `db2/header.rs` + `db2/session.rs` | lib.rs 内 header→latest session 解析 |
| B 树（读） | `io.rs::search_in_leaf_node` / `read_index_data` / `build_index_map` | `db3/btree.rs`/`search.rs`/`iter.rs` | lib.rs 内 B 树下降/枚举（`word6` 界定） |
| B 树（写/插入/分裂） | `writer.rs::ElementWriter`（叶插入/split/split_root） | `db3/insert.rs`/`split.rs`/`delete.rs` | lib.rs 内 COW 写路径（001 US2/US5 验证） |
| 元素记录读 | `element_record_reader.rs`（变长、16K→64K 自适应、`00000007` padding） | `db4/element.rs`/`attrs.rs`/`page_layout.rs` | lib.rs 内记录/DA/成员/UDA 解码 |
| 元素记录写 | `element_serializer.rs`（与 reader 同构序列化） | （db4 部分） | lib.rs 内编码（验证过的写原语） |
| 打开/保存编排 | `io.rs::open`/`detect_page_size_by_probe` | `db5/open.rs`（探测逻辑与 v1 **逐行同构**）/`save.rs`/`close.rs` | open/decode/commit API |

**结论**：页大小探测逻辑在 v1 与 engine_v2 中**逐行重复**（`io.rs:239` vs `db5/open.rs:45`）；B 树写在三处各有一份；这是知识漂移的实锤。

### 1.2 依赖孤岛证据（grep 实测，2026-06-10）

- `engine_v2`：引用者仅自身模块 + `lib.rs` 声明 + `src/bin/verify_engine_v2.rs`。**零生产调用方**。
- `writer.rs`/`element_serializer.rs`（v1 写路径）：引用者仅 `lib.rs` 声明 + `src/test/test_write_integration.rs`。**零生产调用方**。
- `e3d_io`：在主 crate 中仅 `lib.rs` 的 `pub use e3d_io as e3d_decode` 再导出；`PdmsIO` 内部**未**使用它（两套读取并行）。
- `PdmsIO`：30+ 文件引用（`tests/*` 7 个集成测试、`src/test/`+`src/tests/` 十余个、`src/bin/*`、`watch.rs`、`main.rs`）→ API 必须保持。
- `update_elements_to_database`（SurrealDB 落库）：**no-op 占位**（`io.rs:346`，"保留接口并默认 no-op"）。

### 1.3 IO 模型差异

- `e3d_io`：整文件 buffer 模型。全部解析基于 `&[u8]`；`fs::read` 出现于 `lib.rs:279/384` 等。简单、可测，但 watcher 反复增量读大库时是全量读。
- v1：分页 LRU 模型。`PageManager::new(1024, page_size)`，预读 4 页、批量 flush 32，命中率统计——为"反复读取大文件的小部分"设计。
- 真实规模参照（001）：`ams1112` 103MB / ~42.2 万元素；测试样本 `sam7200_0001` 6.9MB。

### 1.4 现存关键陷阱（迁移时必须保持的行为）

- **页大小字段说谎**：`ams1112_0001` header 声明 512 实为 2048；v1 与 engine_v2 均以"probe `pgno*page_size` 处 `page_type==Session(3)`"兜底，候选顺序 2K→4K→512，最终兜底 2K。
- **`INDEX_PAGE_HEADER_SIZE=0x1C` 而非 0x24**（`writer.rs:18`）：读取侧把 `refno_locs` 视作 0x1C 起；写端若用 0x24，reopen 会把中间 8 字节零填充误判为空索引页。该知识随 v1 写路径退役**归档**（001 写侧已是权威实现）。
- **`ext_no` 恒 0**：`PdmsIO::local_file_ext_no` 注释明确"单扩展文件输入、dbnum 不能作物理扩展号"。`PageSource` 不得引入多 extent 假设。
- **会话范围推导**：`init_ses_maps` 以 `prev_end+1` 起算下一会话页范围、`end = end_pgno.max(start)`——增量归属语义依赖它。

## 2. grill-me 决策记录（2026-06-10）

| # | 问题 | 决策 | 要点/理由 |
|---|---|---|---|
| Q1 | 新 spec 主题 | **A. 三引擎收敛** | 001 已覆盖格式读写；收敛是当前最大架构债务，为写回接入(003)铺路 |
| Q2 | `engine_v2` 去留 | **A. 冻结/退役** | 零调用方、无测试；`e3d_io` 已双实现验证；完成 engine_v2 = 重新验证已验证的东西 |
| Q2' | v1 写路径去留 | **随同退役**（代码自答，未单独成问） | 同为孤岛；写能力唯一来源 = `e3d_io`（001 US2/US5） |
| Q3 | 统一后的 IO 模型 | **B. `PageSource` 页源抽象** | InMemory（现状零成本）+ PagedFile（承接 LRU）；格式逻辑只剩一份且大文件流式不退化 |
| Q4 | 迁移回归基线 | **C. 大爆炸切换**（用户拍板，推荐为 A 双跑对照被否） | 见下方风险声明 |
| Q5 | 002 范围 | **A. 严格收敛** | 不碰落库（no-op 保持）/搜索/sync/写回门面透出；多 extent 范围外；留 003+ |
| Q6 | 产出物 | **A. 精简套件** | spec/plan/research/tasks/contracts(api-compat)；不写 data-model（引用 001 防双源漂移）、不写 quickstart |

### Q4=C 风险声明（如实入档）

用户明确选择**大爆炸切换**：换芯不设新旧实现双跑逐属性 diff 闸门，直接删除旧路径。

- **后果**：`PdmsIO` 深层路径（索引缓存时序、会话边界推导、异常页大小样本）若现有测试未覆盖，回归只能事后发现。
- **既有保障**：workspace 现有测试套件（30+ 引用点的断言）+ `crates/e3d_io` 测试 + 001 的双实现对齐基线。
- **缓解**：`ams1112_0001` 列为必测样本；Phase 1 双页源一致性测试先行（在删除旧路径**之前**就能验证新核心）；git 历史为回滚退路。
- **不重谈**：此为用户在知晓推荐（A 双跑）后的明确决策，实现阶段不再 re-litigate。

## 3. 收敛后责任边界（目标态一句话）

> **`e3d_io` 拥有"字节怎么读写"的全部真相（经 `PageSource` 取页）；`PdmsIO` 只拥有"什么时候读、读完干什么"（缓存编排/会话范围/增量提取/下游集成）；其余实现不存在。**
