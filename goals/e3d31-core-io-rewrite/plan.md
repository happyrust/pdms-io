# 计划：E3D 3.1 Core IO 重写

## 方案概览

本目标为新的 Rust 数据库 IO 架构做准备：先恢复 E3D 3.1 `core.dll` 中必须遵守的数据结构，再设计 Rust 只读 IO 核心。工作从 IDA 开始，而不是从 Rust 代码开始：识别数据库层函数，从真实函数证据中恢复 page/session/index/record 布局，并输出人工可读和机器可读的结构产物。结构产物可信后，再设计一个只读 Rust 核心，用于打开 E3D 数据库、按 RefNo 定位 record、跨页读取 raw record，并暴露结构化 element view。

## 为什么这样做

现有文档已经描述了有价值的 `db1` 到 `db5` 模型，但当前目标已经改为 E3D 3.1，而且当前 IDA 数据库没有暴露现成的领域结构体。如果先写 Rust，就会把猜测写进架构。先恢复结构可以让后续 Rust 模型由证据驱动：字段偏移、record 边界、page header、B+tree entry、session 链都能追溯到 IDA 和 fixture 字节。

第一个 Rust 交付物刻意限定为只读。写回需要同时正确处理 record 分段、B+tree split、session commit、header 更新、claim 和 `save_work`。这些应在读取路径和结构模型稳定后作为后续目标处理。

## 工作方式

工作分为两条相互连接的线。

IDA 恢复线使用 `user-ida-pro-mcp` 分析 `D:\AVEVA\Everything3D3.1\core.dll.i64`。它先通过函数名、字符串、调用图、常量、imports，以及与已有 `db1` 到 `db5` 职责模型的相似性，定位候选数据库函数。随后恢复这些函数操作的结构：物理 page/cache 记录、数据库 header/session 结构、RefNo index entry 与 index page、element record 布局，以及高层 open/save 状态。每个恢复出的字段都必须带证据和置信度。

架构设计线把恢复出的布局转化为 Rust 设计。建议模块划分为：`page` 负责物理页和 extent，`meta` 负责 header/session/db block 元数据，`index` 负责 RefNo B+tree 搜索，`record` 负责 raw element record 读取和结构化 record view，`engine` 负责只读 public API。旧仓库代码可以作为背景引用，但新 API 应由恢复出的 E3D 3.1 布局塑形。

## 工作切片

| Slice | Purpose | Main files or systems | Done when | Risks |
| --- | --- | --- | --- | --- |
| 1 | 建立 E3D 3.1 IDA 基线 | `user-ida-pro-mcp`、`docs/e3d 数据库分析/`、`ida_exports/` | 已记录 active IDA 实例、binary 版本、现有命名/类型、缺失结构清单 | 当前 IDB 可能缺少命名或类型；2.10 文档可能误导 |
| 2 | 恢复数据库函数映射 | IDA functions、xrefs、strings、call graphs、`ida_exports/3.1/db_functions.json` | 候选 `db1` 到 `db5` 函数带地址、职责、置信度和证据记录 | 函数名可能是 Fortran 风格或被优化 |
| 3 | 恢复结构布局 | IDA type/stack/member 分析、常量、fixtures、`ida_exports/3.1/struct_layouts.json`、`docs/ida-3.1-structures.md` | page/header/session/index/record 结构包含偏移、大小、字段含义和证据 | 部分字段可能未知，必须标注不确定 |
| 4 | 设计只读 Rust 核心 | 新 workspace 设计说明、`docs/ida-3.1-structures.md` 中的架构章节或后续设计文档 | 模块边界、核心结构、API、数据流都由恢复布局定义 | 可能过度拟合不完整结构 |
| 5 | 定义验证 fixture | 现有样本 DB、JSON 输出、`verification.md`、`progress.jsonl` | 命令和预期产物能证明 open/session/index/record 读取路径 | Phase 1 不实时调用 `core.dll` oracle |

## 执行顺序

- Slice 1 必须最先执行，因为当前 IDA 状态决定能否直接恢复结构，还是需要先做更多命名工作。
- Slice 2 阻塞可靠的结构恢复，因为函数是偏移和状态转换的证据容器。
- Slice 3 阻塞 Rust 架构，因为恢复出的布局定义 parser 和 public data model。
- Slice 4 可在关键读取路径结构恢复后开始，即使低置信度写入路径结构先延后。
- Slice 5 全程进行，但最终验证必须等读取路径结构和设计都能追溯到产物后才算通过。

## 阶段边界

- 本目标在 E3D 3.1 结构恢复产物和只读 Rust 架构计划通过审阅后结束。
- Rust 只读 IO crate 的实现应创建新 goal。
- 写回、B+tree insertion/split、session commit、claims、refresh、`save_work` 应创建后续 goal。
- 实时 `core.dll` FFI/oracle 集成应创建单独 goal。

## 方向控制

- 优先使用带证据的命名，而不是看起来漂亮但缺少证据的命名。
- 对未知字段显式标注，不要用猜测填满。
- 架构应兼容后续写入支持，但本目标不实现写入。
- 执行前必须用 Plannotator gate 审阅关键计划文档。

## 验收标准

- [ ] 已记录 active IDA target 和 E3D 3.1 基线，包括初始状态下没有现成 E3D 领域结构体这一事实。
- [ ] `ida_exports/3.1/db_functions.json` 存在，并记录候选数据库层函数的地址、层级、职责、置信度和证据。
- [ ] `ida_exports/3.1/struct_layouts.json` 存在，并记录读取路径所需布局：header、session page、index entry/page、data page header、record segment、element record view。
- [ ] `docs/ida-3.1-structures.md` 用人工可读形式解释每个恢复出的结构，并为偏移和不确定字段引用证据。
- [ ] 已写清只读 Rust 架构，包括模块、API、数据流和明确非目标。
- [ ] 验证证据已追加到 `goals/e3d31-core-io-rewrite/progress.jsonl`。

## 必需证据

| Requirement | Evidence to inspect | Where evidence is recorded |
| --- | --- | --- |
| IDA 基线 | `list_instances`、type/function query 摘要 | `progress.jsonl`、`docs/ida-3.1-structures.md` |
| 函数映射 | IDA function/xref/callgraph 输出 | `ida_exports/3.1/db_functions.json` |
| 结构布局 | IDA decompile、stack/member offset、常量、fixture 字节检查 | `ida_exports/3.1/struct_layouts.json`、`docs/ida-3.1-structures.md` |
| 只读架构 | 模块/API/数据流设计 | `docs/ida-3.1-structures.md` 或后续架构文档 |
| 验证 | 命令运行结果和通过/失败状态 | `verification.md`、`progress.jsonl` |

## 完成审计

在标记 goal 完成前，Codex 必须把每个明确需求、文件、命令、检查和交付物映射到真实证据。如果任何项目缺失、不完整、验证薄弱或仍不确定，则 goal 不能完成。
