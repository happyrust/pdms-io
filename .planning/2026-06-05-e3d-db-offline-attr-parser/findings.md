# Findings:E3D db 逆向(支撑本开发方案)

> 均为本轮实时 IDA(core.dll 2.10, base 0x10000000)+ 真实样本(sam7200/acp7002 等)交叉验证的事实。
> 完整版见 `docs/e3d 数据库分析/`(格式规范 / 解析指导 / 索引)。

## 1. 基线纠错(Phase 1 依据)
- **页大小 = `header[0x34] × 4`**(该字段是 32 位字数,非字节)。4 样本实测均 = 2048。旧 `detect_page_size`/`e3d_db_reader` 把 512 当字节是错的。
- **db1_hash = base-27 + 偏移 0x81BF1**(`rs-core/src/tool/db_tool.rs::db1_hash`),非 base-26。
  - 验证:PIPE=0x9CAF3(641779)、ELBO=0xCA439、NAME=0x9C18E、WELD=0x97247、USER=0xD943A。
  - `noun_hash_table.json`(及旧《完整指南 §5》)用错误 base-26(PIPE=0x463E9),已重生成正确版 `noun_hash_table_base27.json`(1932 项,往返校验)。
- 元素 on-disk `noun`/`type_hash`(记录 +0x0C)就是该 base-27 hash;`db1_dehash` 直接得类型名(`parse.rs:460-462`)。

## 1b. 重要:生产代码现状(Phase 1 范围修正,2026-06-05 执行中发现)
- **页大小**:生产 `src/io.rs::PdmsIO::open` 用 `detect_page_size_by_probe`(io.rs:239)——按候选 [2048,4096,512] 探测会话页类型来定真实页大小(与本工具一致),注释已写明 header.page_size 不可靠(ams1112=512)。已有测试 `page_size_probe_test.rs`/`smoke_io_test.rs` 覆盖。⇒ **页大小问题团队已稳妥解决**;`defines::detect_page_size` 是未被 IO 使用的遗留辅助函数(本次仍修正为 ×4 以消除潜在误用,但不影响生产路径)。
- **base-27 哈希**:`rs-core db1_hash` 本就是 base-27(正确)。错误仅存在于旧 artifacts(`noun_hash_table.json`、旧 python 读取器、旧《完整指南》)。
- ⇒ Phase 1 的实际代码改动很小(detect_page_size 清理);"纠错"主要价值是修正**旧 artifacts 与文档**,而非现行 Rust 主路径。

## 2. 磁盘格式关键(已实测)
- 文件=固定页序列;page0=PdmsHeader(大端,0x40B);page 类型:1 RefArray/3 Session/5 Data/7 Special(元素)/8 Index。
- 会话页(type3):`latest_ses_pgno`→最新会话;含 sesno、index_root_pgno、claim、时间戳、计算机名;`last_ses_pgno` 形成会话链。
- 索引页:type5 且 `noun==0xCC47DF`;`RefnoDataLoc{refno(2×u32), pgno, offset:20bit, flag:12bit}`;`att_off = pgno*page_size + offset*2`。
- 元素记录 `EleRawData`:`[impl_flag/count][refno 8B][noun 4B][owner 8B]...隐式payload...members(flag0x0002)...explicit(flag0x0001)`。
- 隐式属性 offset 编码:`0`=Pseudo;`<0x100000`=字偏移(byte=off*4);`≥0x100000`=BOOL 位打包(bit=off>>20, word=off&0xFFFFF)。
- POS = 3×大端 double;实测 ele_data_0(WELD)POS@word13 = (9630.0, 8224.0, 5130.5)。

## 3. attlib 寻址机制(Phase 2/3 核心依据)
- 元数据在 `attlib.dat`(分页 2048;段指针在 file 0x800 的 8×u32;数据页 = 0x1000 + page*2048,512 字/页)。
- 查表 = `DB_Noun::internalGetField`(`sub_1084F7C0`),表 ATNAIN/ATNATX/ATNALO;属性侧并行 `DB_Attribute::internalGetField`(`sub_10850888`),共享页缓存。
- 辅助:`ATFIND`(`sub_10450144`,线性查找→1-based 索引);`ATRDRC`(`sub_1044FC20`,LRU 页缓存:`FHDBRN` 把文件页读入槽,返回**槽号**)。
- ATGTIX 加载(`sub_10852A64`):每条 **2 word `[attr_hash, combined]`**,`record=combined/512`、`disp=combined%512`(= rs-core `AttlibAttrIndex`)。
- 加载器/编排:`ATTOPE`(`sub_10851210`,`FHFIND` 打开)。`dword_11C2A860` 本质=attlib 文件页缓存。
- **更正(2026-06-05 探针+IDA 复核)**:`DB_Noun::internalGetField` 的 `ATNAIN` 路径不是 `record(attr)+noun_index`。真实角色:
  - `v47[6]` 经 `sub_10852A64(ATGTIX)` 填 `unk_11BFA080/dword_11C02080`:**noun_hash -> (record, disp)**。实测 WELD=idx82, record=2225, disp=1;PIPE=idx111。
  - `v47[4]` 经 `sub_10852E20(ATGTDF)` 填 `unk_11C12080/dword_11C12210`:**DB_Noun 字段定义**。实测仅 88 项,含 NAME/DBTP/DISPLY 等;**不含 POS**。
  - `DB_Noun::internalGetField` 实际矩阵首跳:`v = page[record(noun)][disp(noun) + field_index - 2]`;为 0 时按默认字段/链式回退。
  - `POS` 位于 `v47[2]` 属性 ATGTIX:idx27, record=1129, disp=127, combined=0x8D27F。旧记录的 `POS -> 0x83787` 来自属性记录相邻字段/ORI hash 污染,不是可用的 POS 矩阵坐标。

