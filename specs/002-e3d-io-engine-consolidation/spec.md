# Feature Specification: E3D I/O 三引擎收敛（单一格式核心）

**Feature Branch**: `002-e3d-io-engine-consolidation`

**Created**: 2026-06-10

**Status**: Implemented（2026-06-11;Phase 0–4 全清,SC-001~006 核销见 tasks.md;grill-me 决策记录与证据见 research.md）

**Input**: User description: "分析当前是如何读写 e3d 数据库文件的 → 使用 grill-me 分析，然后编写 spec kit"

> 说明：本仓库当前**三套 E3D 读写实现并存**——`PdmsIO`(v1, `src/io.rs` 族)、`engine_v2`(`src/engine_v2/` db1~db5 分层)、`e3d_io`(`crates/e3d_io`, 已被 001 规范并双实现验证)。页缓存、B 树、元素记录解析三处重复，`engine_v2` 与 v1 写路径均为零生产调用方的孤岛。本规范定义把仓库收敛到**以 `e3d_io` 为唯一格式核心**的目标态。格式字节级规则**不在本文重复**——一律引用 `specs/001-e3d-data-format/`（data-model.md / decode-contract.md），避免双源漂移。

## User Scenarios & Testing *(mandatory)*

### User Story 1 - 单一格式核心 (Priority: P1)

维护者修复或扩展任何格式行为（B 树、元素记录、会话链、页大小探测）时，希望**只存在一处实现**需要修改与验证：`crates/e3d_io`。不再出现"v1 修了、engine_v2 没修、e3d_io 行为不同"的三体问题。

**Why this priority**: 这是本次收敛的存在理由。三处重复直接造成知识漂移与验证成本三倍。

**Independent Test**: 对仓库做实现盘点：B 树搜索/插入、元素记录读取/序列化、页缓存各自只剩**一份**非测试实现；`src/` 下不再有与 `e3d_io` 平行的格式解析代码。

**Acceptance Scenarios**:

1. **Given** 收敛完成后的仓库, **When** 检索 B 树叶搜索 / 元素记录边界判定 / base-27 哈希的实现, **Then** 命中点全部位于 `crates/e3d_io`（`PdmsIO` 仅有委托调用）。
2. **Given** 一处格式行为修复落在 `e3d_io`, **When** 重新构建, **Then** `PdmsIO` 门面与 CLI/导出路径自动获得同一修复，无需第二处改动。

---

### User Story 2 - `PdmsIO` 门面 API 兼容换芯 (Priority: P1)

仓库内 30+ 处调用方（tests/bins/`watch.rs`/`main.rs`）依赖 `PdmsIO` 的公共 API（`new/open/read_pdms_header/collect_increment_eles/build_index_map/search_latest_refno/...`）。换芯后这些调用方 MUST **零修改**编译通过且行为不变。

**Why this priority**: API 破坏会把"引擎收敛"演变成"全仓库重写"，爆炸半径不可控（尤其已选定大爆炸切换，见 Assumptions）。

**Independent Test**: 不修改任何调用方源码，仅替换 `PdmsIO` 内部实现后 `cargo build --workspace` 通过、现有 `cargo test` 全绿。

**Acceptance Scenarios**:

1. **Given** 换芯后的 `PdmsIO`, **When** 运行现有全部单元/集成测试（`src/test/`、`src/tests/`、`tests/`）, **Then** 全绿，断言值与换芯前一致。
2. **Given** `contracts/api-compat-contract.md` 列出的公共 API 清单, **When** 对照换芯后的 `PdmsIO`, **Then** 签名逐项保持（允许内部类型私有化，不允许公共签名变更）。

---

### User Story 3 - 页源抽象：大文件流式不退化 (Priority: P2)

增量 watcher / 大库场景（`ams1112` 103MB、数十万元素级）的使用者希望换芯后**按页读取 + LRU 缓存**的能力不退化：`e3d_io` 通过极小的 `PageSource` 抽象取页，整文件 buffer（现状）与分页文件缓存（吸收 `PageManager` LRU）是同一格式核心下的两个可替换页源。

**Why this priority**: `e3d_io` 现为整文件 `fs::read` 模型；若不抽象页源，watcher 路径要么退化为全量重读、要么被迫保留 v1 重复实现——后者直接违背 US1。

**Independent Test**: 同一库分别经 `InMemory` 与 `PagedFile` 两个页源解码，逐元素逐属性结果一致；`PagedFile` 路径单次增量扫描的页读取数远小于全文件页数（缓存命中可观测）。

