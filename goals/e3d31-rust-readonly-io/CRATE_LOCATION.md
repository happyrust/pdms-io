# Crate 物理位置

本 goal 的执行产物 crate 实际位于 **`D:/work/plant-code/e3d-io/`**（独立仓库），**不在**本仓库 `pdms-io-fork-engine-v2/crates/` 下。

> **重命名记录**：2026-05-11 已把仓库与包名 `e3d-reader` → `e3d-io`（作为 `e3d31-writeback` Slice 0 落地，决策来自 plannotator gate）。仓库路径同步迁移：`D:/work/plant-code/e3d-reader/` → `D:/work/plant-code/e3d-io/`；包名 `Cargo.toml::package.name` 从 `e3d-reader` → `e3d-io`；Rust 模块路径从 `e3d_reader::*` → `e3d_io::*`。`tests/read_real_db.rs` 已同步更新；`cargo check` / `cargo test --no-run` 通过。

## 历史背景

- `progress.jsonl` 第 2 行（`all_slices_complete`）的 `files_created` 列表使用了前缀 `crates/e3d_reader/...`，与实际位置不符。该记录保留作为原始执行日志，不再修改文件路径字段；本说明用于澄清。
- 仓库布局策略：经 2026-05-11 进度审查决策为 **C（保持双仓库 + 交叉引用）**，未将 e3d-reader 纳入 `pdms-io-fork-engine-v2` workspace。

## 跨仓库交叉引用

- 上游：本目录 + `docs/ida-3.1-structures.md` §13 + `ida_exports/3.1/struct_layouts.json`
- 下游：`D:/work/plant-code/e3d-io/README.md`（原 `e3d-reader/README.md`）

## fixture 位置

`D:/work/plant-code/pdms-io-fork/test-file/ams1112_0001`（位于第三个兄弟仓库 `pdms-io-fork`）。

## 命名差异（重命名后）

- crate 包名：`e3d-io`（含连字符，`Cargo.toml`，2026-05-11 改）
- 模块路径在 Rust 内：`e3d_io::...`（下划线）
- 仓库文件夹：`D:/work/plant-code/e3d-io/`
- 历史名（保留作为查找词）：`e3d-reader`、`e3d_reader`、`crates/e3d_reader/`（原 progress.jsonl 第 2 行的 `files_created` 仍引用旧名作为执行日志）。

## 待办

- 任何把 crate 重新整合进本 workspace 的提案，需要新开一个独立 goal package，并在那里完成 plannotator gate。

## crate 重命名（2026-05-11 完成）

- 重命名 `e3d-reader` → `e3d-io` 已在 `e3d31-writeback` Slice 0 落地完成（文件夹 / 包名 / 测试导入 / README / 本文件全部同步）。
- 后续 goal `e3d31-attribute-parsing` 已决策：在 `e3d-io` 之外，重写 `D:/work/plant-code/e3d-attlib/` 作为属性解析层。
- 进度记录见 `goals/e3d31-rust-readonly-io/progress.jsonl` 与 `goals/e3d31-writeback/progress.jsonl`。
