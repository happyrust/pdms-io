# 阻塞项：E3D 3.1 core.dll FFI Oracle

## 开放问题

- 32-bit Rust 子进程方案 vs 在主进程内通过 thunk 调用：先选哪个？
- 通信协议（stdio JSON / named pipe / shared memory）的选择标准（吞吐、延迟、调试便利度）。
- 是否要把 baseline oracle session 的快照纳入版本控制（用于回归）。
- core.dll 是否依赖额外的 AVEVA 运行时（环境变量、license server）？

## 停下并询问

- 在尚未取得稳定 fixture 副本前，**不允许**对原始 fixture `ams1112_0001` 执行任何写操作。
- 如果发现 core.dll 调用会修改全局磁盘状态（license / 缓存目录 / 日志），停下并确认隔离策略。
- 子进程在子目录外做任何 I/O 之前，必须先通过审阅。
- 把 oracle 接入 `e3d31-writeback` 之前，oracle 的关闭 / 异常恢复策略必须先 review。

## 危险或高风险操作

- `LoadLibrary("core.dll")` 在主进程直接调用（被显式禁止）。
- 修改 IDA 数据库 / 重命名函数。
- 删除或写入原始 fixture。
- 暴露 oracle 的实现细节回 `e3d-reader` 形成循环验证。

## 已知阻塞

- 子进程的 build 与运行环境需要 32-bit MSVC toolchain，CI 配置可能不齐。
- `db_*` 函数的精确 ABI（cdecl / stdcall）需要在 IDA 中确认，不能凭命名猜测。
- 部分函数依赖全局状态（已打开 DB 表、错误码缓冲区），多次调用之间的状态清理需要专门测试。
