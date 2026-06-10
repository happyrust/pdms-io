# Implementation Plan: E3D I/O 三引擎收敛（单一格式核心）

**Branch**: `002-e3d-io-engine-consolidation` | **Date**: 2026-06-10 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/002-e3d-io-engine-consolidation/spec.md`

## Summary

把仓库内三套并存的 E3D 读写实现（`PdmsIO` v1 / `engine_v2` / `e3d_io`）收敛为**以 `crates/e3d_io` 为唯一格式核心**：`e3d_io` 引入最小 `PageSource` 页源抽象（`InMemory` + `PagedFile` LRU 双实现），`PdmsIO` 保 API 换芯为门面，`engine_v2` 与 v1 写路径等零调用方孤岛退役删除、知识归档。切换策略为大爆炸（Q4=C），合入闸门 = workspace 构建 + 全部现有测试全绿。

## Technical Context

**Language/Version**: Rust（edition 2024；`crates/e3d_io` 维持 std-only 红线）

**Primary Dependencies**: 无新增第三方依赖；`PagedFile` 的 LRU 用 std 实现（承接 `page_manager.rs` 既有逻辑）

**Storage**: 同 001——元素库文件（512/2048/4096B 页、大端）+ 模式库 `*vir.dat`

**Testing**: workspace `cargo test`（含 `src/test/`、`src/tests/`、`tests/`）+ `crates/e3d_io` 独立 `cargo test`；`ams1112_0001`/`sam7200_0001` 为必测样本

**Target Platform**: 跨平台库 + CLI（同 001）

**Project Type**: 仓库内部架构重构（library 收敛），无新用户面流程

**Performance Goals**: 增量 watcher 路径读页计数 < 全文件页数（LRU 生效）；全库枚举性能不劣于收敛前基线

**Constraints**: `PdmsIO` 公共 API 签名零变更；`e3d_io` std-only；大爆炸切换（无双跑期）；不触及落库/sync/写回透出（003+）

**Scale/Scope**: 删除 ~30 文件（engine_v2 25 + v1 写路径与读取辅助 5）；改造 `e3d_io` 取页边界 + `PdmsIO` 内部全部委托

## Constitution Check

对照 `.specify/memory/constitution.md` v1.0.0：

| 原则 | 闸门 | 状态 |
|---|---|---|
| I. 纯离线/纯文件优先 | 收敛不引入任何运行期外部依赖 | ✅ 纯内部重构；页源抽象仍是纯文件 |
| II. 取证式逆向 | 不新增格式结论；既有结论引用 001 | ✅ 002 不建立新格式真相，引用 001 data-model/contract |
| III. 双实现与对齐 | Python↔Rust 对齐保障不受损 | ✅ `e3d_io` 测试与对齐套件原样保留并继续作为闸门 |
| IV. 非破坏性写入 | 写能力唯一来源为 e3d_io（COW+新会话） | ✅ v1 写路径退役后只剩 001 规范下的写实现 |
| V. 规模与健壮性 | ≥100MB 大库能力不退化 | ✅ `PagedFile` 页源承接 LRU 流式；SC-006 量化 |

**结论**：无违反项。唯一非常规点 = Q4 大爆炸切换（无双跑对照），已作为用户决策与风险声明记录于 spec Assumptions 与 research.md，不构成宪法违反。

## Project Structure

### Documentation (this feature)

```text
specs/002-e3d-io-engine-consolidation/
├── spec.md          # 需求规范（WHAT/WHY + 范围边界）
├── plan.md          # 本文件
├── research.md      # 三引擎现状证据 + grill-me 决策记录（Q1~Q6）
├── contracts/
│   └── api-compat-contract.md   # PdmsIO API 兼容契约 + PageSource trait 契约
└── tasks.md         # 任务分解
```

### Source Code (目标态)

```text
crates/e3d_io/                  # 唯一格式核心
├── src/lib.rs                  # 解码/编码核心（既有）
├── src/page_source.rs          # 新增：PageSource trait + InMemory + PagedFile(LRU)
└── src/main.rs                 # e3d-io CLI（不变）

