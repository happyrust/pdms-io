# Contract: `PdmsIO` API 兼容 + `PageSource` 页源

**Spec**: [../spec.md](../spec.md) | **Date**: 2026-06-10

> 本契约是 Phase 2 换芯的验收依据：C1 冻结门面公共 API；C2 定义页源 trait 不变量；C3 定义行为等价判据；C4 列退役清单。违反任一条即视为收敛失败。

## C1. `PdmsIO` 公共 API 冻结清单

以下签名（含语义）MUST 保持不变——来源：`src/io.rs` 现状 `pub fn` 盘点 + crate 根再导出（`lib.rs`: `pub use io::{PdmsIO, benchmark_increment_eles}`）。

**生命周期/基础读取**
- `new(project, file_path, detail)` / `open()` / `init_ses_range_map()`
- `read_pdms_header()` / `get_page_basic_info()`
- `read_bytes(offset, length)` / `read_data_cached(...)` / `read_element_record_cached(start_offset)`
- `cache_hit_rate()`（委托 `PagedFile` 统计后 MUST 仍有意义）

**会话**
- `get_sesno(pgno)` / `get_latest_sesno()` / `get_latest_att_pgno()` / `get_latest_dt()`
- `get_sesno_datetime(sesno)` / `get_sesno_timestamp(sesno)`
- `read_ses_data(ses_pgno)` / `get_ses_data(sesno)` / `get_ses_pageno(sesno)`
- `get_nearest_large_sesno(sesno)` / `get_nearest_less_sesno(sesno)`

**索引/查找**
- `read_index_data(index_pgno)` / `build_index_map()` / `build_index_map_default()` / `build_index_map_verbose(verbose)`
- `cache_index_map(path, map)` / `load_cached_index_map(path)`
- `search_latest_refno(refno, sesno)` / `search_latest_and_prev_refno(...)` / `search_in_leaf_node(...)`
- `find_refno_loc(...)` / `check_refno_exists(refno)` / `search_refno_pgno_optimized(refno)`
- `fast_lookup_refno(...)` / `fast_lookup_latest_loc(...)`

**元素/增量/历史**
- `parse_raw_element(refno_offset)` / `auto_get_raw_element(refno)`
- `collect_refno_locs(sesno)` / `collect_refno_locs_in_session(ses_pgno)` / `filter_index_data(...)`
- `collect_increment_eles(...)` / `collect_recent_n_sessions_eles(...)` / `collect_ele_history(refno)`
- `get_refno_operation_status(...)` / `get_refno_primary_operation_status(...)`
- `search_history_refnos(...)` / `get_attribute_value(...)` / `build_noun_attr_map(...)`
- `update_elements_to_database(...)`（**保持 no-op**，签名不动——003 的事）

**规则**
- 允许：内部私有项删除/重写、返回值内部表示替换（只要公共类型不变）。
- 禁止：公共签名变更、公共类型字段删改（`EleData`/`PdmsHeader`/`SessionPageData`/`IndexPageData`/`RefnoDataLoc` 等被调用方直接消费的类型）。
- `EleData` 若改为 `e3d_io` 类型适配产生（FR-003），MUST 保持字段级兼容。

**冻结落地（T201,2026-06-10）**：`tests/api_freeze_c1.rs` 把本清单全部签名以"调用 + 显式类型绑定"锁死——任一冻结签名变更（参数/返回/接收者可变性/async 性）即编译失败;泛型项以具体类型实例化锁定（`read_bytes<u64>/<i64>`、`build_noun_attr_map<&Path>`）,生命周期项独立函数锁定（`fast_lookup_*`）。**盘点备注**：`io.rs` 实际 pub 面更大（~80 fn,另含 `fast_get_*` 异步族、`parse_element`/`parse_incr_element`、`collect_latest_eles`、`sync_history`、`store_all_refno_sesno_map` 及模块级 demo/benchmark）;本清单为换芯委托核心,其余 pub 项同受"禁止签名变更"约束,由 workspace 既有测试(C3.1 oracle)与编译保障。

