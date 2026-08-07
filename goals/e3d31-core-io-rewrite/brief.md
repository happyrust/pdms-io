# E3D 3.1 Core IO 重写

## 目标结果

从 IDA 证据中恢复 E3D 3.1 `core.dll` 的数据库数据结构，并基于这些结构设计一个干净的 Rust 只读 IO 核心。第一个可执行实现目标不是完整写回，而是验证从数据库文件到 `RawRecord` 和结构化 `ElementRecordView` 的读取路径。

## 背景

- 当前 IDA MCP 连接的是 `D:\AVEVA\Everything3D3.1\core.dll.i64`。
- 仓库内已有文档描述了 `db1` 到 `db5` 的分层架构，但许多资料来自早期 E3D/core.dll 假设。它们只能作为背景材料，不能作为 E3D 3.1 的最终真值。
- 初步 IDA 检查没有发现已命名的 E3D 领域结构体，例如 `db`、`page`、`session`、`element`、`index`；当前 IDA 类型表主要是 Windows/CRT 类型。因此结构恢复必须成为明确的第一阶段。
- `crates/pdmsdb_engine_v2/` 下的现有 Rust 代码可作为对照材料，但新架构不能在没有 IDA 或 fixture 证据时继承它的假设。
- 后续目标是在一个新的 Rust IO 库中，按照恢复出的 `core.dll` 读写规则实现 E3D 数据库文件 IO。

## 约束

- 本目标的基线版本是 E3D 3.1，不是 E3D 2.10。
- Phase 0 必须先恢复结构与证据，再设计 Rust API。
- Phase 1 必须保持只读：打开数据库、探测 page size、回溯 session、搜索 RefNo 索引、读取原始 element record、暴露结构化 view。
- 第一阶段不实时调用 `core.dll`。使用 IDA 证据、已有 fixture 和生成的分析产物。
- 新 Rust 架构必须独立于旧 `PdmsIO`、`writer.rs`、`parse_pdms_db` 实现细节。旧代码只能作为参考材料或对照基线。
- 每个恢复出的字段偏移都必须引用证据：IDA 函数、常量、栈/成员访问、xref 或 fixture 字节观测。
- 最终产物中避免使用无证据的结构命名。若命名或字段含义不确定，必须标注置信度和证据。

## 非目标

- 本目标不实现完整写回、`save_work`、claim/release、refresh、multiwrite merge 或 compact。
- 本目标不构建实时 `core.dll` FFI harness。
- 不把旧 Rust 实现整体搬进新 workspace。
- 除非恢复出 IDA 证据并更新验收标准，否则不声明支持 multi-extent 或 Extract 语义。
- 本目标不解析所有 E3D 属性类型或 UDA 行为；先证明底层读取路径和 record 结构。

## 需要先询问

- 改变 E3D 3.1 目标基线前必须询问。
- 做破坏性 IDA 修改、批量重命名或给大量函数套用猜测类型前必须询问。
- 在仓库外创建新 workspace 前必须询问。
- 添加依赖或确定会阻碍后续写入支持的 crate 布局前必须询问。
- 将低置信度 IDA findings 当作结构真值前必须询问。

## 完成定义

完成意味着该 goal 包有通过审阅的计划，并且执行目标清晰：把 E3D 3.1 数据库结构恢复为已审阅的 Markdown 和机器可读 JSON 产物，然后围绕这些结构设计只读 Rust IO 核心，并写清具体验证命令与停止条件。
