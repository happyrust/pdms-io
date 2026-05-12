# Changelog - pdms-io-fork

## 2026-05-12

### Added — E3D 3.1 属性解析全流程恢复（Slice 1-3+4 step 1-5+7 step 1-6）

- **IDA 证据驱动的属性子系统恢复**
  - `ida_exports/3.1/attribute_names.json` — 从 `core.dll` 全局符号 `?ATT_<NAME>@@3...` 提取 **6377 条系统属性名表**（0 collision，1031 KB，db1_hash 验证全部通过）
  - `ida_exports/3.1/attribute_types.json` — DBE_Value/Position/Direction/Orientation/String 类家族 + DB_Attribute 布局 + EXMAP 派发表 + `expType` 枚举完整命名 + DB_Attribute::type 7/12 处历史误解的修正
  - `docs/ida-3.1-attributes.md` — 完整架构文档，含 §1a 系统属性名表 + §3 EXMAP 派发 + §3a DB_Attribute 布局 + §3b DBE_Base/Attribute 运行时表示 + 修订记录

- **goal package `e3d31-attribute-parsing` 完整闭环**
  - `goals/e3d31-attribute-parsing/progress.jsonl` — 26 条记录覆盖 Slice 1-3+4+7 全过程
  - `goals/e3d31-attribute-parsing/SLICE4_IMPLEMENTATION_PLAN.md` — turn-key 实施方案
  - `goals/e3d31-attribute-parsing/{brief,plan,blockers,verification,goal-prompt}.md` — plannotator gate 已批准的全套文档
  - `goals/e3d31-attribute-parsing/CURRENT_STATE.md` + `IDA_VERIFICATION.md` — Slice 1 清点 + Slice 1.5 IDA 验证

### Discoveries — 关键架构发现

- **DBE_Value `+8` 字段修正**：先前命名 `type_or_subtype` 错误；实际是 KCONDM 编码的单位代码（uomlib），不是类型标签。证据 `DB_Element::getAtt` (`0x5933ea0`)
- **EXMAP 派发表**：`sub_51D368F`（MTR=`exprlib/EXMAP`）映射 `(isUDA, type, size, ityp)` → `DBE_Base::expType`；14 个 `expType` 值全部命名（BOOLEAN/REAL/TEXT/DBREF/POSITION/DIRECTION/ORIENTATION/各 *_ARRAY/BLOB）
- **DB_Attribute::type 语义修正**：e3d-attlib 老启发式 7/12 处错误（type=4↔5 Text/Reference 互换；type=7/8/9 Direction/Position/Orientation 重新指派；type=10/11/12 不是 Array 变体）
- **isUDA 机制**：`hash > 0x171FAD39` (`0x171FAD39` = base-27 dehash 上界) 为 UDA
- **attlib.dat 表名修正**：sub_55F4FFC = ATGTIX（2-word `(hash, page*512+offset)` 索引，不是 ATTR），sub_55F594C = ATGTSX（3-word 系列，不是 ATNAIN）；ATGTDF 保持 IDA 名
- **ATNLOG 字节读取公式**：`page_cache[(page-1)*512 + (word_offset-1) + atgtdf_index]`；元素记录使用直接 ATGTDF-索引 slot 数组
- **chain-fallback 属性**：`unk_5DAEB9C` = `0xCC7D2` = `db1_dehash` → "SYNO"（synonym）；用于 raw_word==0 时的 overflow attribute 重定向
- **noun→attribute 真实来源**：不是 attlib.dat 直接得到；通过 `DB_Noun::ReadData` (0x58D6D20) → `convertToDabType` → DGTALT (op 536 `db_get_att_list_for_given_template`) → DB 文件内的 template structures (dword_6A54024 数组) → sub_55BC8DC (核心字节读取器)

### Verified — Fixture 验证

- **ams1112_0001 fixture**：两个不同元素的 ATNLOG 解码均成功输出语义有意义的值
  - NXTR 元素（refno=17496/9621）：19/26 非零 slot，4 个 named（SIZE/DTYP/NAME/RPTX/TYPE/QUAL/DEPEND）
  - STWALL 元素（refno=17496/924，Structural Wall）：32/61 非零 slot，6 个 named；**DEPEND = -0.0872**（语义合理）、**TRUN = 1599933000**（Unix timestamp ≈ 2020-09-12，看起来是元素最后修改时间）
- 78% 系统属性名匹配率（5180/6644，0 disagreements）

> 实际代码改动在两个兄弟仓库 `D:/work/plant-code/e3d-attlib/`（parser 完全重写 + system_names + exmap 模块）和 `D:/work/plant-code/e3d-io/`（atnlog 字节解码模块 + 集成测试），它们不在本仓库 git 跟踪范围内但其 IDA 证据来源与本仓库 `goals/`/`ida_exports/`/`docs/` 同步落地。

## 2026-04-13

### Added

- **pdmsdb_engine_v2 crate — core.dll db1~5 全量复刻引擎**
  - `db1`: PageStore (LRU 缓存+页读写+分配+脏页 flush+预读) + PageLockManager (lock_count + referenced bit)
  - `db2`: HeaderView + SessionChain + SessionBuilderV2 + HeaderUpdaterV2 + ExtractManager + DbLookupTable
  - `db3`: IndexPageView + search_refno (FHSRCH) + upsert_refno (FHXPND) + split (FHSPLT) + delete_refno (FHDELT) + IndexTableIterator (FHITER) + scan_all_entries
  - `db4`: RecordReaderV2 + RecordWriterV2/DataPageBuilderV2 + ElementRecordView + CurrentElement (CE 导航栈) + AttrValue/AttrType + ExplicitBlock + ElementRefs + ElementBuilder
  - `db5`: open_read_db + open_write_db + commit_session + TransactionManager (set_mark/undo) + compact_database + refresh_sessions
  - `fortran_io`: FileToken + DirectAccessToken + RetryPolicy (SYWAIT + FHSWIT 重试)
  - `compare`: LegacyOracle + CoreDllOracle 比对工具
  - 32 个测试全通过，4590 行 / 37 个源文件
  - 开发计划文档: `docs/2026-04-13-pdms-db-engine-v2-plan.md`
  - GitHub Issues #2~#9 跟踪

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
