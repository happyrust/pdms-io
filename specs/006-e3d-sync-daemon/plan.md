# Implementation Plan: E3D⇄DB 常驻同步链路（e3d-syncd）

**Branch**: `006-e3d-sync-daemon` | **Date**: 2026-06-11 | **Spec**: [spec.md](./spec.md)

## Summary

两层薄壳把 001~005 串成生产形态:**同步核心**(扫描→水位比对→增量→ingest,可测纯逻辑,故障隔离)+ **守护壳**(notify+静定窗 500ms+轮询兜底 30s+优雅退出,bin `e3d-syncd`)。纯水位驱动 ⇒ 重启安全免费;零新落库构造点;e3d_io/C1 零触碰。

## Technical Context

**Language**: Rust(edition 2024);**Dependencies**: 零新增(notify/tokio/log 均既有)

**Testing**: 同步核心 = kv-mem + 临时目录端到端;守护壳 = 静定/轮询逻辑单测 + bin smoke;oracle = 水位/IngestReport 可观测断言

**Constraints**: 守护只读库文件;单工程单实例;连接显式给定;G4 红线

## Constitution Check

| 原则 | 状态 |
|---|---|
| I. 纯离线 | ✅ kv-mem + 本地文件验收;无外部服务依赖 |
| II. 取证式逆向 | ✅ 零新格式结论(静定窗依据 = 001 db5_save_work 多页写实证) |
| III. 测试先行 | ✅ 核心先于壳,每阶段测试随行 |
| IV. 非破坏写 | ✅ 守护只读;落库经既有幂等入口 |
| V. 规模健壮 | ✅ 故障隔离 + 轮询兜底最终一致;ams1112 量级在端到端可选覆盖 |

**结论**:无违反项。

## Phases

### Phase 0 — Research(已完成)
存量盘点(六环节全可复用,缺口仅两层)+ Q0~Q6 决策 → research.md、contracts/sync-daemon-contract.md。

### Phase 1 — 同步核心(库函数面,`src/sync_core.rs`)
1. `scan_targets`(G1-A1:`*_0001` 识别 + dbno 白名单)。
2. `sync_db`(G1-A2:水位比对 → 范围增量 → 唯一入口 ingest;初见库立基线;Failed 隔离)。
3. kv-mem + 临时目录测试:首轮基线/新会话捕获/幂等跳过/重启零重复/坏文件隔离(SC-001~003)。
**闸门**: `--features surrealdb` 核心测试全绿;落库构造点 grep 零新增。

### Phase 2 — 守护壳(`src/bin/e3d_syncd.rs`)
1. notify + 静定窗 + 轮询兜底 + 单飞串行(G2-A1~A3);参数(dirs/whitelist/窗口/周期/--bootstrap/--surreal/--ns/--dbname)。
2. 优雅退出(G2-A4)+ 每轮日志与累计计数(G3-A1)。
3. 静定窗合并逻辑单测(事件风暴→一轮);bin smoke(临时目录实跑一轮)。
**闸门**: SC-004;bin 编译+smoke。

### Phase 3 — 端到端与收口
1. e2e:临时目录放 sam7200 → 基线 → e3d_io 写新会话替换文件 → 捕获 → 水位前进 → 重启幂等(SC-001/002 全链)。
2. T303 GATE:双特性全量套件 + api_freeze + e3d_io diff 空(SC-005)。

### Phase 4 — 文书
ARCHITECTURE 数据流(监控段改述)+ CHANGELOG + SC 核销 + spec → Implemented。

## Risks

| 风险 | 缓解 |
|---|---|
| 读到保存中间态 | 静定窗 + 失败跳过下轮重试(COW+page0 原子重指,无半态落库) |
| notify 丢事件 | 轮询兜底 30s 最终一致 |
| 事件风暴重复同步 | 静定窗合并 + 单库单飞 + 水位幂等三重保险 |
| 005 未收口的交叉 | 006 实现排在 005 T103~T402 之后;spec 先行不阻塞 |