## 4. 实测坐标(Phase 2/4 锚点)
- POS hash = 0x853B1(545713);属性 ATGTIX 实测:`combined=0x8D27F → record=1129, disp=127`(来自 `v47[2]`/physical page 1682)。
- noun 哈希 ATGTIX 在 `v47[6]`(directory 0x8BC, physical page 2235 起),布局为 `[noun_hash, combined]` 交错 pair;WELD=idx82, `combined=0x116201 → record=2225, disp=1`;PIPE=idx111。
- 新探针:`docs/e3d 数据库分析/attlib_atnain_probe.py` 可复现上述表装载与 WELD/POS 归属判断。
- WELD 元素(ele_data_0):refno=(0x5C20,0x161A);impl_len=46;POS@word13=(9630.0,8224.0,5130.5)。
- WELD 元素(sam7200_0001):索引叶 `refno(0x5C20,0x1618)` → `pgno=807`,`offset=524`,`byte_off=1653784`;元素头 noun=`0x97247("WELD")`;POS@word13=(9630.0,8072.0,5282.5)。结合 `attlib` 中 `WELD.DISPLY/PRDISP` 包含 `POS(0x853B1)` 且 `POS` schema=`TYPE=8,SIZE=3,DEFI=5,UNIT=DIST`,已形成最小 `noun→attribute→value` 闭环。

## 5. 已知风险(Phase 3 重点)
- `internalGetField` 含跨页**链式回退**(`i = dword_11C2A860[...]` 回填后继续):单步公式仅"无冲突直读"。
- 旧风险"交错 noun 表 → noun_index 提取易错"已收窄:WELD/PIPE 可由 `v47[6]` ATGTIX 稳定定位;当前新风险是**POS 不属于 DB_Noun 字段表**,需转向 `DB_Attribute::internalGetField`/属性元数据侧来拿物理 offset,不能继续用 `DB_Noun::internalGetField(POS)`。
- 回退方案:用 core.dll 运行时(IDA `py_eval` 调 `db_get_attribute_list` opcode 60)产出 ground truth,对照离线实现。

## 5b. 属性 offset 离线推导调查(2026-06-05, IDA 不可用时的纯数据实验)
> 环境:本次 IDA Pro MCP **未连接**(可用服务器仅 best-mcp/claude-mem),故无法读 `db4_get_ce_att`/`db4_get_att_dets` 代码;改用 attlib.dat 纯字节实验。新增探针:`attlib_offset_probe.py`/`attlib_segment_probe.py`/`attlib_deftable_probe.py`/`attlib_nounfields_probe.py`/`attlib_hash_locator.py`。
- **排除假设 A**:offset ≠ `POS属性页[disp(POS) + noun_index(WELD) - 2]`。该列实为 CATEG 字符串 `"Positional"` 的文本区(0x50 0x6F 0x73...)。
- **排除假设 B**:offset ≠ `WELD noun页[disp(WELD) + attr_index(POS) - 2]`。该列为 `0`/`0xFFFFFFFF` 哨兵(POS 非 noun 字段)。
- **noun 字段无完整 DAB 布局**:WELD 的 88 个 noun ATGTDF 字段里,唯一属性列表是 `DISPLY`(13 项)/`PRDISP`(28 项),均为**显示顺序**,字数远少于元素隐式区,不能直接给存储 offset。
- **offset/布局数据实际位置 = 段 `v47[1]=0x4`(record 4~~640 大表,"Syntax/定义")**,经 `attlib_hash_locator.py` 实测:
  - POS(0x853B1) 出现 **463** 次、ANGL(0xBCBFF) **220** 次,几乎全在 v47[1]。
  - 含**属性定义**条目:如 rec524 `... POS 939199(=DIST,UNIT) 3 ...`(SIZE=3 内联)。
  - 含**属性序列**条目:如 rec5 `... POS 538503(=ORI) 13 ...`(连续属性列表,疑似 DAB 布局)。
  - **WELD 全表仅 1 次**(noun ATGTIX rec2236)。⇒ noun→布局**按 noun 索引而非 hash 关联**,纯离线复刻需先破解 v47[1] 记录语义 + noun 索引映射。
- **结论**:POS 在元素内是 **count 前缀变长编码**(实测 WELD: w11=count=3, w13 起 3×double;w7=0x268001/w10=0x20098000 疑似位打包布尔)。offset 由 `db4_get_ce_att` **运行时按 DAB 序列累加**得到,非 attlib 静态单元。彻底离线化需破解 v47[1]("Syntax")——高不确定、可能多轮(命中计划 §Phase3 风险与"环境受阻"评估)。
- **可达 vs 受阻**:`noun名 + 属性schema(TYPE/SIZE/UNIT/CATEG) + 属性值(按 TYPE 模式)` 当前环境**可达**(软闭环已成);`任意属性的精确 offset 公式`受阻,需 (a) IDA 活进程读 db4 / (b) 可构建 Rust / (c) 深逆 v47[1]。

## 7. 属性 offset 权威机制(2026-06-05 晚, IDA 恢复连接后反编译 db4)
> ⚠ **2026-06-06 更正**:本节末"仍存疑:offset 由 DABACON 运行时按 DAB 顺序累加构建"的推测**已被推翻**。offset 实为**磁盘持久化**在模式库 `*vir.dat` 中,装载时整块读入,**完全可离线**。详见 **§8**(已实测闭环 WELD.POS=(9630,8072,5282.5),纯离线)。
> IDA Pro MCP 中途恢复(core.dll @127.0.0.1:13337, idb=D:\AVEVA\Everything3D2.10\core.dll.i64),反编译 `db4_get_ce_att`/`db4_get_att_dets` 后**又掉线**(fetch failed)。以下为掉线前已确证结论。
- **`db4_get_att_dets`(0x10611FF0, ver 4.5.1)**:在当前元素(CE=`dword_11599348`)的**类型定义表**里线性查描述符。
  - CE 60 字节/项:`+8`=idx,`+0x10`=typedef 指针,`+0x14`=元素数据缓冲,`+0x30`=db 句柄,`+0x40`=缓冲字索引。
  - typedef:`*(typedef+0x24)`=描述符数;描述符从 `typedef+0x38` 起,变长。
  - 查找:`while attr_hash != desc[0]: desc += desc[1]`(desc[1]=stride)。
  - 描述符字段:`[0]=hash, [1]=stride, [2]=type, [3]=size, [5]=offset(低20)+bit(高位), [8]=alt_offset`。
