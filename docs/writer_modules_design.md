# 写回模块化拆分与依赖设计

本文档定义“写回新 Element + core.dll 回归”所需的模块拆分、依赖关系，以及各模块的输入/输出与不变量（有序性、容量、page_size 适配）。实现时可分步迁移现有 `writer.rs` / `page_manager` / `element_record_reader` / `element_serializer` 到对应模块。

---

## 1. 模块总览与依赖图

```
                    ┌─────────────────┐
                    │ ffi/core_dll    │  回归：调用 core.dll 读属性，输出 JSON
                    └────────┬────────┘
                             │ 依赖（仅测试/对比）
        ┌────────────────────┼────────────────────┐
        ▼                    ▼                    ▼
┌───────────────┐   ┌─────────────────┐   ┌─────────────────┐
│ storage/page  │   │ storage/session │   │ storage/index/  │
│ 页 I/O+缓存   │   │ 会话页构建/提交 │   │ refno_btree     │
└───────┬───────┘   └────────┬────────┘   └────────┬────────┘
        │                    │                      │
        │    ┌───────────────┴───────────────┐      │
        │    │ 依赖                          │      │
        ▼    ▼                               ▼      ▼
┌───────────────────────────────────────────────────────────┐
│ storage/element/record                                     │
│ 元素记录 读/写 同构（ElementRecordReader + EleSerializer）  │
└───────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────┐
│ 文件头更新    │  （可放在 session 或独立 header 子模块）
│ HeaderUpdater │
└───────────────┘
```

- **storage/page**：底层页读写与缓存，被 session、index、element/record 共同依赖。
- **storage/session**：会话页构建与提交，维护 index_root / claim / end_pgno，依赖 page；提交时可能触发 header 更新。
- **storage/index/refno_btree**：RefNo B+树 解析/序列化、search、insert、split；依赖 page。
- **storage/element/record**：元素记录读取（现有 ElementRecordReader）与序列化/跨页写入（EleSerializer 对齐）；依赖 page。
- **ffi/core_dll**：仅用于回归测试，调用 core.dll 按 RefNo 读属性并输出 JSON，不参与写回链路。

---

## 2. 各模块定义

### 2.1 `src/storage/page/`

**职责**：页面 I/O、缓存，统一“页头”解析与校验（page_type、data 子类型等）。

| 项 | 说明 |
|----|------|
| **输入** | 文件路径或 `File`、`ext_no`、`pgno`、`page_size`（构造时固定） |
| **输出** | 单页原始字节 `Vec<u8>`；写端：接受整页字节写回指定 (ext_no, pgno) |
| **不变量** | 单页长度 = `page_size`（512 / 2048 / 4096）；缓存键 (ext_no, pgno) 唯一；读出的前 4 字节与可选 4 字节子类型与 `PageType`/`DataPageSubtype` 一致时可校验 |
| **迁移** | 将现有 `PageManager`、`PagedReader` 的页读取与缓存迁入；写页接口与 `writer.rs` 中 `write_page` 对齐，便于被 session/index/element 调用 |

### 2.2 `src/storage/index/refno_btree/`

**职责**：RefNo B+树索引页的解析与序列化（按 2.10 布局）；提供 `search(refno) -> Option<(sesno, offset)>`；提供 `insert(refno, loc)`，支持叶子有序插入、节点满时 split、根满时 split_root。

| 项 | 说明 |
|----|------|
| **输入** | 根页号 `index_root_pageno`（及 ext_no）、`page_size`、读页回调（从 page 模块取页）；insert 时：`(refno, pgno, offset, flag)` |
| **输出** | search：`Option<(sesno, att_offset)>`；insert：新根页号（若发生 split_root）、否则不变 |
| **不变量** | 叶子内 `RefnoDataLoc` 按 (refno_0, refno_1) 有序；每页条目数 ≤ 容量（由 page_size 与 16B 条目标出）；根页 level 最大；父子指针/边界键与 core.dll db3 语义一致（以 2.10 逆向为准） |

### 2.3 `src/storage/element/record/`

