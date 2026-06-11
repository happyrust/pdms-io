# E3D / PDMS 数据库格式与解析 — 总索引

> 基线:**AVEVA Everything3D 2.10** `core.dll`(`D:\AVEVA\Everything3D2.10\core.dll.i64`,base `0x10000000`)。
> 方法:现有分析 + Rust 地面真值(`pdms-io` / `rs-core`)+ 实时 IDA(ida-pro-mcp)+ 真实样本十六进制走查,四方交叉核验。

## 交付物

| 文件 | 内容 |
| :--- | :--- |
| **`E3D_DB_文件格式规范.md`** | 磁盘格式:页布局/头部(PdmsHeader)/页类型/会话页/B-树索引/元素页/属性编码/attlib;§11 真实样本走查;§11.6 属性值实测 |
| **`E3D_DB_解析指导.md`** | db1–db5 五层解析机制;端到端解析算法;NOUN 元数据双路径;§11.4 attlib 真实访问机制(IDA);**§12 写入/保存路径(db5_save_work)**;**§13 B-树写侧(插入/分裂)**;**§14 维护(压缩/刷新/多写)** |
| `assets/e3d_db_file_layout.png` | 文件页布局 + 头部 |
| `assets/e3d_db_parse_flow.png` | 解析链路(refno→字节) |
| `assets/e3d_noun_attr_metadata.png` | NOUN 属性元数据解析 |
| `noun_hash_table_base27.json` | **修正版** noun 哈希表(1932 项,base-27,往返校验通过) |
| `e3d_db_reader_v2.py` | **可运行参考读取器(修正版)**:头部 / 会话链 / B-树枚举 / 元素+POS 解码 / noun 类型直方图 / **`--json` 导出** / **`--attrs` 命名属性解码**。已在 **sam7200(设计库)+ acp7002(目录库)双验证** |
| **`离线属性解析_总结.md`** | ⭐ **入口总结**(2026-06-06):全离线属性解析的数据模型全景 / 解析链 / 格式速查 / 工具链 / 验证结果 / 关键函数 |
| `desvir_typedef_probe.py` | 解析模式库 `*vir.dat` 的 type-def(**属性 offset 的磁盘来源**),验证 WELD.POS offset |
| `e3d_attr_decoder.py` | 离线属性解码核心:`Schema`(跨库选型)/`decode_full_element`(整元素:隐式+DA+NAME)/`decode_da_list`(含跨页链)/`extract_names` |
| `e3d_export.py` | 整库导出 JSON;`--cat <db>` **跨库引用解析**(设计↔目录:SPRE→SPCO/PSPE→SPEC) |
| `e3d_tree.py` | 由 owner refno **重建模型层级树**(ZONE→STRU→…/EQUI→NOZZ) |
| `type_enum_probe.py` | 全库统计 type-def 描述符 `type` 枚举分布 + 示例(支撑 §7.7.7 type 表;只读) |
| `uda_probe.py` | 巡检元素 UDA 存储(hash>0x171FAD39):类型/原始值/结构 + 直方图(支撑 §7.10;只读) |
| `uda_expr_probe.py` | 解码 `0xFFF` 族**派生/表达式 UDA**→可读 PDMS 表达式(RPN + DORTXT 几何字面量;sam7200 98.8%;支撑 §7.10.5;只读) |
| `e3d_write.py` | **写侧(在位)**:安全就地编辑定长内联值(实/整/引用;仅副本;基于"页无校验和"§12) |
| `e3d_write_full.py` | **写侧(完整 COW + 新会话 CRUD)**:S1 内联 / S2·S6 DA 文本(同页·跨页·链式) / S3 新增 / S4 删除 / S5 任意键插入+分裂/长高 / S7 成员列表 / S8 UDA·DA 条目;多版本 + 仅副本(§12.6);**14 自检 demo 全 PASS**(S1–S9,含 Slice 9 `verify_commit` + `batch`) |
| `tools/e3d_decode_rs/`(Rust) | std-only **读取/导出 CLI**(`--json`/`--cat`),现为 `crates/e3d_io` 的**薄消费者**(已消除重复解码器,解码单一真源);输出与改前逐项一致(10392/1209/145) |
| `../../crates/e3d_io/`(Rust crate) | 独立 std-only crate:**读**(全属性/NAME/`resolve_refs`)+ **写**(S1–S8 COW CRUD,含 S4 delete);+ **安全事务层**(`verify_commit` 写后自校验 / `batch` 多笔合一会话 / `dry_run`·`element_diff` / `delete_guards` 护栏);+ **页源/读视图**(specs/002:`PageSource` trait,`InMemory`/`PagedFile` LRU,`Rdb` 惰性影子页——`btree_find` 点查/`leaves`/`session_chain`/`element_record`);稳定 `EdbWriter` API + 类型化 `E3dError`;`cargo test` **38+1 测试**;现为**全仓唯一格式核心**(`PdmsIO` 门面整体委托) |
| `../../crates/e3d_io` `e3d-io`(CLI) | std-only 命令行(基于 `EdbWriter`):读 `show`/`refs`;写(COW)`rename`/`set-pos`/`delete`/`insert`(`--out`/`--cat`/`--inplace`) + 批量 `plan`/`apply`(护栏 `--force`/`--yes`);端到端 smoke 验证 |
| **`../../specs/001-e3d-data-format/`** ⭐ | **spec-kit 规范**(格式的 WHAT/WHY + 字节级模型 + 契约 + 任务):`spec.md`(US1–US5 / FR-001..022 / SC-001..010)·`data-model.md`·`contracts/decode-contract.md`·`plan.md`·`tasks.md`。本索引 ↔ specs/001 互为入口 |
| **`../../specs/002-e3d-io-engine-consolidation/`**(已 Implemented) | **三引擎收敛规范**:v1 `PdmsIO` 自带解析 / `engine_v2` / `e3d_io` → 单核心+门面;`PageSource`/`Rdb`、API 冻结契约、孤岛退役(`engine_v2` 39 文件 + v1 写路径,知识归档 `../engine-v2-archaeology.md`);2026-06-11 Phase 0–4 全清,SC-001..006 核销 |