## C2. `PageSource` trait 契约（`crates/e3d_io`）

```rust
/// 格式核心与物理 I/O 的唯一边界。std-only。
pub trait PageSource {
    /// 页大小（512/2048/4096），对单个打开实例恒定。
    fn page_size(&self) -> usize;
    /// 取第 pgno 页的完整字节（长度 == page_size）。ext_no 现阶段恒 0。
    fn page(&mut self, ext_no: u32, pgno: u32) -> Result<&[u8], E3dError>;
}
```

**不变量**
- I1: 返回页长度 MUST == `page_size()`；越界 MUST 返回错误而非截断页。
- I2: 同一 `(ext_no, pgno)` 在文件未变更期间重复取页 MUST 字节一致（缓存透明性）。
- I3: 实现 MUST NOT 解释页内容（格式语义只在核心层）；MUST NOT 引入第三方依赖。
- I4: `ext_no` 仅透传；多 extent 语义为 002 范围外（恒 0）。

**两个实现**
- `InMemory`：整文件 buffer。I2 天然成立；为 CLI/测试默认，行为与收敛前 `e3d_io` 等价。
- `PagedFile`：文件句柄 + LRU。容量/驱逐/命中统计 MUST 等价承接 `PageManager::CacheStats`（hits/misses/reads/evictions）；页大小由打开时探测（C3.2）确定后恒定。

## C3. 行为等价判据

- **C3.1 测试 oracle**：现有 workspace 测试断言 = 行为基线；Phase 2 迁移**禁止**为通过测试修改断言（发现旧实现 bug 须单独记 issue，不在 002 顺手改语义）。
- **C3.2 页大小探测**：保持"header `page_size` 不可信"语义——以 `session_page_no`/`latest_ses_pgno` 在候选 {2048, 4096, 512}（按此顺序）探测 `page_type==Session(3)`，全失败兜底 2048。`ams1112_0001` 为本条的必测样本。
- **C3.3 会话范围**：`init_ses_maps` 的范围推导（沿 `last_ses_pageno` 回溯、oldest→newest 重放、`start = prev_end+1`、`end = end_pgno.max(start)`）MUST 语义等价，增量归属不得漂移。
- **C3.4 索引缓存文件**：`cache_index_map`/`load_cached_index_map` 二选一并写死：(a) 磁盘格式逐字节兼容；或 (b) 格式版本号失配时静默重建。选择结果在实现 PR 中回填本契约。
  **已落 (a)（2026-06-11,T207）**：换芯仅改变 `IndexMap` 的**构建来源**（`e3d_io` 枚举），磁盘缓存 `PIM1` 格式（magic+version+page_size+count+entries）与旧无 magic 格式的读取兼容逻辑均未动 = 逐字节兼容；版本/页大小失配维持**既有显式报错要求重建**（非静默重建）。持续护栏：`test_index_map_cache_roundtrip_pim1`。
- **C3.5 双页源一致性**：同一库经 `InMemory` 与 `PagedFile` 全库枚举，元素计数+属性值逐项一致（spec SC-003）。

## C4. 退役清单（Phase 3 验收）

| 删除项 | 能力去向 |
|---|---|
| `src/engine_v2/**`（25 文件） + `src/bin/verify_engine_v2.rs` | 知识归档 `docs/engine-v2-archaeology.md`；实现由 `e3d_io` 取代 |
| `src/writer.rs` + `src/element_serializer.rs` + `src/test/test_write_integration.rs` | 写能力 = `e3d_io`（001 US2/US5）；`0x1C` 头长等陷阱知识入归档文档 |
| `src/page_manager.rs` + `src/paged_reader.rs` | LRU/统计语义并入 `e3d_io::PagedFile` |
| `src/element_record_reader.rs` | 变长记录读取由 `e3d_io` 记录解码取代 |

**验收**：`lib.rs` 无上述模块声明；workspace 构建通过；`grep -r "engine_v2\|ElementWriter\|element_serializer"` 在 `src/` 无非文档命中。
