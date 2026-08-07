# E3D 3.1 Rust 只读 IO Crate 实现

## 目标结果

基于前序 goal（`e3d31-core-io-rewrite`）恢复的 E3D 3.1 数据库结构，实现一个 Rust 只读 IO 库 `e3d_reader`。该库能打开 E3D 3.1 数据库文件、探测 page size、回溯 session 链、按 RefNo 搜索 B+tree 索引、读取原始 element record、并暴露结构化 `ElementRecordView`。

## 背景

- 前序 goal 产出了完整的结构恢复产物：
  - `ida_exports/3.1/db_functions.json` — 85 个函数 / 7 层映射
  - `ida_exports/3.1/struct_layouts.json` — 13 个结构定义
  - `docs/ida-3.1-structures.md` — 17 章节技术文档，含架构设计（第 13 章）
- 架构设计已在 `docs/ida-3.1-structures.md` 第 13 章定义：8 模块、6 API、9 非目标
- 现有 `crates/pdmsdb_engine_v2/` 可作为对照参考，但新实现必须从 3.1 IDA 证据驱动

## 约束

- 基线版本是 E3D 3.1，不是 2.10
- 所有结构偏移必须追溯到 `struct_layouts.json` 中的 IDA 证据
- 魔术常量使用 3.1 值（7618377, 13387743 等），不使用 2.10 值
- 只读：不实现写回、save_work、claim/release、refresh、compact
- 字节序：**大端 (BE)**——数据库文件沿用 Fortran 大端约定，与 `docs/ida-3.1-structures.md` §1、§13.5 一致；fixture `ams1112_0001` 字节实测确认（修正于 2026-05-11，原 brief 误标为小端）
- page_size 单位：descriptor 偏移 0x34 处存储的是 **4 字节字数**（words），实际字节数 = words × 4；fixture 实测 512 words = 2048 字节（修正于 2026-05-11，初始 brief 未明示单位）
- 依赖管理：最小化外部依赖，优先使用标准库；目前最小集为 `thiserror = "2"` + `nom = "8"`（解析），任何超出此集合的依赖需先审批

## 非目标

- 写回、session commit、claim/release、refresh、compact、multiwrite merge（→ goal `e3d31-writeback`）
- 实时 core.dll FFI 调用 / oracle（→ goal `e3d31-coredll-ffi-oracle`）
- **完整的属性类型解析与 UDA 解析**（→ goal `e3d31-attribute-parsing`）；`ElementRecordView` 在本 goal 内只暴露原始字节 + 已识别的 hash/owner 等少量字段，不解析 attribute schema
- Multi-extent 支持 / Extract 语义（→ goal `e3d31-multi-extent`）
- 与旧 PdmsIO/writer.rs 的兼容层
- GUI 或 CLI 工具（本 goal 只产出库）

> 实现期间 `e3d-reader/src/record/attrs.rs` 与 `e3d-attlib` 依赖被引入，事后被认定**已越过本 goal 的非目标边界**。它们暂留在 e3d-reader 内，但**不在本 goal 的验收范围**；后续将迁移到 `e3d31-attribute-parsing` goal，参见 `CRATE_LOCATION.md` 与该 goal 的 brief。

## 需要先询问

- 创建新 crate 目录结构前确认放置位置
- 选择依赖（thiserror 版本等）前确认
- 修改 workspace Cargo.toml 前确认
- 发现 struct_layouts.json 中的偏移与实际 fixture 字节不匹配时暂停

## 完成定义

完成意味着 `e3d_reader` crate 能通过编译，并且至少一个 fixture 数据库文件可以被打开、元数据可以被读取、至少一个已知 RefNo 的元素可以被定位和读取。