- **`db4_get_ce_att`(0x10612A50, ver 4.6.1a)**:取值。
  - `v16 = data_buf + 4*v12` → 元素**记录起始**;`(u16)*v16`=记录字数(WELD=46)=边界。
  - `v13=(*(v16+0x28)>>29)&1` 选主/备 offset:`v14=v13?3:0`;**`offset = desc[v14+5] & 0xFFFFF`**,`bit = desc[v14+5] >> 20`。
  - BOOL(type5):`(v16[offset]>>bit)&1` ⇐ 与 §7.2 位打包规则**完全一致**。
  - 标量/向量:连读 `v16[offset..]` size 个;实型(2/6)经 `dbl_10F68E90[type]` 定点换算。
  - 越界报 `4GCEA:...attribute offset is %d`(623)。
- **offset = 从记录起始计的字索引**。⇒ WELD.POS 的 `offset` 必为 **13**(记录 w13),与 §11.6/§11.7 实测 `POS@w13` **逐字吻合**。⇒ **机制层硬闭环已成立**(运行时硬路径 = 离线字节解码)。
- 类型等价(switch 实测):`2↔6`、`3↔7`、`4↔8`(packed↔unpacked 别名)。
- **仍存疑**:typedef(含 offset)是元素装载时 **DABACON** 在内存构建(按 DAB 顺序累加 size)。静态 IDB 读不到该内存表;**纯离线自动推导 offset** 需复刻 DABACON 装载器。offset 相关 attlib 段疑为 `v47[1]=0x4`(Syntax,POS 463 次)。下一步(待 IDA 恢复):定位 typedef 构建器(写 CE+0x10 / 写 desc+0x14)。
- **消费者**(非构建器,已排除):`db4_get_ce_da_list`(0x1060FA80,二级表),`db4_get_ce_table_att`(0x10613690,表属性)。db4_get_att_dets 调用方 26 处(0x10611B10/0x10617A60/0x10650000 等)。
- **CE/type-def 结构(由 `db4_ce_update_ok` 0x10611B10 复核)**:
  - CE 项(60B):`+0x10`=type-def 指针(`*v6`),`+0x14`=元素数据,`+0x20/0x24`=refno0/1,`+0x2C/0x30`=不定,`+0x2C`... ;`v6[8]`(=+0x30)=dbno,`v6[9]`=refno?,`+0x2C`(=+44)&0x1FFF 用作 dbno 编码。
  - type-def(`*v6`)字段:`+8`(与 `&unk_1431DBF9`/dbno 比较)、`+0x10`、`+0x24`=属性数、`+0x38`=描述符数组、`+388/+20/+412`(权限/状态)。
  - `db4_ce_update_ok` 用 `sub_10634DC0`(=db1_get_page)按 dbno+refno 取**元素数据页**;type-def 本身在更早的"设置当前元素"流程装载。
- **DABACON 线索**(dabacon 串搜索):`DB_Element::dabPutA…`(0x104E4AF0 "Testing RAW DABACON")、`DB_Attribute::setDabaconError`(0x10460270)、`sub_104E2DF0/104EF890`("Dabacon iteration")。DAB 布局(offset)疑由 DB_Element/DB_Noun 在定义/装载期紧致化。
- **下一步(待 IDA 稳定)**:定位写 CE+0x10 的"设置当前元素"函数 / 写 type-def+0x38 描述符(含 offset)的构建器;入口候选:db5 访问器簇 `0x105DDE20~0x105DF370`、`sub_10610AA0`(CE 就绪检查)、`sub_106104A0`。
- **环境提示**:本轮 IDA Pro MCP **连接不稳定**(多次 `fetch failed`/60s 超时);深逆 loader 需连接稳定后分批小步进行。

## 6. 关键函数地址速查(2.10, base 0x10000000)
db1 get_page 0x10634DC0 / read_page 0x10630C20 / write_page 0x10633FB0 / update_page 0x10635E30 / get_new_page 0x10635B00。
db3 insert_page_entry 0x1061B5C0 / split_node 0x1061BA50 / get_table_entry 0x1061E9F0。
db4 get_ce_att 0x10612A50 / get_att_dets 0x10611FF0 / copy_user_element 0x1060D820。
db5 open_read_db 0x105E4940 / save_work 0x105E9C80 / compact 0x105EA8B0 / refresh_work 0x105E8000。
attlib internalGetField(Noun) 0x1084F7C0 / (Attr) 0x10850888 / ATGTIX加载 0x10852A64 / ATTOPE 0x10851210 / ATFIND 0x10450144 / ATRDRC 0x1044FC20。
schema/template db: DB_SchemaMngr::openAllSchemas 0x10498BE0 / DB_DBSchema::openSchema 0x10497310 / db_open_template_db 0x105DC6F0→0x105F44E0 / db2_open_template_db 0x10621850 / db2_get_element_details 0x10624400 / db2_get_element_definition 0x10624AA0 / db2_get_db_int_att 0x10622F20 / db2_find_current_db_block 0x10622DC0 / db4_set_ce_from_extref 0x1060F170 / db4_there_is_no_ce 0x10610AA0 / db4_save_ce_stack 0x106104A0。注册表全局:tlu 数组 dword_11599778(24B/项)、count dword_115997E4、CE 栈 dword_11599348。

## 8. offset 的磁盘来源 = 模式库 `*vir.dat`（2026-06-06,IDA 反编译 + 实测纯离线闭环）
> 推翻 §7 末的"运行时 DABACON 累加"假设。type-def(含每属性 offset)**预存于磁盘**,装载时整块读入。

### 8.1 运行时链路(已反编译,均自证字符串)
- `DB_SchemaMngr::openAllSchemas`(0x10498BE0)→ `DB_DBSchema::openSchema`(0x10497310):构造路径 `%AVEVA_DESIGN_EXE%/<schema>.dat`,`FHFIND "OLD,READ" "DB,BL 512"`(大端,2048B/页)。
- → `db_open_template_db`(0x105DC6F0→0x105F44E0)→ `db2_open_template_db`(0x10621850,"2.1.1"):读头部 + 类型索引(tlu),注册到全局 `dword_11599778`。
- 装载元素:`db4_set_ce_from_extref`(0x1060F170)取元素记录 `word[3]=noun hash`,调 `db2_get_element_details`(0x10624400,"2.2.5")**二分查 tlu** → 取 skeleton K(typedef),写 `CE+0x10=typedef`、`CE+0x14=记录缓冲`、`CE+0x40=记录字索引`。
- `db4_get_ce_att`(0x10612A50)再用 typedef 描述符 offset 取值(§7.6)。

