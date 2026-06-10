# Research: E3D / PDMS DABACON 数据格式逆向证据与决策

> Phase 0 输出。基线:AVEVA Everything3D **2.10** `core.dll`(base `0x10000000`),IDA Pro 实时反编译 + 真实样本(sam7200 / acp7002 / ams1112 / amssys)交叉验证。
> 完整依据见 `.planning/2026-06-05-e3d-db-offline-attr-parser/findings.md` §1–§22 与 `docs/e3d 数据库分析/E3D_DB_文件格式规范.md`。

## 关键决策(Decisions)

### D1. 属性 offset 的权威来源 = 模式库 `*vir.dat` 的 type-def(NOT attlib,NOT 运行时累加)

- **曾经的假设(已推翻)**:offset 来自 attlib 的 noun×attr 矩阵 / 运行时 DABACON 累加。
- **结论**:type-def(含 offset)预存于磁盘模式库 `*vir.dat`,装载时整块读入;`desc[5]&0xFFFFF` 即字偏移,`desc[8]` 为备(packed)偏移。
- **依据**:反编译 `db4_set_ce_from_extref`(0x1060F170)→ `db2_get_element_details`(0x10624400,二分查类型索引)→ `db2_open_template_db`(0x10621850,装载模式库);`db4_get_ce_att`(0x10612A50)/`db4_get_att_dets`(0x10611FF0)确证描述符结构;`desvir.dat` 实测 WELD type6 size3 off=11 → 与 sam7200 记录 w13 逐字吻合。
- **影响**:offset **完全可离线**,无需运行时——这是整套离线解码可行的基石。

### D2. 取值规则:sel / 标量 vs 计数前缀 / 实数低字在前

- `sel = (record[w10] >> 29) & 1` 选主 `desc[5]`(unpacked)/备 `desc[8]`(packed)。
- 标量(size==1 且非文本):值直存 `record[off]`,**无计数字**;`size>1`/文本:`count@off`,数据自 `off+1`。
- Bool(type5):`(record[off]>>bit)&1`。实数(2/6):sel=1 → 2 字 double,**低字在前**;sel=0 → 1 字 float。
- **依据**:`db4_get_ce_att` 整函数逐行(`v68` 标量判定 / switch 取值 / 定宽表 `dbl_10F68E90`)。

### D3. `sel=0` / packed 记录:干净数据中从不出现(定论,非缺口)

- 跨全库分类:有效 schema + `/`-名的干净记录中,sel=0 计数 = **0**(sam7200/acp7002/ams1112/amssys 全为 0)。
- ams1112 个别 sel=0(WALL)解出怪值,经查为**非干净/变体记录**,非解码 bug;WALL typedef main_off≠alt_off 有 9/46,确认 packed 是两套布局。
- **结论**:干净元素 100% sel=1;packed 路径"代码确证 + 合成验证"完成,**确认不适用任何干净数据**——真实样本验证为空集(非待办)。

### D4. UDA 真名离线**不可**还原(需字典库 udalib)

- `DEHASH`(0x1065B930)对 UDA 做 **base-64 有损短码**(`v=(hash-0x171FAD39)%0x1000000` → `:`+4 字符),含非标识符字符,非真名。
- `LXANAM`(0x100C7992)→ `ATATXT`(0x10467D70)→ `DB_Attribute::findAttribute`:真名取自属性注册表(UDA 由字典库 udalib `LXANAM/LXALEN/LXUNIT` 装入),**不经 DEHASH**。
- **结论**:离线可得 UDA hash + 有损短码 + 强类型值;真名/类型/单位需外部字典库 db(与 catalogue 同性质,缺文件)→ **范围外**。

### D5. `0xFFF` 族 UDA = 序列化 PDMS 表达式(派生属性),非字面量

- 反编译 `EXRTPD`(0x10080F62,表达式→文本):opcode 用 ASCII 字符 switch;`'j'`(0x6A)=属性引用,`'k'/'l'/'m'/'s'`(0x6B/0x6C/0x6D/0x73)→ `DORTXT(...)` 几何字面量(方向/取向/位置/AT)。
- 外层封装 `[len][0][count][sublen][1] <RPN...>`;复用仓库已有 `crates/parse_pdms_db/.../expression_payload.rs`(剥 5 字头)即可解。
- **实测解码率**:sam7200 **98.8%**(647/655)、acp7002 95.4%(3.4 万条)。剩余为几何分量美化,非阻塞。

### D6. 写 = COW + 新会话(`db5_save_work`),页无校验和