src/
├── io.rs                       # PdmsIO 门面：API 不变，内部全部委托 e3d_io
├── defines.rs                  # 保留：类型与常量（去除与 e3d_io 重复的解析逻辑）
├── watch.rs / config.rs / ...  # 不变（消费 PdmsIO 门面）
└── lib.rs                      # 移除 engine_v2/writer/element_serializer 声明

(删除) src/engine_v2/**         # 25 文件退役
(删除) src/writer.rs            # v1 写路径退役
(删除) src/element_serializer.rs
(删除) src/page_manager.rs      # LRU 语义并入 e3d_io::PagedFile 后删除
(删除) src/paged_reader.rs
(删除) src/element_record_reader.rs
(删除) src/bin/verify_engine_v2.rs

docs/
└── engine-v2-archaeology.md    # 新增：db1~db5 ↔ core.dll 对照知识归档
```

## Phases

### Phase 0 — Research 固化（已完成）
现状盘点与决策记录写入 `research.md`：三引擎依赖关系（孤岛证据）、重复矩阵、grill-me Q1~Q6 决策（含 Q4=C 风险声明）。**产出**: research.md。

### Phase 1 — `PageSource` 页源抽象（e3d_io 内）
1. 定义 `PageSource` trait（`(ext_no, pgno) → 页字节` + `page_size()`；契约见 contracts）。
2. `InMemory` 实现 = 现有整文件 buffer 路径改走 trait（行为零变化，e3d_io 既有测试全绿即过）。
3. `PagedFile` 实现 = 移植 `page_manager.rs` 的 LRU（容量/驱逐/统计），std-only。
4. 双页源一致性测试（SC-003）+ 缓存统计可观测测试（SC-006）。
**闸门**: `crates/e3d_io` 独立 `cargo test` 全绿。

### Phase 2 — `PdmsIO` 换芯
1. 按 `contracts/api-compat-contract.md` 冻结公共 API 清单。
2. 头部/会话链/页大小探测/B 树查找/元素读取逐项改为委托 `e3d_io`（经 `PagedFile` 页源）。
3. 增量提取与索引映射（FR-010）语义等价迁移；索引缓存文件兼容策略落地（沿用或版本重建，写入契约）。
4. `parse_pdms_db` 职责评估：`EleData` 保留为类型适配层或并入（FR-003）。
**闸门**: workspace `cargo build` + 全部现有测试全绿（调用方零修改）。

### Phase 3 — 孤岛退役
1. `lib.rs` 移除 `engine_v2`/`writer`/`element_serializer` 声明；删除对应文件与 `verify_engine_v2` bin。
2. 删除 `page_manager.rs`/`paged_reader.rs`/`element_record_reader.rs`（能力已被 Phase 1/2 吸收）。
3. db1~db5 ↔ core.dll 对照知识归档至 `docs/engine-v2-archaeology.md`。
**闸门**: workspace 构建通过、无引用残留（SC-004）。

### Phase 4 — 清理与文档
1. 更新 `docs/ARCHITECTURE.md` 至收敛后架构（单核心 + 门面图）。
2. 实现盘点核查（SC-001）：grep 证明 B 树/记录/页缓存各仅一份。
3. CHANGELOG 记录；spec 状态置 Implemented。
**闸门**: SC-001~SC-006 全部达成。

## Risks

| 风险 | 缓解 |
|---|---|
| 大爆炸切换（Q4=C）：深层路径回归事后才能发现 | `ams1112_0001` 页大小说谎样本必测；现有测试全绿硬闸门；git 历史为回滚退路；Phase 1 双页源一致性测试先行 |
| `PdmsIO` 隐式行为（缓存时序、会话边界推导）与测试断言耦合 | Phase 2 逐项迁移时以现有测试断言为 oracle，禁止"顺手修语义" |
| `PageSource` 抽象诱发过度设计 | trait 保持最小（取页 + 页大小）；多 extent 等假设显式范围外 |
| `parse_pdms_db` 与 `e3d_io` 的 `EleData` 类型双轨 | FR-003 评估先行；若保留适配层，禁止其中含字节解析 |
| `rs-core ↔ surrealdb` 漂移阻塞 workspace 构建 | 同 001 的旁路策略；002 验收以可构建的目标集为准并在 tasks 中显式标注 |