### 8.2 `*vir.dat` 磁盘格式(实测 desvir.dat)
- 页→偏移:`(page-1)*2048`(页 1=偏移 0)。链式:511 数据字 + 第 512 字=下一页。`db1_read_page`(0x10630C20)→`FHDBRN` 大端转主机序。
- 头部(页 1 大端):`w0==6`(魔数)、`w2`=模板类型 id(desvir=0xB0692)、`w5`=类型数(desvir=745)、`w7`=tlu 起始页(desvir=1516)、`w9+`=创建信息 ASCII。
- tlu:`count×7` 字、按 noun hash 升序;条目 `[0]=noun hash, [1]/[2]=skelK 页/字数(=typedef), [3]/[4]=skelI, [5]/[6]=skelJ`。
- typedef(skelK):`word9`=描述符数,`word14`=描述符数组起;描述符 `[0]=hash [1]=stride [2]=type [3]=size [5]=主offset|bit [8]=备offset|bit`。

### 8.3 offset→记录值 解码(实测)
- `sel=(record[w10]>>29)&1` → 选 `desc[5]`(sel=0)/`desc[8]`(sel≠0)。`off=desc&0xFFFFF`;`off==0`=不内联。
- BOOL(type5):`(record[off]>>bit)&1`。其余:`record[off]`=分量计数,数据自 `record[off+1]`;实型(2/6)每分量 2 字、**低字在前**(double = `BE(hi=w[k+1])‖BE(lo=w[k])`);整/引用每分量 1 字。

### 8.4 闭环验证(纯离线,无运行时)
- WELD(0x97247)@ desvir tlu idx30,skelK 页580/691字,66 描述符;POS(0x853B1)`type6 size3 off=11 alt=11`。
- sam7200_0001 WELD 记录 `w10=0x20098000→sel=1`,`record[11]=3`,自 w12 低字在前 → **POS=(9630.0,8072.0,5282.5)** ✓(与 §11.6/§11.7 逐字吻合);ORI=(0,90,0)。
- 复现:`docs/e3d 数据库分析/desvir_typedef_probe.py`(`Schema.typedef(noun)` + `decode_attr(record, desc, hash)`)。
- 校正:`attlib.dat`=属性库(名/类型/单位/类别 + noun→属性列表);`*vir.dat`=类型定义库(每属性存储 offset)。二者不同文件 ⇒ §5b 纯 attlib 找不到 offset 的根因。

### 8.5 完整命名属性解码器 + 双元素交叉验证(2026-06-06)
- 新增 `docs/e3d 数据库分析/e3d_attr_decoder.py`:`SchemaSet(exe_dir)` 加载全部 `*vir.dat`(实测 20 库 / 1478 noun),`decode_element(ss, record_words)` 输出元素**全部内联命名属性**(`db1_dehash` 名 + 类型 + offset + 值)。
- type-def `desc[2]` 的 `type` 枚举(与 §7.1 attlib TYPE **不同**,实测):`2/6`=Real、`3/7`=Int、`4/8/16`=引用(2字)、`5`=Bool(位)、`14/15/18/19`=文本/计数前缀、`10/11/12`=数组(多 off=0)。**标量(size==1 非文本)直接存 `record[off]` 无计数字**;`size>1`/文本才"计数@off + 数据@off+1"(对应 `db4_get_ce_att` 的 `v68` 分支)。
- 双元素交叉验证(均 desvir.dat,纯离线):
  - `sam7200_0001` WELD:POS=(9630.0, 8072.0, 5282.5),ORI=(0,90,0)。
  - `ele_data_0` WELD:**POS=(9630.0, 8224.0, 5130.5)**(与 §11.6/findings §4 独立实测**逐字吻合**),ORI=(−180,0,90)。
  - 两元素解出一致 WELD 属性集(POS/ORI/BUIL/SHOP/ORIL/POSI/LOFF/SPRE/LSTU/ARRI/LEAV/ISPE/TSPE/ANGL/HEIG/ALLO/WLDN…),布尔位/引用对/整数/实数全部正确。
- 待精化(非阻塞):type 4/8/16 引用对的 dbno/refno 精确语义;type 14 文本(如 SPAMAP)的字符解码;main(sel=0)路径用 float 的样本(目前样本均 sel=1)。

### 8.6 接入 reader 并跨 16 种元素类型泛化(2026-06-06)
- 把解码器接进 `e3d_db_reader_v2.py`:新增 `--attrs [--exe <dir>]`,沿 B 树枚举元素并解出命名属性(import `e3d_attr_decoder`)。
- **记录有效性过滤**(关键):某些索引叶项指向的不是主元素记录(而是引用/成员结构,`word0` 为 refno 片段如 `0x????5C20` ⇒ 误得 impl=23584)。过滤条件:`(word0>>16)==0 且 8≤(word0&0xFFFF)≤512`。加此过滤后无垃圾解码。
- **跨类型泛化(sam7200_0001,纯离线,均 desvir.dat / sel=1)**:一次解出 16 种 noun 的合理值,例:
  - `NBOX` XLEN/YLEN/ZLEN=(494,68,12);`CTOR` RINS/ROUT/ANGL=(20,36,180);`NCYL` DIAM/HEIG=(510,53);`CYLI` DIAM/HEIG=(56,4);`DPSP` DDIR=(0,0,−1) RADI=510;`PANE/NREV/VERT/PAVE/PLOO/NCYL/CTOR…` 的 POS/ORI 均为合理几何量。
- **现状**:本库(设计库)主记录**全部 sel=1(unpacked/double)**,已全面验证。sel=0(packed/float,主 offset desc[5])路径在本库无干净样本(出现 sel=0 的都是被过滤的误定位项),其精确布局留待 catalogue 类库取证。

