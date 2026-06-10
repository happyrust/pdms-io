# Implementation Plan: E3D / PDMS DABACON 数据格式离线读写规范

**Branch**: `001-e3d-data-format` | **Date**: 2026-06-08 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/001-e3d-data-format/spec.md`

## Summary

把已逆清并双实现验证的 E3D/PDMS DABACON **离线读 + 写**能力固化为规范与可维护实现。技术路径:以模式库 `*vir.dat` type-def 为 offset 权威源做隐式属性解码,沿会话链 + B 树枚举元素,解析 DA/成员/UDA/引用;写侧以 COW + 新会话(`db5_save_work` 语义)实现全 CRUD。核心为 std-only Rust crate(`crates/e3d_io`)+ Python 参考工具链,二者按真实库属性级对齐。

在已验证的写原语之上,**US5(安全事务化编辑层,Phase 7)** 规划"多笔合一会话 + 写后自校验(`verify_commit`)+ dry-run/diff + 安全护栏",对应 active plan `2026-06-07-e3d-offline-edit-safety-batch`(本环境可做、E3D-无关,测试先行)。

## Technical Context

**Language/Version**: Rust(edition 2024,std-only)+ Python 3.11+(参考实现/探针)

**Primary Dependencies**: 无第三方依赖(核心 std-only,刻意保持纯净);逆向取证用 IDA Pro(仅验证,非运行期)

**Storage**: 磁盘文件——元素库 `<proj><dbno>_0001`(2048B/页,大端)+ 模式库 `%AVEVA_DESIGN_EXE%/*vir.dat`

**Testing**: `cargo test`(`crates/e3d_io` 模块测试 + 真实样本集成测试,数据缺失则优雅跳过);Python 自检 demo(`e3d_write_full.py` S1–S8b)

**Target Platform**: 跨平台库 + CLI(开发验证于 Windows;无平台特定运行期依赖)

**Project Type**: library + CLI(离线数据 I/O),被 `pdms_io` 复用

**Performance Goals**: 全库遍历 ams1112(103MB / ~42.2 万元素)Rust ≤ 数秒;Python ~30s 量级可接受(参考实现)

**Constraints**: 纯离线(无运行中 E3D / IDA 运行期依赖);std-only;写默认仅副本;不擅改项目外 `rs-core`

**Scale/Scope**: 设计/目录/系统库 + ≥100MB 大库;单库数十万元素;20+ noun schema 库 / 1478 noun 类型

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

对照 `.specify/memory/constitution.md` v1.0.0:

| 原则 | 闸门 | 状态 |
|---|---|---|
| I. 纯离线/纯文件优先 | 交付物运行期不依赖 E3D 进程/IDA | ✅ 读写全程仅文件;IDA 仅取证 |
| II. 取证式逆向 | 每结论 = 反编译函数 + 真实字节 | ✅ 见 `research.md` 证据表(函数名+地址+样本) |
| III. 双实现与对齐 | Python + Rust 属性级对齐 | ✅ 目录库 100% / 设计库隐式 0 mismatch |
| IV. 非破坏性写入 | COW + 新会话,默认副本,diff 仅 page0 | ✅ S1–S8 验证字节 diff 仅 page0 |
| V. 规模与健壮性 | 设计/目录/系统 + ≥100MB 大库 | ✅ ams1112 42.2 万元素;边界已处理/定论 |

**结论**:无违反项;Complexity Tracking 留空。

## Project Structure

### Documentation (this feature)

```text
specs/001-e3d-data-format/
├── spec.md              # 需求规范(WHAT/WHY)
├── plan.md              # 本文件
├── research.md          # 逆向证据 + 关键决策(Phase 0)
├── data-model.md        # 字节级格式结构(Phase 1)
├── quickstart.md        # 工具运行入口(Phase 1)
├── contracts/
│   └── decode-contract.md   # 解码/编码不变式契约
├── checklists/
│   └── requirements.md  # spec 质量自检
└── tasks.md             # 任务分解(/speckit-tasks 生成)
```

### Source Code (repository root)

```text
crates/e3d_io/              # 单一真源:std-only 读写库 + e3d-io CLI
├── src/lib.rs              # 解码/编码核心 + EdbWriter API + E3dError + 测试
└── src/main.rs             # e3d-io CLI(show/refs/rename/set-pos/delete/insert)

tools/e3d_decode_rs/        # 薄读取/导出 CLI(消费 crates/e3d_io,已去重)
└── src/main.rs

src/
├── e3d_decode.rs           # (历史)→ 现经 `pub use e3d_io as e3d_decode` 复用
└── defines.rs              # PdmsHeader 字段语义注释(已校正)

docs/e3d 数据库分析/         # 规范 + Python 参考工具链 + 逆向文档
├── E3D_DB_文件格式规范.md   # 权威格式规范(§2/§7/§8/§12)
├── 离线属性解析_总结.md
├── e3d_attr_decoder.py / e3d_export.py / e3d_tree.py / e3d_write_full.py / ...

.planning/                  # 隔离计划(findings/progress/task_plan)
```

**Structure Decision**: 解码器单一真源 = `crates/e3d_io`(std-only,可独立 `cargo test`);`tools/e3d_decode_rs` 与 `pdms_io` 均为其消费者;Python 工具链为参考实现 + 交叉验证 oracle;规范与逆向依据落在 `docs/e3d 数据库分析/` 与 `.planning/`。本 feature 不新增运行代码,而是把上述既有结构固化为 spec-kit 规范并规划剩余工作。

## Complexity Tracking

> 无 Constitution 违反项,留空。
