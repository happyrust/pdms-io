# 验证：E3D 3.1 core.dll FFI Oracle

## 命令

| Command | Purpose | Expected pass condition | Evidence location |
| --- | --- | --- | --- |
| `cargo build --target i686-pc-windows-msvc -p e3d-oracle-child` | 子进程 harness 在 32-bit target 下编译通过 | exit code 0；产物存在 `target/i686-pc-windows-msvc/debug/` | `progress.jsonl` |
| `cargo test -p e3d-oracle --test smoke -- --nocapture` | 子进程启停 + 基础协议 smoke | 所有 smoke 用例 pass；子进程退出码 0 | `progress.jsonl` |
| `cargo test -p e3d-oracle --test readonly_parity -- --nocapture` | 5 类只读操作 oracle vs `e3d-reader` 一致 | 所有断言 pass，对照差异为空 | `progress.jsonl` |
| `cargo test -p e3d-oracle --test writeback_roundtrip -- --nocapture` | 写入 round-trip：Rust 写 → oracle 读一致 | 所有断言 pass；fixture 副本未污染原始 | `progress.jsonl` |

## 人工检查

- 子进程协议（命令 / 响应 / 错误码 / 超时）有完整文档；二进制载荷的编码（base64 / hex）明确。
- DLL 加载失败、core.dll 异常、子进程崩溃这三类异常路径都有 cleanup 与日志。
- oracle 不被 import 进生产 `e3d-reader` crate；仅测试代码引用。
- fixture 复制策略保证原 fixture 不被任何 oracle 写入操作触及。
- 对 `brief.md`、`plan.md`、`verification.md`、`blockers.md`、`goal-prompt.md` 全部跑 Plannotator gate，每个批准前不得继续执行。

## 证据规则

- 每次 oracle vs Rust 对照测试都记录差异 diff（即使为空），追加到 `progress.jsonl`。
- core.dll MD5 与子进程 build 信息记录到 progress，确保结果可重现。
- 任何"暂时跳过"的对照用例都必须显式列出原因，不允许静默忽略。