### 8.7 schema 三个 skeleton 的角色 + 默认值机制(`db4_get_ce_att_default` 0x1064E630)
- `db4_get_ce_att`(offset==0 时)旁路到 `db4_get_ce_da_list`(0x1060FA80,二级表)与 `db4_get_ce_att_default`(0x1064E630,"4.10.2",取默认值)。后者用 `db2_get_element_details` 的 **mode 1/2** 读 schema 的 I/J skeleton。
- **三个 skeleton 角色实测**(WELD entry=`[noun,Kpg,Kcnt=691,Ipg,Icnt=35,Jpg,Jcnt=26]`):
  - **K**(mode0)= type-def(66 描述符,存储布局)。
  - **I**(mode2)= 默认"记录镜像",按 offset 定位:`default = I[offset−11]`;WELD POS off=11 → `I[0]=3`(默认 count)。
  - **J**(mode1)= 默认/覆盖**哈希表**:`[hash, sizeword, value…]`,`type=sizeword>>26`、`count=sizeword&0x3FFFFFF`、步长 `count+2`。实测 WELD J:`0xBC6C0(t5)=1`、`0x6A02604(t3)=0x367ECC`、`0x1071D120(t2,n2)` 等,与 db4 解析逻辑逐位吻合。
- **NAME 等纯 pseudo**:不在记录、也不在 K/I/J(offset=0),由上层名字服务(name table/PseudoAttPlugger)解析,属记录/模式库格式**之外**;OWNER 则在记录头 word4–5。
- ⇒ 模式库 `*vir.dat` 的**每类型数据(K/I/J 三 skeleton)已全部解释**;元素记录 + 模式库的离线属性解码格式分析**完成**。

### 8.8 元素 NAME / 显式属性区(数据侧定位,纯离线可解)
- 数据侧扫描确认:元素名(`/HS-ADMIN/...` 等)与 DESC 以 **ASCII 存于 type-7 页**(非单独 name table)。
- **显式属性区格式**(隐式区之后):条目 `[hash][ctrl][value...]`,`type=ctrl>>26`、`wordcount=ctrl&0x3FFFFFF`、步进 `wordcount+2`(与 J skeleton 同构)。文本(type 10/14/15)= `[length][packed 4 字符/字,高字节在前]`。
  - **NAME** `hash=0x9C18E type=15`;DESC/FUNC 一般文本 `type=10`;标量如 PURP `type=3`。
  - 实测 `/GRID-STABILIZER`:NAME=`/GRID-STABILIZER`、DESC=`Grid for STABILIZER`、FUNC=`SYSTEM`、PURP=617227;`/HS-ADMIN/ADMIN/ANCHOR-LINE`(27 字符,wc=8)。
- **离线全量抽取**:`extract_names(buf)` 扫 `[0x9C18E][type15][len][chars]`,对 `sam7200_0001` 得 **1254 个元素名**。工具新增 `decode_explicit_attrs`/`extract_names`(`e3d_attr_decoder.py`)。
- 修正认知:NAME 在 type-def 里 `offset=0`,但**物理存在元素显式区**,可纯离线解析(并非纯运行时名字服务)。填补了"显式/成员区 TODO"。

### 8.9 完整 record framing(`db4_get_list` 0x1060CE20)+ 全元素离线解码(闭环)
- 反编译 `db4_get_ce_da_list`(0x1060FA80)→ `db4_get_list`(0x1060CE20,"4.2.1"),拿到记录头三段定位:
  - `rec[6]`=page_no;`rec[7]`→DA 显式区页内字偏移 `(rec[7]>>13)&0xFFF`;`rec[8..9]`→成员定位;`rec[10]`:`bit29`=sel、`(>>14)&0x3FFF`=DA 字数、`&0x3FFF`=成员字数。
  - DA/成员是**链式节点**:5 字头 `word0=(u16)(payload+5)|(type<<16)`(type 1=DA/2=members)、`word1..2`=refno、`word3..4`=下一节点链接;payload 自 +5 字起,为 §7.8 的 `[hash][ctrl][value]` 条目。NAME=DA 里的 `0x9C18E`。
- **闭环验证**(sam7200 WELD@page807 word262):`rec[6]=807,rec[7]=0x268001→da_off=308(=262+46 紧接隐式区),rec[10]=0x20098000→sel1/DA38/mem0`;节点头 `0x0001002B`(payload38,type1)。
  - 解出:`noun=WELD name="/WB1" refno=(23584,5656) owner=(23584,5653)`;隐式 POS/ORI/bools/refs…;DA: ISOH/RLOC/HREL/DELDSG/AEXCES/LEXCES/LOOS/WELDTY/TYPEX/NAME="/WB1"/PTNB。
- 工具:`e3d_attr_decoder.py` 新增 `decode_da_list`、`decode_full_element(ss, buf, record_off)`(无 lint 错误)。
- ⇒ **给定记录偏移即可纯离线解出 元素 noun/NAME/refno/owner + 全部隐式属性 + 全部 DA/显式属性**。元素记录格式分析**全部闭环**。剩余仅:DA payload 跨页链式(node word3/4)、catalogue 库 sel=0(float)取证 —— 均非阻塞。

### 8.10 整库离线导出(端到端 + 规模化验证)
- `e3d_export.py`:头部→会话链→B 树枚举 refno→`decode_full_element`→JSON。对 `sam7200_0001` 实测:**6536 元素 / 1128 命名 / 140 noun 类型**,5.1 MB JSON。
- top nouns:PAVE747/BOX580/CYLI543/VERT378/SUBS271/DISH254/SJOI218/SCTN207/SNOD191/NCYL181/RTOR178/POIN176/ELBO122…(均真实 PDMS 设计类型)。
- 真实工厂数据正确还原:`EQUI /P1501A POS=(9340,12145,645)` + 管嘴 `NOZZ /P1501A-N1//P1501A-N2`(owner 指回设备 23584/5364),`/E1302B-S1` 等。⇒ 离线解析**端到端打通并规模化验证**。
- 工具集(docs/e3d 数据库分析/):`desvir_typedef_probe.py`(模式库 typedef)、`e3d_attr_decoder.py`(Schema/decode_element/decode_full_element/extract_names)、`e3d_db_reader_v2.py --attrs`(遍历解码)、`e3d_export.py`(整库 JSON 导出)。

