# Changelog - pdms-io-fork

## 2026-04-09

### Fixed

- **parse.rs — 6 处边界防御修复，防止畸形/截断数据导致 panic**
  - `parse_raw_explicit_attrs`: 循环条件从 `!is_empty()` 改为 `len() >= 4`，防止不足 4 字节时切片越界
  - `parse_raw_explicit_attrs` STRING 分支: 增加 `4 + len_a <= tmp_input.len()` 检查，防止恶意 `len_a` 导致切片溢出
  - `get_implicit_len_by_offset`: 增加 `index + 1 < count.len()` 保护，防止最后一个元素时数组越界
  - `get_refno_entry`: 增加 `offset < 4` 与 `tmp_pos + 20 > input.len()` 前置检查，防止偏移量越界；`else` 分支增加 `tmp_pos + 12 <= input.len()` 守卫
  - `collect_explict_data`: 引入 `MAX_RESYNC = 64` 上限，连续 resync 超限时中断循环，防止畸形数据导致无限循环
  - `parse_db_basic_info` / `parse_file_basic_info`: `File::open` 和 `read_exact` 的 unwrap 改为优雅降级；输入长度不足时返回默认值而非 panic

- **element_record_reader.rs — 超限处理改为显式报错**
  - `find_record_end`: 元素记录超过 1MB 限制时从静默截断改为返回 `Err`，便于上层定位问题

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
