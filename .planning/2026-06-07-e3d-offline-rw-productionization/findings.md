# Findings:E3D 离线读写 生产化与集成（方案输入）

> 本计划的事实输入 = 前序计划 `2026-06-05-e3d-db-offline-attr-parser` 的成果(详见其 `findings.md` §1–§22 与 `docs/e3d 数据库分析/`)。此处只摘"作为本方案前提"的已验证状态与待解阻塞。

## A. 已验证状态(read + write,Python + Rust 双实现)

### A.1 读侧(全闭环)
- 给定元素记录即解:`noun · NAME · refno · owner · 全部隐式(typedef)属性 · 全部 DA/显式属性 · UDA(强类型值 + `0xFFF` 派生表达式 98.8%)· 引用(本库+跨库目录)`;重建 owner 层级树;整库 JSON 导出。
- offset 权威来源 = 模式库 `*vir.dat` type-def(非 attlib;前序 §8)。规模验证:设计/目录/系统 + 103MB 大库(ams1112 ~42.2 万元素)。
- Rust↔Python 属性级对齐:目录库 100%、设计库 99.95%(implicit 0 mismatch)。

### A.2 写侧(离线 COW + 新会话,全 CRUD)
- `db5_save_work` 落地:绝不改旧页、改动追加(COW)、仅原子重指 page0 会话指针(0x28)⇒ 多版本(旧会话逐字节不变可读)。开库契约 IDA 确证(`db2_get_db_int_att`/`db2_find_current_db_block`,前序 §15.3)。
- 切片:S1 内联值 / S2·S6 DA 文本(同页·跨页·链式·增长) / S3 新增(最大键) / S4 删除 / S5 任意键插入+节点分裂/根长高 / S7 成员(子 refno)列表 / S8 UDA·DA 条目改增删。B 树正确性判据 = `nav_ok`(对齐 PDMS 宽松分隔键);节点合并刻意不做。
- 双实现:Python `e3d_write_full.py`(13 自检 demo 全 PASS)+ Rust `src/e3d_decode.rs`(S1–S8,edition-2024 **15 测试**:含读计数/POS、`resolve_refs`、内联/DA/UDA/成员/B 树合成+真实分裂)。

### A.3 Rust 模块特性
- `src/e3d_decode.rs` = **std-only**,无第三方依赖;`Edb` 持 `Vec<u8>`(COW=append);`walk` word6 界定;公共读写函数 + `CowReport`。

## B. 待解阻塞(本方案要解决)

### B.1 真实 E3D round-trip 未取证
- 写侧验证当前仅靠:写后自读(多版本)、字节 diff 仅 page0、开库契约反编译。**未在真实 E3D 中打开 COW 写出的库**确认加载/显示/无损。
- 未定细节:新建 DA/list 专用页的**完整 PDMS 页型保真**(读取器只经 rec[6]/节点头到达 DA,不校验页头,但真机刷新/压缩是否容忍待验);`page0[w11]`(次会话指针字)语义未定,保守未改。
- **前置**:需可用真实 E3D 环境 / 可写工程库(用户侧)。

### B.2 `pdms_io` 整 crate 构建阻塞(项目外)
- 链路:`pdms_io → aios_core(../rs-core) → surrealdb-3.1`;`rs-core` 的 `FromValue::from_value` 等 **7+ 处** 仍用旧签名(`anyhow::Result<Self>`),新 surrealdb 要求 `Result<Self, surrealdb::Error>`。属**项目外兄弟 crate** 对移动中 git 分支的迁移,非快速修复。
- NASM 已装(aws-lc-sys,经 surrealdb)。故 `src/e3d_decode.rs` 现仅以 edition-2024 临时 crate 验证。

## C. 关键约束(影响方案设计)
- 不能擅改 `rs-core`(项目外,需授权)⇒ Phase 3 倾向**抽独立 sub-crate 旁路**(std-only,不依赖 rs-core/surrealdb)。
- 保持 E3D I/O 核心 std-only 纯净(利于独立构建/测试/复用)。
- 写操作默认仅作用于副本;破坏性 inplace 需显式开关 + 二次确认。

## D. 关键函数/地址速查(2.10, base 0x10000000)
- 写:`db5_save_work` 0x105E9C80 · `db3_change_table_entry` 0x1061DEA0 · `db3_split_node` 0x1061BA50 · `db3_split_root` 0x1061C340 · `db1_write_page` 0x10633FB0 · `db2_get_db_int_att` 0x10622F20。
- 读:`db4_get_ce_att` 0x10612A50 · `db4_get_att_dets` 0x10611FF0 · `db4_get_list` 0x1060CE20 · `db2_get_element_details` 0x10624400。
- (完整速查见前序计划 findings §6 / 总结 §7。)