**Acceptance Scenarios**:

1. **Given** `sam7200_0001`, **When** 分别以两种页源全库枚举, **Then** 元素计数与属性值逐项一致。
2. **Given** 页大小字段说谎的样本（`ams1112_0001` 声明 512 实为 2048）, **When** 经任一页源打开, **Then** 页大小探测（probe `page_type==Session`）行为保持，解码正常。
3. **Given** watcher 增量场景反复读取最新会话, **When** 使用 `PagedFile` 页源, **Then** LRU 命中率/读页计数可观测，不发生整文件重读。

---

### User Story 4 - 孤岛退役与知识归档 (Priority: P2)

维护者希望零调用方的死代码离开构建图：`engine_v2`（仅 `bin/verify_engine_v2.rs` 引用）与 v1 写路径（`writer.rs` + `element_serializer.rs`，仅自身测试引用）退役删除；其中有价值的逆向知识（db1~db5 与 core.dll 的分层对照、`INDEX_PAGE_HEADER_SIZE=0x1C` 等陷阱）以文档形式归档，不随代码消失。

**Why this priority**: 死代码是误导源（三套 B 树插入实现并存）；但退役只删实现，不删知识。

**Independent Test**: `lib.rs` 不再声明 `engine_v2`/`writer`/`element_serializer`；`cargo build --workspace` 通过；归档文档存在并涵盖 db1~db5 分层映射表。

**Acceptance Scenarios**:

1. **Given** 退役完成, **When** 构建全 workspace 与全部 bins, **Then** 无引用残留、构建通过。
2. **Given** `docs/`(或 research.md) 归档, **When** 查阅 db5_save_work / db1_get_page 等 core.dll 对照知识, **Then** 在文档中可检索到（含原 `engine_v2` 模块到 core.dll 函数的映射）。

---

### Edge Cases

- **页大小字段说谎**（`ams1112_0001` 声明 512 实为 2048）：探测逻辑（probe `pgno*page_size` 处 `page_type==Session(3)`）MUST 在新核心中保持；该样本为回归必测样本。
- **`ext_no` 语义**：v1 `PdmsIO` 以单文件为输入、`ext_no` 恒 0（`local_file_ext_no`）；`PageSource` 抽象 MUST 不引入多 extent 假设（多 extent 支持显式列为范围外）。
- **索引缓存文件兼容**：`cache_index_map`/`load_cached_index_map` 的磁盘缓存格式 MUST 不变，旧缓存文件在换芯后仍可加载（或显式版本失配重建，二选一并写入契约）。
- **会话边界语义**：`init_ses_maps` 的会话页号范围推导（prev_end+1 起算）MUST 语义等价迁移，增量提取的会话归属不得漂移。
- **`e3d_io` std-only 红线**：`PageSource` 抽象与 `PagedFile` 实现 MUST 不给 `crates/e3d_io` 引入任何第三方依赖（001 FR-017 约束延续）。

## Requirements *(mandatory)*

### Functional Requirements

**核心收敛（P1）**
- **FR-001**: 仓库 MUST 收敛为单一格式核心：B 树（搜索/枚举/插入）、元素记录（读取/边界/序列化）、会话链、头部解析、页大小探测的实现 MUST 仅存在于 `crates/e3d_io`。
- **FR-002**: `PdmsIO` MUST 保留为上层门面：公共 API 签名不变（清单见 `contracts/api-compat-contract.md`），内部全部委托 `e3d_io`；其自有的格式解析私有实现 MUST 删除。
- **FR-003**: `parse_pdms_db` 对元素数据的解析职责 MUST 经评估后并入或委托 `e3d_io`（若 `EleData` 结构被广泛依赖，允许保留为纯类型适配层，但 MUST 不含独立的字节解析逻辑双份）。

**页源抽象（P2）**
- **FR-004**: `e3d_io` MUST 引入最小 `PageSource` 抽象（按 `(ext_no, pgno)` 取页字节 + 页大小），格式逻辑 MUST 仅经由该抽象取页。
- **FR-005**: MUST 提供两个页源实现：`InMemory`（整文件 buffer，等价现状、CLI/测试默认）与 `PagedFile`（文件句柄 + LRU 页缓存，吸收 `PageManager` 的容量/驱逐/统计语义）。
- **FR-006**: `PagedFile` MUST 提供可观测的缓存统计（命中/未命中/驱逐计数），等价替代 `PageManager::CacheStats` 既有能力。

