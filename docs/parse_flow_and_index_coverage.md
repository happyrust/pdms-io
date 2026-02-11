# 当前 Rust 解析流程与索引表覆盖度

本文档梳理 pdms-io 仓库中**数据解析流程**、**索引表/结构清单**及**已实现/缺口**，为写回新 Element 与 core.dll 回归提供参考。所有引用均为仓库内路径。

---

## 1. 当前 Rust 解析流程

### 1.1 入口与会话映射

- **入口**：`PdmsIO::open()`（[src/io.rs](src/io.rs) 约 L197–L218）
  - 调用 `get_file()` 打开数据库文件；
  - `read_pdms_header()` 读取头部，得到 `dbnum`；
  - `detect_page_size_by_probe(&header)` 识别真实 page_size（512/2K/4K），必要时重建 `page_cache`；
  - 若 `sesno_pgno_map` / `ses_range_map` 为空，调用 `init_ses_maps()`。
- **page_size 探测**：`detect_page_size_by_probe()`（[src/io.rs](src/io.rs) 约 L227–L258）
  - 对候选 2K/4K/512，用 `header.session_page_no`、`header.latest_ses_pgno` 计算偏移，读 4 字节 page_type；
  - 若为 Session(3) 则认定该 page_size 有效；否则兜底 2K。
- **会话链初始化**：`init_ses_maps()`（[src/io.rs](src/io.rs) 约 L285–L318）
  - 从 `header.latest_ses_pgno` 沿 `last_ses_pageno` 回溯，收集 `(sesno, ses_pgno, end_pgno)`；
  - 反转后填入 `sesno_pgno_map` 和 `ses_range_map`（会话号 → 页号范围）。

### 1.2 索引定位（RefNo → 数据位置）

- **B+树搜索**：`search_latest_refno(refno, sesno)` → `search_latest_refno_optimized()` → `btree_search_fixed(root_pgno, refno)`（[src/io.rs](src/io.rs) 约 L1047–L1072、L1133–L1142）。
- **递归下降**：`btree_search_optimized_recursive()`（[src/io.rs](src/io.rs) 约 L1151–L1313）
  - 读 `IndexPageData`（`read_index_data(page_no)`）；
  - `level == 0` 为叶子，在 `refno_locs` 中精确匹配 `(refno_0, refno_1)`，返回 `(sesno, get_att_offset_with_page_size())`；
  - 非叶子：处理起始标记 `0x80000001_0x80000001`、去重，按 B+树语义选子页递归。
- **按会话收集 RefNo 列表**：`collect_refno_locs_in_session(ses_pgno)`（[src/io.rs](src/io.rs) 约 L2877–L2915）
  - 读当前会话页得 `end_pgno`、`last_ses_pageno`、`index_root_pageno`；
  - 读上一会话的 `end_pgno`，再读索引根页，`filter_index_data()` 递归过滤出 `last_end_pgno < pgno < cur_end_pgno` 且 `flag==1` 的 `RefnoDataLoc`。

### 1.3 跨页读取元素记录

- **按偏移读变长记录**：`ElementRecordReader::read()`（[src/element_record_reader.rs](src/element_record_reader.rs)）
  - 内部用 `PagedReader::read()` 按 page 跨页读取（[src/paged_reader.rs](src/paged_reader.rs)）；
  - `find_record_end()` 通过 impl_len、0x00000000/0x00000007 padding、0x0001/0x0002 块与 segment 规则确定记录结尾。
- **页缓存**：`PageManager::get_page()`（[src/page_manager.rs](src/page_manager.rs)）LRU 缓存，未命中则从文件读取。

### 1.4 元素解析（隐式/成员/显式/UDA）

- **同步解析**：`parse_pdms_db::parse_raw_ele_data_with_info(input, database_info)`（[crates/parse_pdms_db/src/parse.rs](crates/parse_pdms_db/src/parse.rs) 约 L392–L682）
  - 前 4 字节 impl_len，4–12 refno，12–16 type_hash(noun)，16–24 owner；
  - 隐式区按 `noun_attr_info_map` 布局解析，支持 0/7 padding 扩展 actual_impl_len；
  - members 块（flag 0x0002）用 `collect_segmented_payload` + `parse_attr_members`；
  - 显式区 `parse_raw_explicit_attrs`，UDA 与普通显式分开。
- **异步封装**：`parse_ele_data_with_info()`（约 L691–L721）在同步解析基础上做 UDA 处理和 `refine`。
- **全库基础数据**：`parse_db_basic_data()`（约 L913）生成 `DbBasicData`（refno_table_map、children_map 等）；`parse_file_db_basic_data()`（约 L205）为文件入口。

---

## 2. 索引表/结构清单与代码位置