### 8.11 owner 链层级树重建(owner refno 语义验证)
- 每元素记录头 `word4-5 = owner refno`。按 owner→child 连边即重建 PDMS 模型树。`e3d_tree.py` 从导出 JSON 重建,实测 sam7200:6536 元素 / 359 根。
- **语义全部正确**:最大子树根均为 **ZONE**(`/EQUIPRACK-ACCESS` 子树 1414、`/STEEL` 754、`/PIPES` 502、`/CIVIL` 258…);层级 `ZONE → STRU/FRMW → SCTN/SNOD/SUBS → BOX/CYLI/DISH…`;`EQUI /P1501A → CYLI/BOX/NCYL 几何 + NOZZ /P1501A-N1//N2`(泵+管嘴,与 PDMS 建模一致)。
- ⇒ owner refno 语义确认;**层级 + 名称 + 属性的完整结构化模型可纯离线重建**。工具新增 `e3d_tree.py`(无 lint)。

### 8.12 引用属性(type 4/8/16)= `(dbno, refseq)` + 管道连通性(实测)
- 引用值 = **2 字 `(dbno, refseq)`**(与记录头 refno/owner 同构,首字=dbno)。sam7200 实测 3345 个非零引用对,首字(dbno)直方图:`23584`(本设计库,791)、`15192/15193/15194/15195/15200/15213…`(catalogue/spec 库,外部文件)。
- **776 个本库引用解析到元素名**,恢复真实**管道连通性**:`CREF`(connection)→管道分支(`/150-B-6-B1`/`/100-B-8-B2`)、`HREF`(head)→设备管嘴(`/P1502A-N2`/`/P1502B-N2`)、`TREF`(tail)→三通(`/100-B-1-B1-TEE1`)。即管道 head/tail 引用指向管嘴/三通——PDMS 连通模型。
- 语义:**连接类引用(CREF/HREF/TREF)指向本设计库(可离线解析)**;**规格/材料类(SPRE/MATR/ISPE…)指向 catalogue 库(外部 dbno 15192+,需对应 .dbf 文件才能解名)**。
- 跨库解析需 catalogue db 文件;`test-file/` 下其实有:`acp7002`(dabacon dbno 15194,catalogue)、`ams1112`(17496)、`amssys`(24575)、`acp7002`/`sam7200` 等(注:文件头 dbnum 字段=7002/7200,与 refno 用的 dabacon dbno 15194/23584 不同,二者按项目 db 列表映射)。

### 8.13 catalogue 库解码 + 跨库引用解析(闭环)
- `acp7002_0001` = **catalogue 库**(dabacon dbno 15194):39 noun 类型(SCOM/SCYL/SBOX/SPCO/CATE/SECT/PTMI… 目录组件),10124 命名元素,经 `catvir.dat` schema 正常解码(如 `SCYL /FAANGBRCO-C1INSUBREXT`)。
- **跨库引用解析(sam7200 → acp7002)**:sam7200 中 `dbno=15194` 的 241 个引用全部解析到 catalogue 名:
  - `.PSPE → SPEC /AVEVAHVACSPEC`(管道规格)、`.ISPE → SPEC /AVEVAHVACISPEC`(绝热规格)
  - `.SPRE → SPCO /AVEVAHVACSPEC/STDAHU`(空气处理单元)/`/RVCD`(风阀)/`/RSBEND`(弯头);`.LSTU → SPCO …/RTUBEA`(风管)
  - 即设计元素(HVAC 风管 /HVAC2-B1/…)→ catalogue 规格组件,PDMS 设计↔目录链路完整还原。
- **sel=0(packed/float)在设计库与 catalogue 库均未出现(全 sel=1/unpacked double)** —— 该路径已实现但实务未用。
- ⇒ 设计库 + catalogue 库 + 跨库引用,全部纯离线解码闭环。
- **工具化**:`e3d_export.py --cat <db>` 加载 catalogue 库构全局 refmap,导出每元素 `refs` 字段(引用→目标名,本库+跨库;未加载库用 `=db/ref`)。实测 sam7200 + acp7002:refmap=29672,1897 元素带已解析引用(如 `NOZZ /E1302B-S1 → CREF:/150-B-6-B1`)。

### 8.14 Rust 移植(独立 crate,已编译验证)
- `pdms_io` 整 crate 缺 `dpc-sync` 无法构建;改做独立 std-only crate `tools/e3d_decode_rs/`(空 `[workspace]` 隔离父 crate)。
- `cargo run`(rustc 1.98 nightly)编译+运行成功,与 Python **逐字一致**:desvir.dat 745 类型、WELD 66 描述符、`name="/WB1"`、`POS=[9630.0,8072.0,5282.5]`、`ORI=[0.0,90.0,0.0]`。
- 证明格式/算法(BE 读、模式库 typedef、offset 取值、低字在前 double、NAME 显式区)可干净移植到 Rust;待 dpc-sync 就绪可并入 `pdms_io`。
- **整库遍历**(续):Rust crate 扩成完整读取器(头部→会话→B 树→加载全部 `*vir.dat`→逐元素 noun/NAME/POS)。与 Python 核心一致:20 schemas/1478 类型、**elements=6536 精确一致**、POS 逐字相同(`EQUI /P1501A=[9340,12145,645]`)。次要计数差(named 1122/1144、noun_types 156/140)源于 NAME 探测边界,<2%,非正确性问题。
- **全面对齐(2026-06-06 续18)**:`tools/e3d_decode_rs/` 扩成完整解码器+导出器(移植 `_decode_one` 全类型、**链式 `decode_da_list`**、`_parse_attr_words`、跨库 refmap、std-only JSON)。链式 DA 遍历**修复 NAME 边界** ⇒ `named=1144、noun_types=140` 与 Python **精确一致**;`refmap=29672` 一致;`WELD /WB1 POS=[9630,8072,5282.5]`、`EQUI /P1501A=[9340,12145,645]`;`--cat acp7002` 跨库引用 `NOZZ /E1302B-S1→CREF:/150-B-6-B1` 一致;JSON 经 `json.load` 校验。无告警;`cargo clean` 已清理。⇒ Rust 端与 Python 工具链**全面对齐**,待 `dpc-sync` 并入 `pdms_io`。

## 9. db4_get_ce_att 全函数反编译 + type 枚举全库实测 + UDA 定位(2026-06-06,IDA 稳定)
> IDA 重连稳定(core.dll @127.0.0.1:13337, idb 2.10, hexrays_ready)。把"type 14/15/18/19 + sel=0 packed + UDA"三处历史待办用**权威反编译 + 全库实测**收口。