**孤岛退役（P2）**
- **FR-007**: `engine_v2` 模块 MUST 从 `lib.rs` 与构建图移除，`bin/verify_engine_v2.rs` 一并删除；其 core.dll 分层对照知识 MUST 归档至文档。
- **FR-008**: v1 写路径 `writer.rs`、`element_serializer.rs` 及其集成测试 MUST 退役删除；仓库内写能力 MUST 仅由 `e3d_io`（001 US2/US5 所规范）提供。
- **FR-009**: `page_manager.rs`/`paged_reader.rs`/`element_record_reader.rs` MUST 在其能力被 `PageSource`/`e3d_io` 等价吸收后删除。

**行为保持（P1）**
- **FR-010**: 增量提取（`collect_increment_eles` 族）、历史检索（`collect_ele_history`/`search_latest_refno` 族）、索引映射（`build_index_map` 族）MUST 在新核心上语义等价，现有测试断言不变。
- **FR-011**: 页大小探测 MUST 保持"header 字段不可信，以 `page_type==Session` 实测为准"的语义。
- **FR-012**: 切换策略为**大爆炸**（无新旧双跑对照期）：合入门槛 MUST 为 workspace `cargo build` + 全部现有 `cargo test` 全绿 + `crates/e3d_io` 独立测试全绿；不设逐属性新旧 diff 闸门（决策 Q4=C，风险声明见 Assumptions）。

**范围外（明确不做）**
- **FR-013**: 本规范 MUST NOT 触及：SurrealDB 落库（`update_elements_to_database` 维持 no-op）、Meilisearch 索引、`sync`/远端分发、`PdmsIO` 门面透出写/事务编辑 API（均留待 003+）；多 extent 文件支持亦为范围外。

### Key Entities

- **格式核心 (`e3d_io`)**：唯一的字节级读写实现；001 的全部 FR/SC 继续约束它。
- **`PageSource`**：页源抽象——`(ext_no, pgno) → 页字节` + 页大小；格式核心与物理 I/O 的唯一边界。
- **`InMemory` 页源**：整文件 buffer；零成本等价现状。
- **`PagedFile` 页源**：文件句柄 + LRU 缓存（容量/驱逐/统计承接 `PageManager` 语义）。
- **`PdmsIO` 门面**：API 兼容层——缓存、会话范围、增量提取编排；不含格式字节逻辑。
- **退役孤岛**：`engine_v2`、`writer.rs`、`element_serializer.rs`、`page_manager.rs`、`paged_reader.rs`、`element_record_reader.rs`。

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 实现盘点归零：B 树/元素记录/页缓存的非测试实现各仅 **1 份**（`e3d_io` 内）；`src/` 不再存在平行格式解析。
- **SC-002**: `PdmsIO` 全部既有调用方**零修改**编译通过；现有测试套件全绿，断言值与收敛前一致。
- **SC-003**: 双页源一致性：`sam7200_0001` 经 `InMemory` 与 `PagedFile` 全库枚举，元素计数与属性值**逐项一致**；`ams1112_0001`（页大小说谎样本）两路均正常解码。
- **SC-004**: 代码净删除：`engine_v2`（~25 文件）+ v1 写路径 + 三个 v1 读取辅助模块全部移出构建图，workspace 构建通过。
- **SC-005**: `crates/e3d_io` 保持 std-only 且独立 `cargo test` 全绿（001 SC-005 延续）。
- **SC-006**: 增量 watcher 路径在 `PagedFile` 页源下读页计数 < 全文件页数（缓存生效可观测），无整文件重读退化。

## Assumptions

- **Q4=C 大爆炸切换已知风险（用户拍板，2026-06-10）**：不做新旧实现双跑逐属性对照；`PdmsIO` 深层路径（索引缓存、会话边界、异常页大小）若现有测试未覆盖，回归只能事后发现。退路为 git 历史回滚。缓解：`ams1112_0001` 列为必测样本、现有测试全绿为硬闸门。
- 格式正确性的权威与持续保障来自 001（双实现属性级对齐、`e3d_io` 测试套件），002 不重复建立格式真相。
- `pdms_io` 整 crate 端到端构建仍受 `rs-core ↔ surrealdb-3.1` API 漂移影响（001 已记录）；收敛工作以不恶化该现状为约束。
- 基线版本同 001：AVEVA Everything3D 2.10。