## 关键已验证结论（含三大纠错）

1. **页大小 = `header[0x34] × 4` = 2048 字节**(该字段是"字数"非字节数)。旧 `e3d_db_reader`/`detect_page_size`/FILELIST 把 512 当字节是错的。[4 样本实测]
2. **`db1_hash` 是 base-27 + 偏移 `0x81BF1`**(非 base-26)。`noun_hash_table.json`/旧《完整指南 §5》的 base-26(PIPE=0x463E9)是错的;**正确 PIPE=0x9CAF3**。[rs-core 源 + 实测]
3. **`core_dll_数据库读写函数.md`(401 函数表)是另一构建的地址**,与当前 2.10 不匹配;应以两份 summary 的"E3D 2.x"列为准(经函数自标识字符串核验)。
4. **元素 on-disk `noun` 字段 = base-27 db1_hash**,`db1_dehash` 直接得类型名(`ele_data_0` 0x97247→"WELD")。
5. **属性 offset 的磁盘来源 = 模式库 `*vir.dat`(非 attlib)**(2026-06-06 更正):type-def(含每属性 offset)预存于 `%AVEVA_DESIGN_EXE%/*vir.dat`(desvir/catvir…),由 `db2_open_template_db`/`db2_get_element_details` 按 noun 二分查载入,**装载时整块读盘、非运行时累加**。这正是早期纯 attlib 启发式对不上的根因。详见格式规范 §7.7。
6. **元素属性离线解析已全链闭环并规模化验证**(2026-06-06):`noun → 模式库 typedef → offset → 隐式值`,`record 头 → DA/显式区 → 显式值(含 NAME)`,`owner → 层级`,`引用(dbno,refseq) → 跨库解析`。WELD `/WB1` POS=(9630,8072,5282.5) 逐字吻合;整库 sam7200 解出 6536 元素 / 1144 命名 / 140 类型;跨库 sam7200→acp7002 目录(SPRE→SPCO)。**先前"未闭环"的完整命名属性集 + 精确 offset 已解决**。入口见 `离线属性解析_总结.md`。
7. **取值规则全权威化 + UDA 容器定位**(2026-06-06,`db4_get_ce_att` 整函数反编译 + 全库 type 实测):type 枚举(2/6 实数标量/向量、3/7 整、4/8/16 引用、5 布尔、10/15 文本、14/18 UDA表、9/17 特殊、19 未用);`v68` 标量(size==1 非文本=无计数字)vs 计数前缀;`sel`(record[10]bit29)=主/备 offset + packed(1字 float)/unpacked(2字 double 低字在前);定宽表 `dbl_10F68E90`(运行时填充,反推 Real0.5/Int1.0/Ref0.5/Text4.0)。详见格式规范 §7.6.2/§7.7.7。
8. **UDA 元素存储离线打通**(2026-06-06,§7.10):UDA(`hash>0x171FAD39`,`PDMS_Hash::IsUDA`)= DA/显式区一条以 UDA hash 为键的条目,带声明类型;`db4_get_ce_att` 走 `off==0`→DA 扫描命中。real/int/text/ref 值**纯离线可解**(sam7200 实测 `ref=(15195,2418)`、`real=(192,192)`、`text='D'`;453/6536 元素带 UDA、765 条)。仅 **UDA 名**(`udalib`/`LXANAM` 字典库,catalogue 同性质)与 `0xFFF` 族表达式 UDA 令牌语义为后续。工具 `uda_probe.py`。

