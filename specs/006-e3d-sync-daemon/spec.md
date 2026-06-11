# Feature Specification: E3D⇄DB 常驻同步链路（e3d-syncd 守护）

**Feature Branch**: `006-e3d-sync-daemon`

**Created**: 2026-06-11

**Status**: Draft（grill Q0=A + Q1~Q6 全按推荐拍板;决策记录见 research.md）

**Input**: User description: "使用 grill-me 分析,然后编写 spec kit——006 = 把 watch.rs 雏形升级为可运行同步守护"

> 说明:001~005 交付了「读格式真相 / 单核心引擎 / 幂等落库 / 安全写回 / 链式解析」,但全部以库函数/CLI 形态存在——生产使用形态(**文件一变,库自动跟上**)尚缺。006 把 `watch.rs` 雏形升级为常驻守护 `e3d-syncd`:监控工程目录 → 自动增量提取 → 自动 ingest(水位幂等,重启安全)。读链路 only;写回自动化留 007。

## User Scenarios & Testing *(mandatory)*

### User Story 1 - 文件变化自动入库 (Priority: P1)

设计员在 E3D 中保存工作(库文件新增会话)后,守护在静定窗内捕获变化,自动提取增量并落库——无人工干预,`pe`/`pe_ses_h`/`ses` 与文件保持跟随。

**Acceptance Scenarios**:
1. **Given** 守护监控的目录中某库文件被替换为"多一个会话"的版本, **When** 静定窗过后, **Then** 新会话增量已 ingest(报告计数可见),水位前进到新 sesno。
2. **Given** 文件无变化, **When** 轮询兜底周期到, **Then** 零写库(水位拦截,跳过计数可见)。

### User Story 2 - 重启安全与幂等 (Priority: P1)

守护崩溃/重启后不重复灌库、不丢增量:状态完全由库内 `ingest_watermark` 驱动,无本地状态文件。

**Acceptance Scenarios**:
1. **Given** 守护已同步至 sesno N 后被杀, **When** 重启, **Then** 首轮对所有库零重复写(水位拦截);此后新增会话正常捕获。

### User Story 3 - 多库目录与故障隔离 (Priority: P2)

目录含多个 `*_0001` 库(可配 dbno 白名单);单库读取/解析失败只记日志并跳过该库,不拖垮守护与其它库。

**Acceptance Scenarios**:
1. **Given** 目录含 2 个库,其一损坏, **When** 同步轮, **Then** 健康库正常入库,损坏库记错误并在下轮重试,进程不退出。

### Edge Cases

- **保存进行中**(E3D 多页连续写):静定窗(默认 500ms 无新事件)+ 读取失败(页校验/会话链断)⇒ 本轮跳过、下轮重试,不半灌。
- **notify 事件丢失**:轮询兜底(默认 30s)全量比对水位,保证最终一致。
- **初次启动基线**:默认不回灌历史——以当前最新会话做一次增量 ingest 并立水位;全库初灌走显式 `--bootstrap N`(委托既有 `collect_and_save_latest_data`,004 已接唯一入口)。
- **DB 连接断开**:ingest 失败记错误,守护存活,下轮重试(幂等保证补齐)。

## Requirements *(mandatory)*

### Functional Requirements

**同步核心(P1)**
- **FR-001**: 同步核心 MUST 为库函数面(可测纯逻辑):`扫描目录 → 过滤库文件/dbno 白名单 → 逐库水位比对 → 增量提取 → ingest`,与守护壳解耦。
- **FR-002**: 增量判定 MUST 纯水位驱动(`ingest_watermark`);文件 latest sesno ≤ 水位 ⇒ 跳过(可观测);无本地状态文件。
- **FR-003**: 单库失败 MUST 隔离(记日志/计数,继续其它库;下轮重试);任何失败不退进程。
- **FR-004**: 增量提取与落库 MUST 复用既有路径(`collect_increment_eles` + `update_elements_to_database` 唯一入口);006 不新增落库语句构造点。

**守护壳(P1)**
- **FR-005**: `e3d-syncd` bin:notify 目录监控 + 静定窗(可配,默认 500ms)+ 轮询兜底(可配,默认 30s);tokio 常驻;Ctrl-C 优雅退出。
- **FR-006**: 可观测:每轮日志(扫描/触发/各库 IngestReport/跳过/错误);累计计数。
- **FR-007**: `--bootstrap N` 显式初灌(委托 `collect_and_save_latest_data(Some(N))`);默认仅以最新会话立水位基线。

**范围外**
- **FR-008**: MUST NOT 触及:写回自动 apply(007)、`sync/` 远端分发(.cba)、Meilisearch、ws 部署运维、多工程并发调度、`crates/e3d_io` 任何改动、`PdmsIO` C1 变更。

### Key Entities

- **同步轮(SyncRound)**:一次"扫描→逐库同步"的执行;产出 per-db 结果(synced/skipped/failed + IngestReport)。
- **库目标(DbTarget)**:`{ path, dbnum }`——目录扫描 + dbno 白名单的产物。

## Success Criteria *(mandatory)*

- **SC-001**: 端到端(kv-mem + 临时目录):放入 sam7200 → 守护核心首轮建立基线;用 e3d_io 写一笔新会话生成新版本文件替换 → 下轮捕获、增量入库、水位前进。
- **SC-002**: 幂等/重启:同一文件状态反复跑轮,写库零增长(跳过计数可见);模拟重启(新核心实例)首轮零重复。
- **SC-003**: 故障隔离:坏文件混入目录,健康库照常同步,进程不退,错误可观测。
- **SC-004**: 触发语义:静定窗合并事件风暴(N 次连续写只触发一轮);轮询兜底在无事件下仍达最终一致。
- **SC-005**: 红线:落库构造点 grep 零新增;`crates/e3d_io` diff 为空;C1 冻结锁未触发;双特性套件全绿。

## Assumptions

- 守护与 E3D 不抢写:守护只读库文件;读到写入中的不完整状态由静定窗+下轮重试消化。
- 单工程单守护实例;库文件名约定 `*_0001`(沿 `watch.rs`/`dblist` 既有识别)。
- SurrealDB 连接由启动参数显式给定(沿 e3d-writeback CLI 惯例,不隐式连生产)。
