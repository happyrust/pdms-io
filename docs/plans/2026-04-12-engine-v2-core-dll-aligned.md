# Engine V2 Core.dll 2.10 对齐计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 在独立 worktree 中实现一套与 `core.dll 2.10` 语义对齐的全新 PDMS 数据库读写引擎。

**Architecture:** 新引擎以 `db1/db2/db3/db4/db5` 五层重新建模，旧 `PdmsIO` 与 `writer` 仅保留作 oracle 与回归基线。第一阶段只做只读链，先打通打开、会话解析、RefNo 搜索与完整 record 读取。

**Tech Stack:** Rust 2024, `anyhow`, `thiserror`, `aios_core`, `pdms_io`(仅测试/compare), `std::fs::File`

---

## 冻结决策

- 真值基线固定为 `Everything3D 2.10 core.dll`
- 新引擎 crate 固定命名为 `pdmsdb_engine_v2`
- 新引擎实现位于 `crates/pdmsdb_engine_v2/`
- 第一阶段仅支持单文件 `ext_no = 1`
- 旧实现只允许在 compare/tests 中访问

