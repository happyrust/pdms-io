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
| `e3d_write.py` | **写侧**:安全就地编辑定长内联值(实/整/引用;仅副本;基于"页无校验和"§12) |

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

## 待办（按价值排序）

1. **离线复刻 attlib 读取器** → 稳定产出"命名属性 + 物理 offset"。**机制已逆清(解析指导 §11.4),但经验闭环未完成**:
   - 已知:`ATFIND`(线性查找)、`ATRDRC`(LRU 页缓存)、公式 `offset = attlib页[record(attr)][disp + noun_index − 2]`;POS 的 ATGTIX `combined=0x83787→rec1051/disp391`;noun 表在 ~file_page 2233。
   - 难点:`internalGetField` 含**跨页链式回退**(单步公式仅无冲突直读);noun 表**交错布局**(WELD 在 noun 表区而非 ATGTSX,故按 noun_hash 直接分组取不到其属性)。
   - 收尾路径:忠实实现链式查找 + 交错 noun 表索引提取 + 以 WELD-POS@word13 闭环验证。属独立聚焦实现工程。
2. ~~写入/保存路径(`db5_save_work`)~~ **✅ 已完成**(解析指导 §12:COW + 会话 + 批量刷脏页 + page0 重指向)。
3. **跨页拼接**:`EleMembers`/属性数据跨页(`defines.rs` 标注 TODO)。
4. 修正 `detect_page_size`(×4)、用 `noun_hash_table_base27.json` 替换旧表。

---

*配套:`E3D_DB_文件格式规范.md` / `E3D_DB_解析指导.md`。生成于 AVEVA E3D 2.10 core.dll 实时逆向 + 真实样本核验。*