| 类型 | 结构/枚举 | 说明 | 定义位置 | 读/写覆盖 |
|------|-----------|------|----------|-----------|
| 文件头 | `PdmsHeader` | 版本、db_num、latest_ses_pgno、session_page_no、page_size、stored_page_count 等 | [src/defines.rs](src/defines.rs) L40–L75 | 读：io 使用；写：HeaderUpdater 部分 |
| 页面类型 | `PageType` | 1=RefArray, 3=Session, 5=Data, 7=Special, 8=Index | [src/defines.rs](src/defines.rs) L628–L677 | 读/校验 |
| 数据页子类型 | `DataPageSubtype` | Main/MainVariant, Aux/AuxIndex, Index, Attr, Ext, Element；含 get_bucket_id() | [src/defines.rs](src/defines.rs) L682–L770 | 读/校验 |
| 会话页 | `SessionPageData` | page_type, last_ses_pageno/extno, sesno, end_pgno/extno, index_root_pageno/extno, claim_pageno/extno 等 | [src/defines.rs](src/defines.rs) L92–L157 | 读：io 链式回溯；写：SessionBuilder |
| RefNo 位置 | `RefnoDataLoc` | refno_0/1, pgno, offset(20bit), flag(12bit)；get_att_offset_with_page_size() | [src/defines.rs](src/defines.rs) L347–L400 | 读：B+树/collect；写：writer 单条插入 |
| B+树索引页 | `IndexPageData` | page_type, noun(0xCC47DF), level, unknowns, pfno, refno_locs, remain_zero_bytes | [src/defines.rs](src/defines.rs) L434–L495 | 读：read_index_data/filter；写：仅叶子空槽 |
| 索引页（类型 8） | `RefnoIndexPage` | page_type, noun, unknowns_0, pfno, data_locs(RefnoIndexPgId) | [src/defines.rs](src/defines.rs) L402–L429 | 读：可选；写：未用 |
| 根索引页 | `RootIndexPage` | residual_num, lock, last_pageno/extno, lower_root, upper_root | [src/defines.rs](src/defines.rs) L318–L341 | 读：未用；写：未用 |

---

## 3. 覆盖度与缺口

### 3.1 已较好覆盖

- **解析链路**：打开 → 探测 page_size → 会话映射 → B+树按 RefNo 查 → 跨页读元素记录 → parse_pdms_db 解析为 `EleData`。B+树路径选择已修复（见 [issues/btree-search-algorithm-fix.md](issues/btree-search-algorithm-fix.md)）。
- **页面类型与数据页子类型**：枚举与校验（[src/defines.rs](src/defines.rs)）、`verify_page_type` / `verify_data_page_subtype`。
- **会话页**：完整解析与链式回溯；`SessionBuilder` 可构建会话页字节（[src/writer.rs](src/writer.rs)）。
- **RefNo B+树**：读取、递归过滤、叶子内精确查找；`RefnoDataLoc` 序列化/反序列化与 offset 计算（含动态 page_size）。

### 3.2 缺口（写回与回归必须补）

- **索引写入**（[src/writer.rs](src/writer.rs)）：
  - `update_index_entry`（L179）、`insert_index_entry`（L256）：仅在单页内找空 16B 槽位写入，**不保证按 refno 有序**，**不支持节点满时的 split**。
- **跨页写入**：`DataPageWriter::write_data()`（L466）在单记录超页时 **panic**（L469 TODO: 处理跨页元素）；需按 0x00000007 与 segment 规则做跨页写入。
- **会话提交**：`DatabaseWriter::commit_session()` 中 `index_root(0)` 为硬编码 TODO（L817），`claim_*` 未维护；需与 db2/db5 语义对齐。
- **数据页头**：`DataPageWriter` 仅写 page_type + subtype（[src/writer.rs](src/writer.rs)）；文档中数据页还有 db_handle、ext_no、page_no、bucket_id 等，需按 2.10 布局补齐。
- **属性/名词元数据**：隐式/显式解析依赖 `PdmsDatabaseInfo`（named_attr_info_map、noun_attr_info_map），来自 aios_core 预置，**未从 DB 自举**属性字典或系统属性列表；写回时仍可复用现有 noun 类型与属性布局。

---

## 4. 关键代码引用速查

| 功能 | 位置 |
|------|------|
| PdmsIO::open, detect_page_size_by_probe, init_ses_maps | [src/io.rs](src/io.rs) L197–L318 |
| search_latest_refno, btree_search_fixed, btree_search_optimized_recursive | [src/io.rs](src/io.rs) L1047–L1313 |
| read_index_data, collect_refno_locs_in_session, filter_index_data | [src/io.rs](src/io.rs) L1945–L1954, L2877–L2915, L2956–L3004 |
| PdmsHeader, SessionPageData, RefnoDataLoc, IndexPageData, PageType, DataPageSubtype | [src/defines.rs](src/defines.rs) L38–L75, L92–L157, L347–L400, L434–L466, L628–L770 |
| parse_raw_ele_data_with_info, parse_ele_data, parse_db_basic_data | [crates/parse_pdms_db/src/parse.rs](crates/parse_pdms_db/src/parse.rs) L392–L682, L685–L727, L913–L968 |
| ElementRecordReader::read, find_record_end | [src/element_record_reader.rs](src/element_record_reader.rs) |
| update_index_entry, insert_index_entry, write_data (TODO), commit_session (index_root TODO) | [src/writer.rs](src/writer.rs) L179–L243, L256–L318, L466–L471, L798–L834 |

上述引用便于在实现写回与 core.dll 回归时精确定位与扩展。
