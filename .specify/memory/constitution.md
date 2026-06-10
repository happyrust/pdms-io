<!--
Sync Impact Report
- Version change: (template) → 1.0.0
- Ratified: 2026-06-08 (first concrete adoption for pdms-io / E3D offline I/O)
- Modified principles: template placeholders → 5 concrete principles
  - [PRINCIPLE_1] → I. 纯离线 / 纯文件优先 (Offline-First, Pure-File)
  - [PRINCIPLE_2] → II. 取证式逆向 (Evidence-Based Reverse Engineering)
  - [PRINCIPLE_3] → III. 双实现与对齐 (Dual Implementation & Parity)
  - [PRINCIPLE_4] → IV. 非破坏性写入 (Non-Destructive Writes / COW)
  - [PRINCIPLE_5] → V. 规模与健壮性验证 (Scale & Robustness)
- Added sections: Additional Constraints; Development Workflow; Governance
- Removed sections: none
- Templates requiring updates:
  - .specify/templates/plan-template.md ✅ Constitution Check 引用本文件(无需结构改动)
  - .specify/templates/spec-template.md ✅ 兼容(spec 保持 WHAT/WHY)
  - .specify/templates/tasks-template.md ✅ 兼容(任务按 user story 组织)
- Deferred TODOs: none
-->

# pdms-io / E3D 离线 I/O Constitution

> 适用范围:AVEVA Everything3D / PDMS 的 DABACON 数据库**离线读写**能力(`crates/e3d_io`、`tools/e3d_decode_rs`、`docs/e3d 数据库分析/` 工具链、`src/e3d_decode.rs`)。
> 基线:AVEVA Everything3D 2.10 `core.dll`(base `0x10000000`)。

## Core Principles

### I. 纯离线 / 纯文件优先 (Offline-First, Pure-File)

所有**交付的**读写能力 MUST 仅依赖磁盘文件(元素库 + 模式库 `*vir.dat`),不得在运行期依赖正在运行的 E3D 进程、IDA、或任何 AVEVA 安装组件。运行中的 `core.dll` 与 IDA 仅可用于**取证与验证**,绝不可成为交付物的运行期依赖。

理由:目标是把 E3D 数据接入纯文件的离线管线(与 pdms-io 既有方向一致);任何运行期耦合都会破坏可移植性与可测试性。

### II. 取证式逆向 (Evidence-Based Reverse Engineering)

每个格式结论 MUST 同时具备两类证据:(a) `core.dll` 反编译函数佐证(给出**函数名 + 地址**),且 (b) 真实样本的**字节级交叉验证**。禁止臆测。不确定项 MUST 显式标注 `[存疑]` 并记录回退方案;被推翻的旧假设 MUST 在 findings 中留痕(不静默删除)。

理由:格式逆向的唯一可信来源是"代码怎么读 + 真实字节是什么";二者缺一即为猜测。

### III. 双实现与对齐 (Dual Implementation & Parity)

核心读/写路径 MUST 同时有 **Python 参考实现** 与 **Rust std-only 实现**,并按真实库做**属性级 diff 对齐**。对齐基线:目录库 100%、设计库隐式属性 0 mismatch。任一端发现 bug MUST 在两端同步修复并补回归测试。

理由:双实现互为 oracle,是发现解码 bug(如 CJK latin1、大库 walk 截断)的最有效手段。

### IV. 非破坏性写入 (Non-Destructive Writes / COW)

写入 MUST 遵循 `db5_save_work` 语义:**COW + 追加新会话**——绝不改动旧页,所有改动追加到文件尾,仅最后原子重指 page0 会话指针(`0x28`)。默认**仅作用于副本**;`--inplace` 需显式开关 + 落盘前自校验。提交后 MUST 满足"新会话=新值 / 旧会话=原值 / 字节 diff 仅 page0 会话指针"。

理由:多版本可回溯 + 原文件逐字节不可变,是对真实工程数据安全编辑的前提。

### V. 规模与健壮性验证 (Scale & Robustness)

解码器 MUST 在设计库 + 目录库 + 系统库 + ≥100MB 大库上验证通过;已知边界(短 skeleton-K、脏/误报索引条目、CJK 名、`sel=0`/packed、多节点 DA/成员链、跨页重定位)MUST 显式处理或给出**定论**(含真实样本依据)。

理由:真实工程库规模达数十万元素并含大量边角形态;只在小样本验证不足以声称"打通"。

## Additional Constraints

- **std-only 核心**:`crates/e3d_io` 与 `tools/e3d_decode_rs` MUST 保持 std-only(无第三方依赖),以便独立 `cargo test` 并绕开重依赖树。
- **不擅改项目外兄弟 crate**:`../rs-core`(aios_core)等项目外 crate MUST NOT 在无显式授权下修改;整 crate 集成阻塞按"旁路 sub-crate"处理。
- **CLI 文本 I/O**:命令行工具 SHOULD 提供人类可读 + JSON 两种输出;写命令默认副本输出。
- **格式权威源**:属性 offset 的权威来源是模式库 `*vir.dat` 的 type-def(K skeleton),NOT attlib、NOT 运行时累加。

## Development Workflow

- **planning-with-files**:重要工作在 `.planning/<date-slug>/` 隔离计划下推进(task_plan / findings / progress 三件套);spec-kit 工件位于 `specs/`。
- **verification-before-completion**:声称"完成/通过/修复"前 MUST 实际运行测试或 demo 并贴出证据(`cargo test` 结果、Python demo PASS、字节 diff)。
- **文档落地**:每条新结论 MUST 同步进 `docs/e3d 数据库分析/E3D_DB_文件格式规范.md`(规范)与对应计划 `findings.md`(依据);临时探针用后删除。
- **取证工具隔离**:IDA/运行时探针属验证手段,产物(地址、反编译结论)记入文档,不进入交付运行路径。

## Governance

本 Constitution 高于其它临时约定;冲突时以本文件为准。

- **修订**:任何原则增改 MUST 提升 `CONSTITUTION_VERSION`(语义化:MAJOR=不兼容的原则移除/重定义;MINOR=新增原则/章节;PATCH=措辞澄清),并在文件顶部 Sync Impact Report 留痕。
- **合规审查**:`/speckit-plan` 的 Constitution Check 闸门 MUST 对照本文件;违反项要么修正,要么在 plan 的 Complexity Tracking 中显式论证。
- **写侧最终判据(gated)**:真实 running-E3D round-trip 取证为写侧能力的最终验收判据;在取得前,写侧结论 MUST 标注"机制完备 / 未真机取证"。

**Version**: 1.0.0 | **Ratified**: 2026-06-08 | **Last Amended**: 2026-06-08
