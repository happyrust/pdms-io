# engine_v2 / v1 写路径 考古归档

> **来源**:specs/002(E3D I/O 三引擎收敛)Phase 3 孤岛退役(T304)。
> 2026-06-11 删除 `src/engine_v2/**`(39 文件)、`src/writer.rs`、`src/element_serializer.rs`、
> `src/page_manager.rs`、`src/paged_reader.rs`、`src/element_record_reader.rs`。
> 本文归档其中**不可丢失的逆向知识**;完整源码在 git 历史(删除提交的父提交)可查。
> 格式字节级权威结论一律以 `specs/001-e3d-data-format/` 与
> `docs/e3d 数据库分析/E3D_DB_文件格式规范.md` 为准,本文不另立真相。

## 1. db1~db5 ↔ core.dll 函数对照(engine_v2 注释抢救)

engine_v2 是按 AVEVA `core.dll` 的 db 分层逐函数对齐的实验性重写,其注释保存了
IDA 逆向得到的函数名对照——这是它最有价值的遗产:

| 层 | engine_v2 模块 | core.dll 对照 |
|---|---|---|
| **db1 页缓存/物理 I/O** | `db1/page_cache.rs`、`db1/page_io.rs` | `db1_get_page`、`db1_is_page_incore`、`db1_plu_locate_entry`(LRU 槽位)、`db1_read_page`/`db1_write_page`、描述符池 `dword_6A540EC` |
| **db2 头部/会话/Extract** | `db2/header.rs`、`db2/session.rs`、`db2/extract.rs`、`db2/db_lookup.rs` | `db2_modify_header_page`、`db2_get_db_int_att`/`db2_set_db_int_att`、`db2_get_session_pgid`、`db2_insert_extract`/`db2_remove_extract`、`db2_find_db_data` |
| **db3 B 树** | `db3/btree.rs`、`search.rs`、`insert.rs`、`split.rs`、`delete.rs`、`iter.rs`、`table.rs` | 搜索对齐 `FHSRCH`;插入/分裂/根长高 = `db3_change_table_entry`(3.3.1)、`db3_split_node`(3.2.6)、`db3_split_root`(3.2.7) |
| **db4 元素/引用/CE** | `db4/element.rs`、`attrs.rs`、`refs.rs`、`ce.rs`、`page_layout.rs` | `db4_create_element`(opcode 32)、`db4_copy_user_element`、`db4_insert_ref`/`db4_remove_ref`、CE 导航栈 |
| **db5 打开/保存/关闭** | `db5/open.rs`、`save.rs`、`close.rs` | `db5_open_read_db`(opcode 134, mode=7)、`db5_open_write_db`(opcode 138, 独占锁)、`db5_save_work`、`db5_close_db` |
| **io_layer 重试/句柄** | `io_layer/retry.rs`、`file_manager.rs`、`direct_io.rs` | `SYWAIT` + `FHSWIT` 重试、`FIOXST`/`FIONEW`、`DirectAccessToken` vtable 读写接口 |

> 真机写侧契约(`db5_save_work` 提交序列)详见 001 计划 findings §15.3 与格式规范 §12。

## 2. 必保陷阱(已被新核心吸收,此处记出处)

1. **索引页条目区从 0x1C 起,不是 0x24**(原 `writer.rs:18`):
   > 现有读取侧 IndexPageData 将 refno_locs 视作从 0x1C 开始;写端若保留 0x24 头长,
   > reopen 后会把中间 8 字节零填充误判为"空索引页"。
   现状:`e3d_io` 写侧(001)以 word7(0x1C)为条目起点,与读侧一致。✅
2. **B 树哨兵 `0x80000001_0x80000001` = −∞ 最左分隔**(原 `writer.rs:22` `START_MARKER_*`、
   engine_v2 `db3/btree.rs` `START_MARKER`):内部节点 entry0 指向最小键子树,遍历必须下降
   (跳过它曾导致 38% 少数,001 findings §16)。现状:`e3d_io` `key_le` 哨兵语义 + word6 界定 walk。✅
3. **头部 `page_size` 字段说谎**(ams1112 声明 512 实为 2048):以 `pgno×ps` 处
   `page_type==Session(3)` 探测,候选序 2K→4K→512,双探测点 0x30(历史名 session_page_no,
   实为 extent 分配计数)→0x28(latest_ses_pgno)。现状:单源 `e3d_io::page_source::probe_page_size`(T202)。✅
4. **`ext_no` 恒 0**:单 extent 文件输入,dbnum 不能作物理扩展号(原 `PdmsIO::local_file_ext_no`
   注释)。现状:`PageSource` 契约 C2 I4(`ext_no` 仅透传)。✅
5. **会话页 claim 字段**:0x24=claim_pageno、0x28=claim_extno(原 `writer.rs:1461`,
   `SessionPageData` 同名字段)——写会话页时需回填。
6. **会话范围推导**(增量归属根基):oldest→newest 重放,`start = prev_end+1`,
   `end = end_pgno.max(start)`。现状:`PdmsIO::init_ses_maps` 保留编排,链回溯单源
   `Rdb::session_chain`(T203,契约 C3.3)。✅
7. **变长元素记录定界**(原 `element_record_reader.rs`):自适应 16K→64K 窗口;
   隐式区顺延 / `0x0001` 显式块 / `0x0002` 成员块 / `0x07` 追加段 / 双词终止 /
   相邻记录启发。现状:同式移植 `Rdb::element_record`(T205,ams1112 430 条逐字节 parity)。✅

## 3. 旧实现的已知缺陷(留档防"复活")

- **v1 写路径从未生产化**:`writer.rs`/`element_serializer.rs` 零生产调用方;
  `page_manager` 脏页回写测试(`test_dirty_eviction_writes_back`)在退役前即失败。
  写能力唯一权威 = `e3d_io`(001 US2/US5:COW + 新会话 + verify_commit + batch)。
- **v1 B 树搜索的去重/哨兵特判/"超界选最后"启发式**是 *null 终止读取*(`read_refno_data_loc`
  读到 0 即停——findings §16 已证伪的解读)的补偿;word6 界定遍历下不需要(T204 parity 结论)。
- engine_v2 的页大小探测与 v1 `io.rs` 逐行重复(知识漂移实锤,research §1.1)——收敛动机之一。

## 4. 能力去向(C4 验收对照)

| 删除项 | 能力去向 |
|---|---|
| `src/engine_v2/**` + `verify_engine_v2` bin | 读=`e3d_io`(`Edb`/`Rdb`);写=`e3d_io`(001);对照知识=本文 §1 |
| `src/writer.rs` + `element_serializer.rs` + `test_write_integration.rs` | 写能力=`e3d_io` COW(001 US2/US5);0x1C/哨兵陷阱=本文 §2 |
| `src/page_manager.rs` + `paged_reader.rs` | LRU/命中统计=`e3d_io::page_source::PagedFile`(+`Rdb` 影子页统计承接 `cache_hit_rate`) |
| `src/element_record_reader.rs` | 变长记录定界=`e3d_io::read_view::Rdb::element_record`(T205 parity) |
