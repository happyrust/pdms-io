# E3D 3.1 写回支持（草案）

> **状态**：DRAFT — 尚未经过 Plannotator gate；未启动执行。

## 目标结果

在 `e3d-reader`（或为写入分支创建的新 crate）上增加 **E3D 3.1 数据库的写回路径**：能在打开的数据库上 claim 元素、修改属性、create / delete 元素、recurse-write，并以与 `core.dll` 兼容的方式产出 `save_work` / session commit。最终交付 fixture 级 round-trip：用 Rust 写一次，用 `core.dll` 读出来一致。

## 背景

- 前序 goal `e3d31-rust-readonly-io` 已完成只读路径（B+tree 搜索 / record 字节读取 / session 解析）。
- IDA 已识别出写入侧函数家族：`db_save_work` / `db_claim_element` / `db_release_element` / `db_create_element` / `db_delete_element` / `FHDBWN`（页写入）/ `FHSPLT`（B+tree split）/ `FLOPEN(WRITE)` 等。
- 现有 `crates/pdmsdb_engine_v2/` 与 `crates/parse_pdms_db/` 在 2.10 时代实现过部分写回逻辑，但 3.1 常量、字节序、page_size 单位等已被证实与 2.10 不同，**不可直接继承**。

## 约束

- 基线必须仍是 E3D 3.1，所有偏移 / 常量回溯 `ida_exports/3.1/struct_layouts.json`。
- 字节序：BE；page_size 单位：words × 4。
- 与 `core.dll` 的写入路径**保持二进制兼容**——验证标准是 `core.dll` 能正常打开 Rust 写出的库并通过其内部校验（不是 Rust 自我校验）。
- 写入前必须 claim；离开必须 release。
- session commit 与 header 更新必须原子完成；不允许中断后留下半提交状态。

## 非目标

- 实时 `core.dll` FFI oracle（属于 `e3d31-coredll-ffi-oracle` goal，但**本 goal 的验收会调用它**）。
- multi-extent 写入与 Extract 语义（→ `e3d31-multi-extent`）。
- UDA / 完整属性 schema 写回（→ `e3d31-attribute-parsing`）。
- 性能优化（先正确，再快）。

## 已决策（2026-05-11 plannotator gate）

- crate 命名：把现有 `e3d-reader` 重命名为 **`e3d-io`**，读写共存。本 goal Slice 0 即重命名 + 跨仓库引用同步（参见 `goals/e3d31-rust-readonly-io/CRATE_LOCATION.md`）。

## 需要先询问

- B+tree split / merge 实现风险高，是否允许"先只支持 in-place update（不触发分裂）"作为里程碑 0。
- session commit 失败后的恢复策略（rollback 还是 fail-fast 留待人工）。

## 完成定义

- Rust 能在 fixture 上原子修改至少 1 个元素属性并 commit。
- 经 `core.dll`（通过 `e3d31-coredll-ffi-oracle` goal）读取，结果与 Rust 视角一致。
- 至少覆盖 claim / release / single attribute update / create / delete 五种写操作的最小用例。
- B+tree split 至少有 1 个用例触发并保留正确性。