### 9.1 `db4_get_ce_att`(0x10612A50)整函数逻辑(逐行)
- **offset/sel**:`record=CE.data+4*CE.wordidx`;`sel=(record[10]>>29)&1`;`v14=sel?3:0`;`off=desc[v14+5]&0xFFFFF`;`bit=desc[v14+5]>>20`;`off>=record[0]&0xFFFF` 报 623;`off==0` 转 DA/默认路径。
- **`v68`(标量 vs 计数前缀)**:`v68=1 ⟺ size==0||size>1||type∈{14,17,19}`;否则 `v68=0`(size==1 且非文本)= **值直接在 `record[off]`,无计数字**。
- **取值 switch**:
  - type 5 Bool:`(record[off]>>bit)&1`,count=1。
  - type 2/6 Real:`sel=1` 每分量 2 字 double(低字在前);`sel=0` 每分量 **1 字 IEEE float**(`*(double*)a4=*(float*)&record[k]`)。
  - type 14:字数 `=count*((desc[4]-1)/size)`;type 18:`=ctrl字数-1`;type 19:`=DA[off+2]`;type 15/其它:`=round(count/scale)`(scale>1 进位)。
- **定宽表 `dbl_10F68E90[type]`**:把分量数换字数(`words=round(count/scale)`,标量 `round(1/scale)`)。**运行时初始化**(静态 IDB 全 0xFF;36 xref 皆读)。由代码+样本反推:**Real(2/6)=0.5、Int(3/7)=1.0、Ref(4/8/16)=0.5、Text(10/15)=4.0**。
- 错误码:17 实际类型不符、18 当前元素类型无此属性、23 缓冲不足、534 表名类(req 14/15/18 走默认路径)、623 offset 越界。

### 9.2 描述符 desc[0..9](`db4_get_att_dets` 0x10611FF0 逐字段)
- `[0]hash [1]stride [2]type [3]size [4]text_cap [5]主off|bit [7]aux [8]备off|bit [9]flag`(typedef:`+0x24`=描述符数、`+0x38`=数组起)。
- **UDA 运行时改写**:命中后用全局 UDA 表 `dword_115994E4`(数 `dword_115994F4`,16B/项)改写 `desc[3]/[4]/[7]`,据 `dword_115994DC` 名单设 `desc[9]=1`。表为运行时全局(0xFF 静态)。

