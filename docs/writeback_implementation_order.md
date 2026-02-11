# 写回实现顺序与串联

本文档明确“写回新 Element 并完成 core.dll 回归”的**实现顺序**，与 [writer_modules_design.md](writer_modules_design.md)、[parse_flow_and_index_coverage.md](parse_flow_and_index_coverage.md) 中的缺口一一对应，便于按步验收。

---

## 1. 实现顺序（五步）

| 步骤 | 名称 | 内容与验收 |
|------|------|------------|
| **1** | **数据页头对齐** | 在 `DataPageWriter`（或后续 `storage/page`）中，按 2.10 数据页布局写入完整页头：除 `page_type`(4B) + `subtype`(4B) 外，补齐文档/逆向中的 `db_handle`、`ext_no`、`page_no`、`bucket_id` 等字段，并与 [db1_页面管理器分析总结](e3d%20数据库分析/db1_页面管理器分析总结.md)、样本 DB 二进制对照。验收：新写出的数据页头与现有可读 DB 的 data 页头字段一致。 |
| **2** | **元素记录序列化 + 跨页写入** | 在 `ElementRecordReader` / `EleSerializer` 同构前提下，实现单条元素记录跨页写入：首段写满当前页时插入 0x00000007 padding 与 segment 边界，续段写入新页并保持与读取端一致的 end-marker 规则。移除 `DataPageWriter::write_data` 中“超页即 panic”的 TODO，改为分段写入并返回首段位置 + 总长度。验收：Rust 写出的新元素可被 `PdmsIO` + `parse_raw_ele_data_with_info` 完整读回。 |
| **3** | **B+树有序插入 + split** | 在 `writer.rs` 的 `insert_index_entry` / `update_index_entry` 或新模块 `storage/index/refno_btree` 中实现：叶子页内按 (refno_0, refno_1) 有序插入；页满时 `split_node`，向父节点插入分裂键；根满时 `split_root` 升高树高。页头/条目布局与 2.10 core.dll db3 对齐（可参考 ida_exports/structs.json）。验收：插入若干 RefNo 后，Rust 侧 `search_latest_refno` 能正确找到并读回。 |
| **4** | **Session commit / claim / 头部更新** | 在 `DatabaseWriter::commit_session` 中：用当前 B+树根页号设置 `index_root_pageno/index_root_extno`（替代硬编码 0）；按 db2/db5 语义维护 `claim_pageno/claim_extno`（若实现 claim 链表）；会话页的 `end_pgno/end_extno` 指向本会话最后数据页；提交后调用 `HeaderUpdater` 更新 `latest_ses_pgno`、`stored_page_count`。验收：提交后再次打开同一 DB，会话链与索引根一致，新元素仍可被 RefNo 查得。 |
| **5** | **core.dll 回归测试串联** | 基准 DB 使用 `test-file/acp7002_0001` 的临时副本；Rust 写入 1 个新 Element（最小属性：REFNO/TYPE/OWNER/NAME）；分别生成 Rust 解析 JSON 与 core.dll harness JSON（格式见 [core_dll_harness_plan.md](core_dll_harness_plan.md)）；自动化字段级 diff（忽略 PGNO/SESNO 等）。验收：两路 JSON 在约定字段上一致，或 diff 列表可接受。 |

---

## 2. 依赖关系（为何此顺序）

- **步骤 1** 是基础：后续写出的每一页都带正确页头，core.dll 与 Rust 解析才能一致识别页类型与桶/扩展信息。
- **步骤 2** 保证“单条元素”可跨页落盘且读回一致，是写回语义正确性的核心。
- **步骤 3** 保证新元素可通过 RefNo 被索引到，否则无法按 RefNo 读回或交给 core.dll。
- **步骤 4** 保证会话与头部一致，再次打开 DB 时索引根和会话链正确。
- **步骤 5** 在 1–4 完成后串联，用 core.dll 做跨实现对比，验证与 2.10 行为一致。

---

## 3. 与模块/缺口的对应

| 步骤 | 对应模块 | 对应缺口（parse_flow_and_index_coverage.md） |
|------|----------|---------------------------------------------|
| 1 | storage/page 或 writer 内 DataPageWriter | 数据页头仅写 page_type+subtype，缺 db_handle/ext_no/page_no/bucket_id 等 |
| 2 | storage/element/record | 跨页写入 panic；需 0x00000007 + segment 规则 |
| 3 | storage/index/refno_btree | insert 不保证有序、不支持 split |
| 4 | storage/session + HeaderUpdater | commit_session 中 index_root(0) 为 TODO，claim_* 未维护 |
| 5 | ffi/core_dll + 测试 | 需 harness 与 JSON 对比策略（见 core_dll_harness_plan.md） |

按上述顺序实现并逐步验收，即可完成“写回新 Element + core.dll 回归”的闭环。
