# E3D 3.1 core.dll FFI Oracle（草案）

> **状态**：DRAFT — 尚未经过 Plannotator gate。

## 目标结果

构建一个 **运行时 oracle**：通过 Rust ↔ `core.dll` FFI 调用真实的 `db_*` C API，把 `core.dll` 的读 / 写结果作为黄金参考，用于校验 Rust 自行实现的 `e3d-reader` / 未来 `e3d-writer` 的行为。Oracle 仅作为测试基础设施，不进入生产路径。

## 背景

- 前期 IDA 分析（goal `e3d31-core-io-rewrite`）已识别 `db_*` C API（~50 个 wrapper）与 dispatcher / 元素操作核心。
- 目前 Rust 实现的正确性靠"读到与 IDA 预期一致的常量"间接确认；想做写回前必须建立独立 oracle。
- `core.dll` 是 32-bit Windows DLL，需要专门的 32-bit Rust harness（或独立子进程 + IPC）。

## 约束

- 不修改 `core.dll`；不重定向其内部状态。
- 调用必须可重入：oracle 进程可被反复创建并销毁。
- FFI 边界仅暴露最小集：`db_open_read_db` / `db_open_write_db` / `db_close_db` / `db_save_work` / `db_read_page` / `db_write_page` / `db_get_header_integer` / `db_claim_element` / `db_release_element` / `db_find_reference` / `db_go_to_element`。
- 不允许把 oracle 的实现细节泄漏到 `e3d-reader`：Rust 实现仍由 IDA 证据驱动。

## 非目标

- 完整 mock / record-replay 框架（先有实时调用即可）。
- Linux / macOS / 64-bit core.dll 兼容（基线是 Windows 32-bit）。
- 性能 / 高并发（oracle 是测试工具，单进程串行足够）。
- 替换 Rust 实现的生产路径。

## 需要先询问

- 32-bit 子进程方案 vs 在主进程内做 32-bit Rust：哪个工作量更小、风险更低。
- 通信协议（stdio JSON / named pipe / shared memory）。
- 是否要把 oracle 的 baseline session 也版本控制起来。

## 完成定义

- Rust 测试代码可通过 oracle 在 fixture 上执行至少 5 类只读操作，结果与 `e3d-reader` 一致。
- 写回 goal `e3d31-writeback` 的 round-trip 验证可调用本 oracle。
- oracle 启动 / 关闭 / 异常恢复行为有文档与回归测试。
