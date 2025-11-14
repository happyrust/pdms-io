# Repository Guidelines

## Project Structure & Module Organization
- `src/lib.rs` 聚合 IO、搜索和同步模块，`src/io.rs`/`src/watch.rs` 负责解析 PDMS 文件与文件系统监听；二进制入口集中在 `src/bin/`（例如 `demo_deletion_with_edges.rs`）。
- `src/test/`、`src/tests/` 保留高密度用例，`examples/benchmark_search.rs` 提供性能基准；文档素材位于 `docs/`，大体量样例数据放在 `pdms-test-data/` 与 `resource/`，请勿提交真实敏感数据。
- `DbOption.toml` 与 `start_meilisearch.sh`、`surrealdb.ts` 记录外部服务配置，修改后需在 PR 描述中说明依赖。

## Build, Test, and Development Commands
- `cargo check`：快速语义检查；`cargo build --release` 用于生成可发布的二进制。
- `cargo run --bin demo_deletion_with_edges -- --source data.ms`：演示删除流程；如需开启调试特性，可附加 `--features debug_parse,debug_btree_search`。
- `cargo test` 覆盖单元 + 集成测试；对单个场景可执行 `cargo test test_parse -- --nocapture`；长时间基准请运行 `cargo bench` 或在 `examples/` 内手动运行。
- 开发前运行 `cargo fmt` 与 `cargo clippy --all-targets -- -D warnings`，CI 会拒绝未格式化代码；`./start_meilisearch.sh` 与 `surrealdb.ts` 用于本地依赖的快速拉起。

## Coding Style & Naming Conventions
- Rust 代码使用 4 空格缩进，模块/函数保持 `snake_case`，类型用 `UpperCamelCase`，常量使用 `SCREAMING_SNAKE_CASE`；日志内容使用统一的 `log` crate 宏。
- 避免在系统参数之外直接持有 `MessageWriter<T>`；新增消息类型请在系统函数中声明独立参数并遵循 Bevy 约束。
- 配置结构体字段保持 serde tag 与数据库列一致，例如 `Refno`、`Operation` 等应与 `parse_pdms_db` 输出保持同名。

## Testing Guidelines
- 集成测试目录 `src/test/` 以 `test_*` 命名，数据夹 `test_data.rs`、`test_data_with_members.rs` 提供基线；提交新的解析逻辑时需新增回归样例。
- 推荐每条 PR 附带 `cargo test --package pdms_io refno_test` 结果摘要，涉及异步或 IO 的模块需补充 `tokio::test` 或基准输出。
- 当测试依赖外部搜索/数据库时，说明使用的端口与模拟数据位置，避免 CI 污染。

## Commit & Pull Request Guidelines
- 历史提交遵循 `type: summary` 约定（如 `feat: integrate raphtory timeline`、`fix: resolve surrealdb deps`），继续使用英文动词 + 简体补充说明的混合格式。
- PR 描述需包含：变更目的、关键命令（如新的同步脚本）、影响的目录列表、截图或日志链接（如涉及 `graphs/` 可视化）。
- 关联 Issue 请使用 `Closes #id`，并在审查前确认 `cargo fmt && cargo clippy && cargo test` 已通过。

## 数据与配置提示
- 运行本地同步前，复制 `DbOption.toml` 并填写私有凭据，避免把真实密钥提交；公共模板可随 PR 更新。
- `resource/` 与 `pdms-test-data/` 空间占用较大，请用 Git LFS 或外部下载链接共享；若需新增数据，附带 README 说明字段含义与生成脚本。
- 所有外部服务（SurrealDB、Meilisearch）默认使用 Rust TLS；如在内网环境需降级，请在 PR 中列出风险与回滚方案。