## 端到端解析链(已在真实字节验证)

```
Page0 头部(latest_ses_pgno) → 会话页(type 3, index_root) → B-树(type5/noun=0xCC47DF)
  → RefnoDataLoc{pgno, offset:20b} → att_off = pgno*2048 + offset*2
  → 元素记录 EleRawData(refno/noun/parent/page_no + 隐式区) → 属性值
```
样本走查(sam7200_0001):会话#36/Administrator/2023-05-21 → 索引根3377 → 叶1447 → 元素(refno 0x5C20/0x0F80);WELD 元素隐式区实测 POS=(9630.0, 8224.0, 5130.5)。

## db1–db5 关键函数(2.10, base 0x10000000)

| 层 | 代表函数@地址 |
| :--- | :--- |
| db1 页管理 | get_page@0x10634DC0, read_page@0x10630C20, write_page@0x10633FB0, update_page@0x10635E30 |
| db2 头部/会话 | read_page@0x10628EC0, modify_header_page@0x10620F60, get_session_pgid@0x10621480 |
| db3 B-树 | get_table_entry@0x1061E9F0, start_table_search@0x1061EC50, scan_index_page@0x1061CF20 |
| db4 元素/属性 | create_element@0x1060CCA0, get_ce_att@0x10612A50, get_att_dets@0x10611FF0 |
| db5 访问 | open_read_db@0x105E4940, close_db@0x105E4D60, save_work@0x105E9C80 |
| attlib | DB_Noun::internalGetField@0x1084F7C0, DB_Attribute::internalGetField@0x10850888, ATGTIX 加载@0x10852A64, 编排器@0x10851210 |

## 待办（状态;大多已收口）

1. ~~离线复刻 attlib 读取器(产出"命名属性 + 物理 offset")~~ **✅ 已完成 / 路线更正**:offset 的磁盘来源**不是 attlib**,而是模式库 `*vir.dat` 的 type-def(结论 #5,§7.7)。早期 attlib 公式启发式作废;读侧已全链闭环(隐式/DA/NAME/owner/引用/UDA),Python + Rust 双实现。
2. ~~写入/保存路径(`db5_save_work`)~~ **✅ 已实现 + 验证**:离线 COW + 新会话 CRUD(改内联值/变长 DA/成员/UDA + 新增/删除 + B 树插入分裂长高),`e3d_write_full.py`(13 demo)+ `src/e3d_decode.rs`(S1–S8,15 测试);§12.6。
3. ~~跨页拼接(`EleMembers`/属性数据跨页)~~ **✅ 已完成**:DA/成员链式遍历(`decode_da_list`,node word3/4),读写双向(写侧 S6/S7 多页 COW relocation)。
4. ~~`detect_page_size`(×4)+ `noun_hash_table_base27.json`~~ **✅ 已修**(`src/defines.rs::detect_page_size`、reader 头部字段正名 §13;生产 `io.rs` 本就用 `detect_page_size_by_probe`)。
5. **剩余(阻塞/范围外)**:① 真 running-E3D round-trip 取证(需真 E3D)② 并入整 `pdms_io` crate(待 rs-core↔surrealdb-3.1 兼容,项目外)③ `0xFFF` UDA AST 级语义编辑 + UDA 真名(字典库)—— 均非阻塞当前能力。

---

*配套:`E3D_DB_文件格式规范.md` / `E3D_DB_解析指导.md`。生成于 AVEVA E3D 2.10 core.dll 实时逆向 + 真实样本核验。*