- `db1_read_page`(0x10630C20)仅 FHDBRN、`db1_write_page`(0x10633FB0)刷原始页 ⇒ **页无 checksum**,就地改值字节(大端 + 实型低字在前)即合法可读回。
- `db5_save_work`(0x105E9C80):claim → 校验会话 type/sesno+1 → 分配新会话页 → 重映射 db-block 属性 → 刷脏/COW 页 → 重指 page0 `0x28` → 解锁。
- 开库契约:`db2_get_db_int_att`(0x10622F20)/`db2_find_current_db_block`(0x10622DC0)从 page0[w10] 所指会话页装入 db-block(`sesno=+12 / end=+20 / index_root=+28 / claim=+36`)⇒ "改会话页 + 重指 page0 w10" 对真实 E3D **机制完备**(`w11` 次会话指针保守不改)。

### D7. B 树写入:正确性判据 = `nav_ok`(对齐 PDMS 宽松分隔键)

- 反编译 `db3_change_table_entry`(0x1061DEA0)/`db3_split_node`(0x1061BA50)/`db3_split_root`(0x1061C340)。
- 索引/表页(type5):7 字页头、`word6`=空闲字数、条目自 word7 升序、容量 `dword_10F68F4C`;插入二分定位→腾位→`word6-=4`;满则分裂(分裂点约半数,向父递归)。
- **关键认知**:正确性判据是 `nav_ok`(二分下降能到每个键),**非**"分隔键==子树最小";原始 sam7200 树本就有宽松分隔键(PDMS 删除不收紧)。节点合并刻意不做。

## 关键函数 / 地址速查(2.10, base 0x10000000)

| 类别 | 函数 | 地址 |
|---|---|---|
| 读·取值 | `db4_get_ce_att` | 0x10612A50 |
| 读·描述符 | `db4_get_att_dets` | 0x10611FF0 |
| 读·DA/list | `db4_get_ce_da_list` / `db4_get_list` | 0x1060FA80 / 0x1060CE20 |
| 读·默认值 | `db4_get_ce_att_default` | 0x1064E630 |
| schema·装载 | `db2_open_template_db` | 0x10621850 |
| schema·类型索引 | `db2_get_element_details` | 0x10624400 |
| schema·设置当前元素 | `db4_set_ce_from_extref` | 0x1060F170 |
| schema·开所有 | `DB_SchemaMngr::openAllSchemas` / `DB_DBSchema::openSchema` | 0x10498BE0 / 0x10497310 |
| 写·提交 | `db5_save_work` | 0x105E9C80 |
| 写·页 | `db1_read_page` / `db1_write_page` | 0x10630C20 / 0x10633FB0 |
| 写·B树 | `db3_change_table_entry` / `db3_split_node` / `db3_split_root` | 0x1061DEA0 / 0x1061BA50 / 0x1061C340 |
| 开库契约 | `db2_get_db_int_att` / `db2_find_current_db_block` | 0x10622F20 / 0x10622DC0 |
| 哈希 | `db1_hash`(base-27, +0x81BF1) / `DEHASH` / `PDMS_Hash::IsUDA` | — / 0x1065B930 / 0x10001BC0 |
| UDA 表达式 | `EXRTPD` | 0x10080F62 |

## 证据对照(结论 → 反编译 + 真实字节)

| 结论 | 反编译佐证 | 真实样本验证 |
|---|---|---|
| WELD.POS off=13 | `db4_get_ce_att` desc[5]&0xFFFFF | sam7200 `/WB1` w13 = (9630,8072,5282.5) |
| offset 源=模式库 | `db2_open_template_db` | desvir.dat WELD type6 size3 off=11 → 记录逐字吻合 |
| 实数低字在前 | `db4_get_ce_att` Real 分支 | POS 双精度逐字节比对 |
| 干净数据全 sel=1 | `record[w10]>>29` | 4 库全库分类 sel0-clean=0 |
| 写仅 page0 变 | `db5_save_work` 重指 0x28 | 多版本 demo 字节 diff 仅 0x28..0x2B |

## 阻塞 / 范围外(Blockers, gated)

- **B1 真实 running-E3D round-trip 取证**:写侧最终判据;需用户侧可用 E3D 环境(本环境仅静态 db + IDB)。
- **B2 `pdms_io` 整 crate 构建**:`rs-core ↔ surrealdb-3.1` `FromValue::from_value` API 漂移(7+ 处),项目外兄弟 crate;E3D I/O 经独立 sub-crate 旁路,不被阻塞。
- **B3 UDA 真名**:需 udalib 字典库文件(缺)。
- **B4 `0xFFF` UDA AST 级语义编辑**:原始字重写已可(S8);语义级编辑显式范围外。