### 9.3 type 枚举全库实测(`type_enum_probe.py`,20 schemas / 1478 noun)
- 计数:type3=4806、14=3482、10=2838、7=2631、2=2052、5=1386、8=1243、18=1227、6=1217、4=1123、16=730、17=103、9=18(19=0)。
- 语义(size 区分标量/数组):**2**=Real标量(sz1)/**6**=Real向量(sz3 POS/ORI…);**3**=Int标量/**7**=Int数组;**4**=Ref标量/**8**=Ref数组/**16**=word-Ref标量(SPRE/ISPE/CELREF/PTRE);**5**=Bool;**10**=文本(DESC/FUNC,sz480,off=0);**15**=NAME 文本(全 off=0);**14**=UDA表(UDATAB/UDAFTB)+ SPAMAP(可内联);**18**=UDA字符串表(UDASTB,全 off=0);**9/17**=方向/MDSYSF 特殊;**19** 本批未用。
- 验证(WELD 双元素):real 标量 ANGL/HEIG/ALLO 各 2 字(off 36/38/40);ref 标量 SPRE/LSTU/ISPE/TSPE 各 2 字 `(dbno,refseq)`;int 标量 ARRI/LEAV/WLDN 各 1 字;**SPAMAP(type14,off=42,内联)** 确认 type-14 可内联。

### 9.4 UDA 存储(历史 §7.3 [UDA块] TODO 收口)
- UDA 值落于元素**特殊表属性** `UDATAB`/`UDAFTB`(type 14)、`UDASTB`(type 18)(schema 中 size=1000、off=0,随显式/DA 区存储),**可纯离线提取原始字**。
- UDA 名(`:`前缀,`db1_hash>0x171FAD39` 走 base-64)↔字段的运行时映射在全局 UDA 注册表(运行时填充),纯离线不可还原映射;需活进程。
- 工具:新增 `docs/e3d 数据库分析/type_enum_probe.py`(只读,全库 type 枚举)。文档:格式规范 §7.6.1/§7.6.2/§7.6.3/§7.7.7/§8 已更新。
- **结论**:历史三处"待精化"(type 14/15/18/19、sel=0 packed、UDA)均收口 —— type 枚举与取值规则全部权威化;packed 路径代码确证;UDA 容器离线可解。

## 10. UDA(用户自定义属性)元素存储 —— 深入(2026-06-06)
> 接 §9.4。把 UDA 的"每元素取值"从"容器已定位"推进到"强类型值纯离线可解"。

### 10.1 判定与读取(IDA)
- `PDMS_Hash::IsUDA`(0x10001bc0)= `hash > 0x171FAD39`(与 `db1_hash` UDA 阈值一致)。
- `db4_get_ce_att` 对 UDA hash 走 `off==0` → `db4_get_ce_da_list` 扫 DA 链表按 hash 命中 → UDA 值是**显式/DA 区一条普通条目**(`[hash][ctrl:type<<26|wc][words]`),带**声明类型**。
- 名/类型/单位:UDA **字典库** `udalib`(`LXANAM`/`LXALEN`/`LXUNIT`/`LXDEF`、`DB_Uda`);`exppdms/EXRTPD`(0x10080F62)中 `hash>0x171FAD39 → LXANAM`,失败打印 `"unknown UDA"`。字典库=独立 db(类比 catalogue)⇒ 离线得 hash+值,**名需字典**。

### 10.2 强类型 UDA 值纯离线解出(sam7200 实测)
- ctrl 类型分布(765 条):`{7:655, 10:48, 4:47, 6:14, 2:1}`。
- 各类型实测值:`type4 ref=(15195,2418)`、`type6 real=(192.0,192.0)`、`type10 text='D'`、`type2 real(2字 double 低字在前)` —— 均用 §7.8 同规则直接解出。
- **两族 hash**:`0x2C00xxxx`(≈738M)= 普通强类型 UDA(real/ref/text,值可解);`0xFFF?xxxx`(≈4.29G,如 `0xFFF7AC4F=:UDA_0xD7FF16`)= **type-7 结构化 int 块** `[len][0][…][1601][1701]`(`897510`=单位、`1601/1701`=OF/WRT 限定符,EXRTPD 语法)= 派生/表达式型 UDA,令牌语义待解。

### 10.3 规模 + 工具
- `sam7200_0001`:6536 元素 / **453 带 UDA / 765 条**。新增只读 `uda_probe.py`(B 树枚举 → 每元素 UDA hash/类型/原始值 + 顶层结构 + 直方图)。
- 文档:格式规范新增 **§7.10**(UDA 存储)+ 更新 §8 row8;`type_enum_probe.py`/`uda_probe.py` 两个只读探针入库。
- **剩余**:UDA 名(需字典库 db);`0xFFF` 族表达式 UDA 的 EXRTPD 令牌语法完整解析。二者均非阻塞,且与 catalogue 跨库依赖同性质。

### 10.4 UDA 名能否离线还原 —— 定论:不能(2026-06-06 反编译)
- `DEHASH`(0x1065B930)UDA 分支 = 确定但**有损**的 base-64 短码(纯算法,不查库):`v=(hash−0x171FAD39)%0x1000000`;`:`+ 最多 4 字符,每字符 `chr((v%64)+32)`,`v//=64`。实测 `0xFFF7AC4F→":6\_U"`、`0x2C00D55A→":A@2X"`、`0x2C00D55B→":B@2X"`(同族首字符递增、共享 `@2X`)。短码含 `@\]^)` 非标识符字符 ⇒ **非真名**。
- 真名路径 `LXANAM→ATATXT(0x10467D70)→DB_Attribute::findAttribute(hash)`:在**属性注册表**(UDA 由字典库装入)按 hash 取 DB_Attribute 读名字段(`getField`),**不经 DEHASH**。⇒ UDA 真名只在字典/注册表,元素 db 内不含。
- **离线可得**:hash、有损短码(新增 `e3d_attr_decoder.dehash_uda_code`)、强类型值;**不可得**:真名/声明类型/单位(需字典库 db,catalogue 同性质)。
- 工具:`db1_dehash` 加注释;新增 `dehash_uda_code(hash)`。`db1_hash` 基线校验:NAME=0x9C18E/POS=0x853B1/WELD=0x97247/PIPE=0x9CAF3 base-27 dehash 全对。

## 11. 写侧:页无校验和 + 安全在位定长值写(2026-06-06,IDA + round-trip 实测)
- **页完整性**:`db1_read_page`(0x10630C20)= 仅 `FHDBRN`(块读+大端转换+锁重试),**无 checksum 校验**;`db1_write_page`(0x10633FB0)= 缓冲/COW 刷原始页缓冲,**无 checksum 计算**。⇒ 保持大端 +(实型)低字在前,**就地覆盖值字节**即字节合法、可读回(同 PDMS 读路径)。
- **安全在位写**(定长内联值 type 2/6/3/7/4/8/16,组件数不变,不动框架):实型 sel=1 每分量 2 字 double 低字在前;整=1 字;引用=2 字。新增 `e3d_write.py`(`set_inline_value`)。
- **实测闭环**(对 sam7200 **副本**):WELD `/WB1` POS `(9630,8072,5282.5)→(1000.25,−2000.5,3000.75)`,读回新值、NAME 不变,字节 diff 全落该值 24 字节区间内。**PASS**。
- **边界**:文本/变长/DA/UDA/组件数变化需重排+COW(拒绝);未建 PDMS 会话(直接改最新数据,值可读回但非受跟踪变更)。完整写需复刻 `db5_save_work`(COW+会话+刷脏页+page0 重指向,《解析指导》§12–14)。文档:格式规范新增 §12。

## 12. 完整写入提交机制 `db5_save_work`(2.10 权威反编译,0x105E9C80 "5.4.4")
> 仓库 `数据库写入架构.md` 是 E3D 3.1(0x5Axxxxx);此为当前 2.10 idb 权威版。
- 提交 = COW + 追加新会话 + 重指向 page0:① claim 锁 db ② 读 page0→当前会话 pgid(page0[w10/w11])+ 当前 sesno(db-block 属性 id 1)③ 读会话页须 type==3(否则 664),`session[w3]+1==新 sesno`(否则 665)⇒ **新 sesno=旧+1** ④ 分配新会话页(end+1),写元数据(`sub_105E6A20`≈db2_modify_header_page)⑤ 重映射 db-block 属性:**索引根 pgid=属性 13387743(=0xCC47DF)**、基准=属性 7618377 ⑥ 刷脏/COW 页(`sub_10636810`→db1_write_page→FHDBWN)⑦ 重指向 page0 sesno(`sub_10623360(db,1,新+1)`)+ 解锁。
- ⇒ E3D 编辑=**多版本追加**:旧页保留、改动写 COW 新页、追加 sesno+1 会话(其 index_root 指新 B 树根)、原子重指 page0。与读侧 §4(会话链)/§5(每会话独立索引根)自洽。
- **离线完整写计划(后续大步,高风险)**:页分配器 + COW + B 树写侧(插入/分裂,《解析指导》§13)+ 新会话页 + 重写 page0;以"写后本读取器 + 真 E3D 双读"验证。建议独立里程碑。格式规范新增 §12.4。

### 12.1 B 树写侧(2.10 逐函数复核,格式规范 §12.5)
- 索引/表页(type5)写布局:页头 7 字;`word6`=剩余空闲字数;条目自 word7(byte28)升序;`dword_10F68F4C`=页字容量。
- `db3_insert_page_entry`(0x1061B5C0,"3.2.4"):二分定位(`sub_1061B1B0`)→ 空间够则右移腾位+写 key/data+`word6-=占用`;不够置 `*out_split=1`;重复 key 报 529。
- `db3_split_node`(0x1061BA50,"3.2.6"):`db1_get_new_page` 分配新兄弟页 → 分裂点 `(容量−word6−7)/2+7`,搬上半条目、修两页 `word6`、向父递归(最深 50,否则 533);校验 type5/表名/层级(659/660/661)。
- ⇒ 在位定长写(§12.1–12.3)不改 key/不动框架,故不触碰 B 树(安全性来源)。**写侧机制(页完整性+COW+刷页+B树插入分裂+会话提交/page0重指)全部权威分析完成**;实现为独立高风险里程碑。
