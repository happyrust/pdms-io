# Contract: 同步核心 / 守护行为 / 红线

**Spec**: [../spec.md](../spec.md) | **Date**: 2026-06-11

> 006 验收依据:G1 同步核心语义;G2 触发与静定;G3 可观测与故障隔离;G4 红线。

## G1. 同步核心语义

- **G1-A1**: `scan_targets(dirs, dbno_whitelist) -> Vec<DbTarget{path, dbnum}>`:识别 `*_0001` 库文件(读头取 dbnum;读失败的文件记跳过,不报错)。
- **G1-A2**: `sync_db(target) -> DbSyncOutcome`:open → `get_latest_sesno` → 与 `ingest_watermark` 比对——
  - latest ≤ 水位 ⇒ `Skipped`(零写库);
  - latest > 水位 ⇒ `collect_increment_eles(Some(水位+1..=latest))` → `update_elements_to_database` → `Synced(IngestReport)`;
  - 无水位(初见库)⇒ 仅以**最新会话**做一次增量 ingest 并立水位(基线;不回灌历史);
  - 任何错误 ⇒ `Failed(err)`(隔离,进程不退)。
- **G1-A3**: 同步核心 MUST 不持本地状态(纯水位驱动);同一文件状态重复执行 = 幂等(003 D2 保证)。
- **G1-A4**: 初灌走显式 `--bootstrap N` ⇒ `collect_and_save_latest_data(Some(N))`(既有唯一入口;006 不新增落库构造点)。

## G2. 触发与静定(守护壳)

- **G2-A1**: notify 监控全部 watch_dirs(递归);事件仅作"待同步标记",真正动作在**静定窗**(默认 500ms 无后续事件)后执行。
- **G2-A2**: 轮询兜底(默认 30s)无条件跑一轮全量 `sync_db`(水位拦截保证零成本跳过)。
- **G2-A3**: 事件风暴(连续 N 次写)MUST 合并为一轮;同一库并发同步 MUST 串行(单飞)。
- **G2-A4**: Ctrl-C/SIGTERM 优雅退出:完成当前库后停,不半途中断 ingest(块级失败有幂等兜底,但不主动制造)。

## G3. 可观测与故障隔离

- **G3-A1**: 每轮日志:扫描库数/触发原因(event|poll|bootstrap)/逐库 outcome(Synced{report}|Skipped|Failed{err});进程级累计计数。
- **G3-A2**: 单库 Failed 不影响同轮其它库;连续失败仅记数(无退避策略,留 007+)。
- **G3-A3**: DB 连接失败 = 该轮全库 Failed,守护存活下轮重试。

## G4. 红线

- 落库语句构造点 grep **零新增**(全部经 003/004 唯一入口)。
- `crates/e3d_io` diff MUST 为空;`PdmsIO` C1 冻结面零变更(api_freeze 为闸)。
- 写回队列不自动 apply(007);`sync/` 远端模块不触。
