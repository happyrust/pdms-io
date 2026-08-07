# 计划：E3D 3.1 core.dll FFI Oracle（草案）

> **状态**：DRAFT — 尚未经过 Plannotator gate。

## 方案概览

把 `core.dll` 包成一个**子进程 oracle**：32-bit Rust 二进制承载 FFI，对外用简单的 stdio JSON 协议接收命令、返回结果。主测试进程（64-bit）通过该子进程对 fixture 执行只读 / 写入操作并取回快照，作为 Rust 自实现的对照基线。

## 工作切片

| Slice | Purpose | Done when | Risks |
| --- | --- | --- | --- |
| 1 | FFI 表面定义 | 11 个 `db_*` 函数的 `extern "C"` 签名 + Rust safe wrappers，编译通过（在 32-bit target 下） | 函数签名细节（指针 vs 整型 token）需要再验证 |
| 2 | 32-bit 进程 harness | 子进程能装载 `core.dll`、初始化、退出 | DLL 依赖的 runtime / VC++ 版本可能缺失 |
| 3 | Stdio JSON 协议 | 命令编解码 + 错误传递 + 关闭流程 | JSON 中的二进制（页字节）需要 base64 / hex |
| 4 | 只读用例 | 打开 fixture、读 page、读 header、find_reference、go_to_element | 部分 API 有隐式全局状态 |
| 5 | 写入用例 | 打开 write 模式、claim、单属性更新、release、save_work | 写错 fixture 会破坏样本，必须先复制 |
| 6 | 与 e3d-reader 对照 | 至少 5 类操作在同一 fixture 上 oracle vs Rust 完全一致 | 字节级一致性可能暴露 record 解析差异 |

## 执行顺序

- 1 → 2 → 3 串行，无法并行（基础设施）。
- 4 与 5 在 3 后可并行；5 必须先做 fixture 复制策略。
- 6 是收口。

## 验收标准

- [ ] 11 个 FFI wrapper 通过类型与 ABI 校验。
- [ ] 子进程能稳定启停（包括崩溃恢复）。
- [ ] 协议有完整错误码表与超时机制。
- [ ] 至少 5 类操作 oracle vs Rust 一致。
- [ ] 写入用例不污染原 fixture（自动复制 + 临时目录）。

## 方向控制

- 不允许在 Rust 主进程中直接 `LoadLibrary("core.dll")`：必须经子进程。
- 不允许 oracle 调用结果反向回填 `e3d-reader` 的实现（避免循环验证）。
- 任何 oracle 异常都要打日志并保留 fixture 副本。
