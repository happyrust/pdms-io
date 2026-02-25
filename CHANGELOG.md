# Changelog - pdms-io-fork

## 2026-02-25

### Changed

- **升级至 Rust edition 2024，全面重构 IO 与解析层**
  - `src/io.rs`：重构读写流程，增强错误处理与日志
  - `src/writer.rs`：优化写入逻辑
  - `src/page_manager.rs`：改进页面管理
  - `src/element_serializer.rs`：简化元素序列化
  - `src/element_record_reader.rs`：优化记录读取
  - `src/search.rs`：改进搜索功能
  - `src/config.rs`：配置加载增强
  - `src/defines.rs`：更新常量与类型定义

- **parse_pdms_db 解析器全面升级**
  - 表达式解析增强：`expression.rs`、`expression_payload.rs`、`opcode.rs`
  - Attlib 解析改进：`attlib/mod.rs`、`noun_schema.rs`
  - 属性解析优化：`explicit.rs`、`implicit.rs`、`axis.rs`
  - 基础组合子与数值解析改进：`combinator.rs`、`numeric.rs`、`primitives.rs`

- **测试用例大规模更新**
  - `test_collect_latest_eles.rs`：大幅扩展（+600 行）
  - 更新 30+ 个测试文件以适配新 API
  - 新增 `test_write_integration.rs` 写入集成测试

### Fixed

- **修复 sync 模块编译与逻辑问题**
  - `sync/clone.rs`、`sync/compress.rs`：适配新 IO 接口