**职责**：元素记录“读写同构”——读取端 `ElementRecordReader`（变长记录、跨页、padding/segment/end-marker）；写入端 `EleSerializer` 生成与读取规则一致的字节（含 0x00000007 padding、分段跨页），并支持跨页写入。

| 项 | 说明 |
|----|------|
| **输入** | 读：`(pgno, offset)`、`page_size`、取页回调；写：结构化元素（RefNo、TYPE、OWNER、NAME、members、显式/UDA 等）或已序列化首段 + 续段 |
| **输出** | 读：原始字节或解析后的 `EleData`（与 parse_pdms_db 一致）；写：写入 (ext_no, pgno, offset) 及可能的多页续写，返回首段位置与总长度 |
| **不变量** | 写出的记录可被本模块读回且被 `parse_raw_ele_data_with_info` 正确解析；跨页处使用 0x00000007 + segment 规则，与现有 reader 一致 |

### 2.4 `src/storage/session/`

**职责**：会话页的构建与提交；维护当前会话的 `index_root_pageno/index_root_extno`、`claim_pageno/claim_extno`、`end_pgno/end_extno`；提交时写会话页、更新文件头（latest_ses_pgno、stored_page_count 等）。

| 项 | 说明 |
|----|------|
| **输入** | `sesno`、`last_ses_pageno`、当前会话的索引根（由 refno_btree 提供）、claim 链表头（若维护）、数据区结束页号；可选 computer_name/comments |
| **输出** | 会话页字节（SessionPageData 序列化）；提交后得到新会话页号，并驱动 HeaderUpdater |
| **不变量** | 会话页中 `page_type == Session`；end_pgno 指向本会话最后数据页；index_root_* 指向当前 B+树根；与 db2/db5 文档/2.10 行为对齐 |

### 2.5 文件头更新（HeaderUpdater）

**职责**：更新 PdmsHeader（latest_ses_pgno、stored_page_count 等）；可在 session 提交时调用，或独立为 `storage/header`。

| 项 | 说明 |
|----|------|
| **输入** | 可写文件、新的 latest_ses_pgno、新的 page_count（或由文件 size 推算） |
| **输出** | 头部 64 字节（或实际头部长度）写回偏移 0 |
| **不变量** | 头部布局与 `PdmsHeader` 一致；page_size 与当前写入使用的 page_size 一致 |

### 2.6 `src/ffi/core_dll/`

**职责**：Windows 下 LoadLibraryW/GetProcAddress 封装；提供“按 RefNo 读属性并输出 JSON”的接口，供回归测试与 Rust 解析结果做字段级 diff。

| 项 | 说明 |
|----|------|
| **输入** | DB 路径、RefNo；可选：init/open 的配置 |
| **输出** | JSON 对象（属性名 → 值），便于与 Rust 侧 EleData→JSON 对比 |
| **不变量** | 仅读；不修改 DB；调用约定与 2.10 core.dll 导出一致 |

---

## 3. 跨模块不变量（全局）

- **page_size**：单库写入全程使用同一 page_size（2K/4K/512），与 PdmsHeader 及所有页头解析一致。
- **有序性**：RefNo B+树叶子内按 (refno_0, refno_1) 有序；插入时保持有序，满则 split。
- **容量**：单页数据区容量 = `page_size - page_header_size`；索引页条数 ≤ `(page_size - index_page_header) / 16`。
- **会话与索引根**：提交会话时，index_root_* 必须指向当前 RefNo 树的根；claim_* 若实现则与 db2 语义一致。

---

## 4. 实现与迁移顺序建议

1. **storage/page**：抽离 PageManager/PagedReader，统一读/写页接口与 page_size。
2. **storage/element/record**：在现有 Reader/Serializer 基础上，明确跨页写入规则并实现跨页写。
3. **storage/index/refno_btree**：实现有序 insert + split/split_root，与 2.10 页布局对齐。
4. **storage/session + HeaderUpdater**：commit 时写入 index_root、claim、end_pgno，并更新文件头。
5. **ffi/core_dll**：最小 API 封装与 JSON 输出，用于回归对比。

此顺序与计划中的“写回实现顺序”一致，便于分阶段验收（解析侧对齐 → core.dll 对齐）。
