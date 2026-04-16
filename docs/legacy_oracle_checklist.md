# Legacy Oracle Checklist

以下能力保留为 legacy baseline，仅用于对照，不再作为新引擎实现骨架：

- `pdms_io::PdmsIO::search_latest_refno`
- `pdms_io::PdmsIO::read_element_record_cached`
- `pdms_io::PdmsIO::parse_raw_element`
- 现有 `writer` 的最小样本写入测试

使用约束：

- 新引擎核心模块不得直接调用上述实现
- 仅允许在 `crates/pdmsdb_engine_v2/tests/` 与 `compare/` 中调用
- 任何差异先记录为 compare 结果，不得反向污染 v2 接口设计

