# Progress Log

## 2026-06-05 — 规划会话(planning-with-files)
- 按 `planning-with-files` 技能创建隔离计划 `.planning/2026-06-05-e3d-db-offline-attr-parser/`,并设为 `.active_plan`。
- 恢复上下文:pdms-io 无既有计划文件;基于本轮(同会话)对 E3D 2.10 core.dll 的逆向成果撰写开发方案。
- 产出:
  - `task_plan.md` — 5 阶段开发方案(基线纠错 → attlib 加载器 → internalGetField 查表 → 闭环验证 → 集成/测试/文档),含目标、完成判据、风险与回退。
  - `findings.md` — 已验证的逆向事实(纠错、磁盘格式、attlib 机制、实测坐标、风险、地址速查)。
  - `progress.md` — 本日志。
- 关联交付(本轮已完成,作为方案输入):`docs/e3d 数据库分析/` 下格式规范 / 解析指导(读/写/索引/维护/attlib机制)/ 索引 / `noun_hash_table_base27.json` / `e3d_db_reader_v2.py`(sam7200+acp7002 双验证)/ 3 图。

### 状态
- 方案状态:proposed(待用户评审/批准后执行)。
- 下一步(执行起点):Phase 1 基线纠错(低风险),先改 `defines::detect_page_size` ×4 + 切 base-27 哈希 + 加单测。

### 待办/阻塞
- 等待是否批准执行;Phase 3 链式查找为已知风险点(已备运行时旁路回退)。

## 2026-06-05 — 执行 Phase 1(基线纠错落地)
- 改 `src/defines.rs::detect_page_size` → `words * 4`(saturating)+ 512/2048/4096 兜底;同步把单测改为 words×4 语义(128→512/512→2048/1024→4096/0,1234→2048)。
- 排查调用点:生产 `io.rs` 用 `detect_page_size_by_probe`(探测页类型)而非该辅助函数 ⇒ 生产主路径本就正确;`rs-core db1_hash` 本就是 base-27。范围修正记入 findings §1b。
- 验证:`cargo test` 因缺本地依赖 `dpc-sync`(`D:\work\plant\dpc-sync` 不存在)无法构建整 crate;改用 `rustc` 独立编译函数体 → **5/5 用例通过**。
- Phase 1 状态:complete(代码+逻辑验证;整 crate 测试受环境阻塞)。

### 测试结果
- detect_page_size(standalone rustc):OK 5/5(128→512,512→2048,1024→4096,0→2048,1234→2048)。
- 整 crate cargo test:阻塞(缺 dpc-sync 依赖,环境问题,非代码问题)。

### 下一步 / 阻塞(2026-06-05 更新)
- **环境硬约束**:`pdms_io` 缺 `dpc-sync` 无法构建;`rs-core`(aios_core)依赖庞大 git 依赖树(surrealdb 等,edition 2024),本环境同样难以可靠构建。⇒ **本环境无法可靠 cargo build/test 这些 crate**。
- **策略转向**:把 Phase 2–4(attlib 加载器 + internalGetField 查表 + 闭环验证)先做成 **Python 原型**(可在本环境运行 + 用 attlib.dat/样本验证),与 Rust 构建解耦;**Phase 5(Rust 移植 + cargo 测试)推迟到可构建环境**(需 dpc-sync 或精简依赖)。
- 风险仍在:attlib 跨页链式查找 + 交错 noun 表索引是已知难点(findings §5),Python 原型也可能无法一次闭环;若 3 次仍不成,回退到"用 IDA 运行时旁路(db_get_attribute_list)产 ground truth 对照"。

## 2026-06-05 — Phase 2 Python 探针收敛(ATTOPE/ATNAIN 表角色纠错)
- 使用 `test-file/attlib.dat` 跑只读 Python 探针;首次命令误用 Bash heredoc,PowerShell 报 `Missing file specification after redirection operator`,已改用 PowerShell here-string 管道给 Python。
- 用静态 IDA MCP 复核:
  - `ATTOPE/sub_10851210`:目录 `v47[2] -> ATGTIX(attribute index)`, `v47[4] -> ATGTDF(DB_Noun fields)`, `v47[6] -> ATGTIX(noun index)`。
  - `ATGTIX/sub_10852A64`:每条 `[hash, combined]`, `record=combined/512`, `disp=combined%512`。
  - `ATGTDF/sub_10852E20`:每条 `[hash, value, kind, ...]`,负责填 `unk_11C12080/dword_11C12210`。
  - `DB_Noun::internalGetField/sub_1084F7C0`:先用 noun ATGTIX 拿 `(record,disp)`,再用 DB_Noun field index 做矩阵列偏移。
- 新增 `docs/e3d 数据库分析/attlib_atnain_probe.py`,可复现:
  - directory=`0x3 0x4 0x693 0x6A8 0x6CD 0x6CE 0x8BC 0x8C2`
  - `attr_index=256`, `noun_fields=88`, `noun_index=256`
  - WELD -> noun idx82, record=2225, disp=1
  - POS -> attribute ATGTIX idx27, record=1129, disp=127, combined=0x8D27F
  - POS **不在** DB_Noun field table 中。
- 结论:Phase 2 已把"noun 哈希数组静态还原"从阻塞推进到可复现:WELD/PIPE 等可由 `v47[6]` ATGTIX 定位。原计划/旧文档中的 `record(attr)+noun_index` 公式与 `POS -> 0x83787` 锚点不可靠。下一步应分析 `DB_Attribute::internalGetField` 或属性元数据侧,而不是继续用 `DB_Noun::internalGetField(POS)`。

### 文档落地
- 按用户要求把 fresh IDA 分析汇总进 `docs/e3d 数据库分析/E3D_DB_文件格式规范.md` §7.4:
  - 增补 `ATTOPE` 目录项与运行时表映射。
  - 增补 `ATGTIX`/`ATGTDF` 记录格式。
  - 增补 Attribute/Noun 两条读取路径与 `ReadData` 字段常量。
  - 修正样本走查中的 attlib 说明:命名属性解析应按 Attribute 侧元数据 + Noun 侧属性列表关联推进。

## 2026-06-05 — Attribute 侧离线字段读取原型
- 按用户"继续下一步"要求,复刻 `DB_Attribute::internalGetField` 的 attribute matrix 首跳。
- IDA 复核 wrapper:
  - `DB_Attribute::internalGetField(bool)` → `AXALOG/sub_108500E1`
  - `ATAINT/sub_1085001C` 与 `AXALOG` 共用核心逻辑;int 标量取 `page[disp + ptr - 2]`
  - string wrapper 经 `sub_10850077`,首 word 为长度,后续 word 低字节为字符
- 更新 `attlib_atnain_probe.py`,现在可输出 POS 元数据:
  - `SIZE=3`, `TYPE=8`, `DEFI=5`, `DTYP=2`, `UNIT=0xE54BF("DIST")`, `NAME="POS"`, `CATEG="Positional"`。
- 已把上述字段读取规则和 POS 示例追加到 `E3D_DB_文件格式规范.md` §7.4。

## 2026-06-05 — Noun 侧显示/属性列表原型
- 复刻 `DB_Noun::internalGetField(DISPLY/PRDISP)` 的 vector 读取路径。
- `attlib_atnain_probe.py` 现在可输出 WELD 的属性 hash 列表:
  - `WELD.DISPLY` 共 13 项,含 `ORI`、`POS`、`ISPE`、`MTOC`、`SPRE`、`TSPE`。
  - `WELD.PRDISP` 共 28 项,含 `POS`、`ORI`、`BUIL`、`SHOP`、`ABOR/ACON/ADIR` 等。
- 已把 WELD 的 Noun 示例追加到 `E3D_DB_文件格式规范.md` §7.4。

## 2026-06-05 — WELD 可解析属性 schema 摘要
- 扩展 `attlib_atnain_probe.py`,对 WELD 的 `DISPLY/PRDISP` 中能在 Attribute ATGTIX 命中的条目批量读取 Attribute 元数据。
- 当前可静态解析:
  - `POS`:TYPE=8,SIZE=3,DEFI=5,UNIT=DIST,CATEG=Positional
  - `ORI`:TYPE=9,SIZE=3,DEFI=5,UNIT=NONE,CATEG=Positional
  - `ISPE/MTOC/SPRE/TSPE` 等规格/出图相关属性
- 已将 WELD schema 摘要表补入 `E3D_DB_文件格式规范.md` §7.4。

## 2026-06-05 — sam7200 WELD.POS 最小闭环
- 在 `pdms-test-data/sam7200_0001` 中通过会话索引根 `3377` 遍历到第一个 WELD:
  - 索引叶键 `refno(0x5C20,0x1618)` → `pgno=807`, `offset=524`, `byte_off=1653784`。
  - 元素头 `implicit_count=46`, `noun=0x97247("WELD")`, `owner=(0x5C20,0x1615)`, `page_no=807`。
  - 隐式区 `w[13..18]` 解为 3 个大端 double:POS=`(9630.0,8072.0,5282.5)`。
- 结合 `attlib_atnain_probe.py` 重新确认:
  - `WELD.DISPLY/PRDISP` 均包含 `POS(0x853B1)`。
  - `POS` Attribute 元数据为 `SIZE=3,TYPE=8,DEFI=5,DTYP=2,UNIT=DIST,NAME="POS",CATEG="Positional"`。
- 已将 `noun→attribute-name→schema→raw-value` 最小闭环补入 `E3D_DB_文件格式规范.md` §11.7。

## 2026-06-05 — 属性 offset 离线推导调查(IDA 不可用,纯数据实验)
- **环境变化**:本轮 IDA Pro MCP **未连接**(可用服务器仅 best-mcp/claude-mem)。叠加既有约束(不能 cargo 构建、IDB 静态),代码侧读 `db4_get_ce_att` 这条路当前不可走。改做纯 attlib.dat 字节实验。
- 新增 5 个只读探针(docs/e3d 数据库分析/):
  - `attlib_offset_probe.py`:测 offset 矩阵假设 A/B。
  - `attlib_segment_probe.py`:分类 8 个目录段。
  - `attlib_deftable_probe.py`:解析 v47[3] 三元组表。
  - `attlib_nounfields_probe.py`:枚举 88 个 noun 字段 + WELD 向量字段。
  - `attlib_hash_locator.py`:全表定位 hash。
- 实验结论(详见 findings §5b):
  - 排除"offset=简单 attr×noun 矩阵单元"(假设 A=CATEG 字符串区,假设 B=哨兵)。
  - noun 字段无完整 DAB 布局(仅 DISPLY/PRDISP 显示列表)。
  - offset/布局数据实际在段 `v47[1]=0x4`(Syntax/定义大表):POS 出现 463 次、ANGL 220 次;含属性定义(POS+DIST+SIZE)与属性序列(POS,ORI,...);但 noun 按**索引**而非 hash 关联(WELD 全表仅 1 次)。
  - offset 本质是 `db4_get_ce_att` 运行时按**变长 DAB 序列累加**得到,非静态单元。
- **状态**:软闭环(noun+schema+按TYPE取值)可达且已成;硬闭环(精确 offset 公式)受阻,需 (a) IDA 活进程 / (b) 可构建 Rust / (c) 深逆 v47[1] Syntax 表。已就下一步方向请用户裁决。

## 2026-06-05(晚)— IDA 恢复,反编译 db4 拿到 offset 权威机制
- 用户告知 IDA 已连接;`list_instances` 确认 core.dll @127.0.0.1:13337(idb 2.10)。改走代码侧权威路线。
- 反编译 `db4_get_ce_att`(0x10612A50)+`db4_get_att_dets`(0x10611FF0),**确证 offset 机制**(详见 findings §7):
  - 描述符在 CE 类型定义表(`*(CE+0x10)`)中线性查找:`[0]=hash,[1]=stride,[2]=type,[3]=size,[5]=offset(低20)+bit(高位)`。
  - 取值 `offset = desc[5]&0xFFFFF`(从记录起始计字索引),BOOL `bit=desc[5]>>20`,与 §7.2 完全一致。
  - `v16` 指记录起始((u16)*v16=记录字数=46)。⇒ WELD.POS offset 必=13,与实测 w13 **逐字吻合 → 机制层硬闭环成立**。
  - 类型等价 2↔6/3↔7/4↔8;实型经 dbl_10F68E90 定点换算。
- 排除消费者:`db4_get_ce_da_list`(0x1060FA80)、`db4_get_ce_table_att`(0x10613690)。
- **IDA 随后掉线**(fetch failed,health/list_instances 均不通)。typedef 的磁盘装载器(DABACON)尚未定位 → 纯离线自动推导 offset 仍差这一步。
- 文档落地:`E3D_DB_文件格式规范.md` 新增 **§7.6**(db4 offset 权威机制 + 描述符结构 + 类型等价 + DABACON 存疑),并更新 §11.7 备注指向 §7.6。
- 新增 IDA 路线探针无(纯反编译);offline 探针仍为 5 个(§5b)。
- **下一步(待 IDA 恢复)**:定位 typedef 构建器(写 CE+0x10 / 写 desc+0x14 offset),复刻 DABACON 累加规则 → 离线产出任意 noun×attr 的 offset。

## 2026-06-06 — IDA 重连,定位 typedef 磁盘来源 → offset **纯离线闭环达成**(Phase 3/4 完成)
- IDA Pro MCP 已稳定(core.dll @127.0.0.1:13337, hexrays_ready)。沿"待 IDA 恢复"方向反编译 typedef 构建/装载链:
  - `db4_set_ce_from_extref`(0x1060F170)= 写 CE+0x10 的"设置当前元素":取元素记录 `word[3]=noun hash`,经 `db2_get_element_details`(0x10624400,"2.2.5")**二分查类型索引**得 typedef(skeleton K),写 `CE+0x10/0x14/0x40`。
  - `db2_open_template_db`(0x10621850,"2.1.1")= 装载器:读"模式库"头部 + 类型索引(tlu)注册到 `dword_11599778`。
  - 上溯:`DB_SchemaMngr::openAllSchemas`(0x10498BE0)→ `DB_DBSchema::openSchema`(0x10497310)→ `db_open_template_db`(0x105DC6F0→0x105F44E0)。
- **关键结论(推翻旧假设)**:type-def(含 offset)**不是运行时 DABACON 累加**,而是**预存于磁盘模式库** `%AVEVA_DESIGN_EXE%/*vir.dat`(`desvir.dat`=DESIGN,大端,2048B/页,`FHFIND "DB,BL 512"`),装载时整块读入。⇒ offset 完全可离线。
- 解析 `desvir.dat` 实测:魔数 6、745 类型、tlu 升序;WELD 在 tlu idx30,skelK=页580/691字,66 描述符;POS `type6 size3 off=11`。
- 解码规则反编译+实测:`sel=(record[w10]>>29)&1` 选主/备 offset;`record[off]`=计数,数据自 `off+1`;实型 2 字/分量、**低字在前**。
- **闭环**:对 `sam7200_0001` WELD 纯离线解出 **POS=(9630.0,8072.0,5282.5)**(与既有结构化实测逐字吻合)、ORI=(0,90,0)。
- 落地:新增 `docs/e3d 数据库分析/desvir_typedef_probe.py`(Schema 解析 + 记录解码,可复现);`E3D_DB_文件格式规范.md` 新增 **§7.7**(模式库格式)、更正 §7.6.2/§7.6.4/§11.7;`findings.md` 新增 **§8** 并标注 §7 更正。

### 状态
- Phase 2(attlib 加载器)/ Phase 3(internalGetField 查表)/ Phase 4(闭环验证):核心难点 offset 来源**已离线打通**(目标判据"离线解出 POS 且与结构化解码一致"达成)。
- 软闭环(noun+schema+按类型取值)与硬闭环(精确 offset)**均已离线成立**。

## 2026-06-06(续)— 完整离线命名属性解码器 + 双元素交叉验证
- 按"推荐下一步"实现 `docs/e3d 数据库分析/e3d_attr_decoder.py`:跨全部 `*vir.dat` 自动按 noun 选库(实测 20 库 / 1478 类型),`decode_element` 输出元素**全部内联命名属性**(`db1_dehash` 名 + 类型 + offset + 值)。
- 关键解码规则补全:**标量(size==1 非文本)直接存 `record[off]`(无计数字)**;`size>1`/文本才计数前缀(对应 `db4_get_ce_att` 的 `v68`)。type-def `desc[2]` 枚举实测:2/6=Real、3/7=Int、4/8/16=引用(2字)、5=Bool、14/15/18/19=文本。
- **双元素交叉验证(纯离线)**:
  - `sam7200_0001` WELD:POS=(9630.0,8072.0,5282.5),ORI=(0,90,0)。
  - `ele_data_0` WELD:**POS=(9630.0,8224.0,5130.5)** —— 与 §11.6/findings §4 **独立实测逐字吻合**;ORI=(−180,0,90)。
  - 两元素 WELD 属性集一致(含 BUIL/SHOP/ORIL/POSI/LOFF 布尔、SPRE/LSTU 引用、ARRI/LEAV 整数、ANGL/HEIG/ALLO 实数)。
- 文档:`E3D_DB_文件格式规范.md` 新增 §7.7.7(type 枚举 + 解码器 + 交叉验证表);`findings.md` 新增 §8.5。

### 状态
- Phase 2/3/4:**离线全链闭环 + 完整属性解码器已成**(单值与全表均验证,双元素交叉一致)。

## 2026-06-06(续2)— 接入 reader + 跨 16 种元素类型泛化
- 把解码器接进 `e3d_db_reader_v2.py`:新增 `--attrs [--exe <dir>]`(import `e3d_attr_decoder`),沿 B 树枚举元素并解命名属性。
- 诊断"sel=0 垃圾"根因:并非主路径解码 bug,而是**部分索引叶项指向引用/成员结构**(`word0`=refno 片段 `0x????5C20` ⇒ impl=23584)。加**记录有效性过滤** `(word0>>16)==0 且 8≤count≤512` 后全部干净。
- **跨类型泛化(sam7200_0001,纯离线)**:一次解出 **16 种 noun**(均 sel=1)合理值:NBOX(494/68/12)、CTOR(20/36/180)、NCYL(510/53)、CYLI(56/4)、DPSP DDIR=(0,0,−1)、PANE/NREV/VERT/PAVE/PLOO/SUBE/TMPL/CTOR/NBOX 的 POS/ORI…。
- 文档:`E3D_DB_文件格式规范.md §7.7.7` 补 reader 接入 + 泛化 + 有效性过滤;`findings.md §8.6`。
- 现状:设计库主记录**全部 sel=1**,已全面验证;sel=0(packed/float)本库无干净样本,留待 catalogue 库取证。

## 2026-06-06(续3)— schema 三 skeleton(K/I/J)角色 + 默认值机制
- 反编译 `db4_get_ce_att_default`(0x1064E630,"4.10.2"):元素未存某属性时,默认值来自 schema 的 I/J skeleton(`db2_get_element_details` mode 1/2)。
- 实测确认 WELD 三 skeleton:**K**=type-def(布局,691字/66描述符);**I**=默认记录镜像(`default=I[offset−11]`,POS off=11→I[0]=3);**J**=默认哈希表(`[hash,sizeword,value]`,`type=sizeword>>26`、`count=&0x3FFFFFF`、步长 count+2;实测 0xBC6C0(t5)=1、0x6A02604(t3)=0x367ECC… 与 db4 逻辑逐位吻合)。
- NAME 等纯 pseudo(offset=0)不在记录/K/I/J,由上层名字服务解析,属格式之外;OWNER 在记录头 word4–5。
- 文档:`E3D_DB_文件格式规范.md §7.7.8`、`findings.md §8.7`。
- **结论:模式库 `*vir.dat` 每类型数据(K/I/J)已全部解释;元素 + 模式库的离线属性解码格式分析完成。**

## 2026-06-06(续4)— 元素 NAME / 显式属性区(纯离线解明)
- 数据侧扫描:元素名以 ASCII 存于 type-7 页(非独立 name table)。摸清**显式属性区格式**:`[hash][ctrl: type<<26|wordcount][value]`,步进 `wordcount+2`(同 J skeleton)。文本(type 10/14/15)=`[length][packed 4 字符/字,高字节在前]`。NAME `hash=0x9C18E type=15`。
- 实测:`/GRID-STABILIZER`(NAME/DESC=`Grid for STABILIZER`/FUNC=`SYSTEM`/PURP=617227)、`/HS-ADMIN/ADMIN/ANCHOR-LINE`。`extract_names(sam7200_0001)` 抽出 **1254 个元素名**。
- 工具:`e3d_attr_decoder.py` 新增 `decode_explicit_attrs`、`extract_names`(无 lint 错误)。文档:格式规范 **§7.8**、`findings.md §8.8`,并更正 §7.7.8"NAME 在格式之外"的旧表述。
- 认知修正:NAME 在 type-def 里 `offset=0`,但**物理存于元素显式区**,纯离线可解(非纯运行时名字服务)。填补"显式/成员区 TODO"。

## 2026-06-06(续5)— 完整 record framing + 全元素离线解码(闭环)
- 反编译 `db4_get_ce_da_list`(0x1060FA80)→ `db4_get_list`(0x1060CE20,"4.2.1"),解明记录头三段定位:`rec[6]`=page_no、`rec[7]`→DA 偏移、`rec[8..9]`→成员、`rec[10]`=sel(bit29)+DA字数(>>14&0x3FFF)+成员字数(&0x3FFF)。DA/成员=链式节点(5字头,(u16)=payload+5,type 在 bit16-19;payload 自 +5 字起为 `[hash][ctrl][value]` 条目)。NAME=DA 里 0x9C18E。
- **闭环**:sam7200 WELD@page807 word262 → `name="/WB1"`、refno=(23584,5656)、owner=(23584,5653)、隐式 POS=(9630,8072,5282.5)/ORI/bools/refs、DA: ISOH/RLOC/HREL/DELDSG/.../NAME=/WB1/PTNB。
- 工具:`e3d_attr_decoder.py` 新增 `decode_da_list`、`decode_full_element`(无 lint)。文档:格式规范 §7.9、findings §8.9。
- **结论:元素记录格式(隐式 typedef + 模式库 K/I/J + DA/显式 + NAME + 成员定位)全部闭环;给定记录偏移即可纯离线解出整元素全部属性。**

## 2026-06-06(续6)— 整库离线导出(端到端 + 规模化)
- 新增 `e3d_export.py`:头部→会话链→B 树枚举 refno→`decode_full_element`→JSON。
- 对 `sam7200_0001` 实测:**6536 元素 / 1128 命名 / 140 noun 类型**(5.1 MB JSON,7s)。top nouns PAVE/BOX/CYLI/VERT/SUBS/DISH/SJOI/SCTN…(真实 PDMS 类型)。
- 真实工厂数据正确:`EQUI /P1501A POS=(9340,12145,645)` + 管嘴 `NOZZ /P1501A-N1//P1501A-N2`(owner 指回设备),`/E1302B-S1` 等。
- 文档:格式规范 §7.9.1、findings §8.10。无 lint。
- **里程碑:E3D 元素数据离线解析端到端打通并规模化验证。** 工具集:`desvir_typedef_probe.py` / `e3d_attr_decoder.py` / `e3d_db_reader_v2.py --attrs` / `e3d_export.py`。

## 2026-06-06(续7)— owner 链层级树重建(语义验证)
- `e3d_tree.py`:从导出 JSON 按 owner refno(记录头 word4-5)连边重建 PDMS 模型树。sam7200:6536 元素 / 359 根。
- 语义全对:最大子树根均为 **ZONE**(/EQUIPRACK-ACCESS 1414、/STEEL 754、/PIPES 502…);`ZONE→STRU/FRMW→SCTN/SNOD/SUBS→BOX/CYLI/DISH`;`EQUI /P1501A → 几何 + NOZZ /P1501A-N1//N2`(泵+管嘴)。
- ⇒ owner 语义确认;**层级+名称+属性的完整结构化模型纯离线可重建**。工具新增 `e3d_tree.py`(无 lint)。文档:findings §8.11。

## 2026-06-06(续8)— DA 跨页链式遍历(已实现)+ 解析重构
- `decode_da_list` 实现**节点链遍历**:`node word3=下一页 / word4=下一页内偏移`,顺链累计 payload 至 DA 字数(对照 `db4_get_list`)。重构出统一 `_parse_attr_words`(词表解析,DA/显式共用)。
- 验证:WELD `/WB1` 不变;sam7200 各元素 DA 均 ≤ 单节点(无多节点样本,逻辑与 db4 一致,向后兼容)。`decode_full_element` 改为给 `member_count`(成员=子 refno 列表,与 owner 链冗余,不再误当属性解析)。
- 重导出稳定:6536 元素 / **1144 命名**(词表解析对文本边界更稳,较前 1128 略增)/ 140 类型。无 lint。文档:格式规范 §7.9、progress。

## 2026-06-06(续9)— 引用属性语义 = (dbno, refseq) + 管道连通性
- 引用值 = 2 字 `(dbno, refseq)`(首字=dbno)。sam7200:3345 非零引用,dbno 直方图 23584(本库,791)/15192+(catalogue 外部库)。
- **776 本库引用解析到名**,恢复管道连通:CREF→管道分支、HREF→设备管嘴(/P1502A-N2)、TREF→三通(/100-B-1-B1-TEE1)。连接类(CREF/HREF/TREF)指向本设计库可解;规格/材料类(SPRE/MATR/ISPE)指向 catalogue 库(外部 dbno,需对应文件)。
- 文档:findings §8.12。临时脚本已清理。

## 2026-06-06(续10)— catalogue 库 + 跨库引用解析(闭环)
- `test-file/` 有更多库:`acp7002`(catalogue,dabacon dbno 15194)、`ams1112`(17496)、`amssys`(24575)。
- `acp7002` = catalogue:39 noun(SCOM/SCYL/SBOX/SPCO/CATE…),10124 命名,经 catvir.dat 正常解码。**全 sel=1**(sel=0/packed 在设计+目录库均未现,该路径实现但实务未用)。
- **跨库引用解析**:sam7200 中 dbno=15194 的 241 引用全部解析到 acp7002 catalogue 名:`.PSPE→SPEC /AVEVAHVACSPEC`、`.SPRE→SPCO …/STDAHU//RVCD//RSBEND`、`.LSTU→…/RTUBEA`。设计↔目录链路完整还原。
- 文档:findings §8.13、格式规范注。临时脚本已清理。

## 2026-06-06(续11)— 导出工具接入引用解析
- 增强 `e3d_export.py`:`--cat <db>`(可重复)加载 catalogue 库构建全局 refmap;`resolve_refs` 把每元素的引用属性(type 4/8/16 的 (dbno,refseq))解析成目标名(本库 + 跨库),写入 JSON `refs` 字段;未命中库用 PDMS `=db/ref` 形式。
- 实测 `--cat acp7002`:6536 元素/refmap=29672,**1897 元素带已解析引用**。例:`NOZZ /E1302B-S1 → CREF:/150-B-6-B1`(本库连接)、`CATR:=15206/514`(未加载的目录库,原样)。无 lint。
- 文档:findings §8.13(工具)、progress。

## 2026-06-06(续12)— 总结文档
- 新增入口总结 `docs/e3d 数据库分析/离线属性解析_总结.md`:数据模型全景(4 类文件)/ 端到端解析链 / 格式速查 / 工具链(5 脚本+用法)/ 验证结果 / 关键函数地址 / 后续。
- 更新 `E3D_DB_索引.md`:交付物表补 5 个新脚本 + 总结;更正结论 #5/#6(offset 来源=模式库 *vir.dat;元素属性离线解析全链闭环,先前"未闭环"已解决)。

### 项目状态:E3D 元素数据离线解析格式分析全部闭环
设计库 + 目录库 + 模式库(typedef/默认)+ attlib + record framing(隐式/DA/成员)+ 全属性类型(实/整/布尔/文本/引用)+ NAME + owner 层级 + 跨库引用,均纯离线解码并规模化验证。工具链:desvir_typedef_probe / e3d_attr_decoder / e3d_db_reader_v2(--attrs)/ e3d_export(--cat)/ e3d_tree。

## 2026-06-06(续13)— Rust 移植(独立 crate,已编译验证)
- `pdms_io` 整 crate 仍因缺 `dpc-sync` 无法构建;改做**独立 std-only crate** `tools/e3d_decode_rs/`(Cargo.toml 用空 `[workspace]` 与父 crate 隔离),把离线解码核心移植到 Rust。
- `cargo run --manifest-path tools/e3d_decode_rs/Cargo.toml`(rustc 1.98 nightly)**编译+运行成功**,与 Python 逐字一致:desvir.dat 745 类型、WELD 66 描述符、`name="/WB1"`、`POS=[9630.0,8072.0,5282.5]`、`ORI=[0.0,90.0,0.0]`。
- 证明:格式/算法(BE 读、模式库 typedef、offset 取值、低字在前 double、NAME 显式区)可干净移植到 Rust。后续可并入 `pdms_io`(待 dpc-sync 就绪)。
- `cargo clean` 清理构建产物;源码 `tools/e3d_decode_rs/{Cargo.toml,src/main.rs}` 为交付。

## 2026-06-06(续14)— Rust 整库遍历(独立 crate,已编译验证)
- 把 `tools/e3d_decode_rs/` 扩成**完整离线读取器**:头部→会话→B 树索引遍历 + 加载全部 `*vir.dat`(SchemaSet)+ 逐元素解 noun/NAME/POS。
- `cargo run` 与 Python **核心一致**:20 schemas/1478 类型;**elements=6536(精确一致)**;POS 逐字相同(`EQUI /P1501A=[9340,12145,645]`、`NOZZ /E1302B-S1=[0,-255,863.5]`…)。
- 次要差异(非正确性):named 1122 vs 1144、noun_types 156 vs 140、PAVE 745 vs 747 —— 源于 NAME 探测/`looks_like_noun`(Rust 先 trim 再判)边界,影响 <2%。
- `cargo clean` 清理产物。Rust 端现为可独立运行的整库读取器,待 dpc-sync 就绪并入 `pdms_io`。

### 下一步(可选,非阻塞)
- 对齐 Rust/Python 的 NAME 探测边界(消除 ~2% 计数差);Rust 端加 DA/引用解析与 JSON 导出。
- 加载更多 catalogue 库;并入 `pdms_io`(待依赖就绪)。

## 2026-06-06(续15)— db4_get_ce_att 全函数反编译 + type 枚举全库实测 + UDA 定位(三处历史待办收口)
- IDA 稳定(core.dll @127.0.0.1:13337, hexrays_ready)。把历史"type 14/15/18/19、sel=0 packed、UDA"用权威反编译 + 全库实测一次收口。
- **`db4_get_ce_att`(0x10612A50)整函数逐行**:`sel=(record[10]>>29)&1` 选 主desc[5]/备desc[8] + packed/unpacked;`v68` 判定标量(size==1 非文本=无计数字,值直存 record[off])vs 计数前缀;取值 switch(Bool 位、Real `sel=1`双精度/`sel=0` 1字 float、type14/18/19 特殊字数);定宽表 `dbl_10F68E90`(运行时填充,反推 Real0.5/Int1.0/Ref0.5/Text4.0)。
- **描述符 desc[0..9]**(`db4_get_att_dets` 0x10611FF0):补全 `[4]text_cap [7]aux [9]flag`;**UDA 运行时改写**机制(全局 `dword_115994E4/DC`,静态 0xFF)。
- **type 枚举全库实测**(新增只读 `type_enum_probe.py`,20 schemas/1478 noun):2/6=Real、3/7=Int、4/8/16=Ref、5=Bool、10/15=Text、**14/18=UDA表(UDATAB/UDAFTB/UDASTB)**、9/17=特殊、19 未用;WELD 双元素验证标量/向量/引用宽度。
- **UDA 存储定位**:UDA 值落于元素 `UDATAB/UDAFTB`(t14)、`UDASTB`(t18) 特殊表属性(off=0,随显式/DA 区),纯离线可提取容器;名↔字段映射在运行时全局表(需活进程)。
- 文档落地:`E3D_DB_文件格式规范.md` §7.6.1/§7.6.2/§7.6.3/§7.7.7/§7.3/§8 更新;`findings.md` 新增 §9。

### 状态
- 元素属性二进制取值规则(type 枚举 + sel/packed + 标量/计数前缀 + 定宽表)与 UDA 容器:**全部权威化/收口**。剩余仅运行时 UDA 名映射(需活进程)与 Rust 端引用解析/计数对齐(工程项,非格式分析)。

## 2026-06-06(续16)— UDA 元素存储深入(强类型值纯离线可解)
- 按"按推荐继续下一步":把 UDA 从"容器已定位"推进到"每元素取值纯离线可解"。
- IDA:`PDMS_Hash::IsUDA`(0x10001bc0)=`hash>0x171FAD39`;`db4_get_ce_att` 对 UDA 走 `off==0`→DA 链表按 hash 命中(UDA 值=显式/DA 区普通条目,带声明类型);`exppdms/EXRTPD`(0x10080F62)证实 `hash>0x171FAD39→LXANAM`(UDA 字典库 udalib:LXANAM/LXALEN/LXUNIT,DB_Uda),失败 "unknown UDA"。
- 实测(sam7200,新增只读 `uda_probe.py`):6536 元素 / **453 带 UDA / 765 条**,ctrl 类型 `{7:655,10:48,4:47,6:14,2:1}`;强类型值直接解出 `ref=(15195,2418)`、`real=(192,192)`、`text='D'`。两族 hash:`0x2C00xxxx`(普通强类型,值可解)、`0xFFF?xxxx`(type-7 表达式块 `[len][0][…][1601/1701]`,EXRTPD 语法,令牌语义待解)。
- 文档:格式规范新增 **§7.10**(UDA 存储 7.10.1-7.10.4)+ 更新 §8 row8;findings 新增 §10。新增探针 `uda_probe.py`。
- **结论**:UDA 容器 + 强类型值(real/int/text/ref)**纯离线打通**;剩余 UDA 名(需字典库 db,catalogue 同性质)与 0xFFF 族表达式 UDA 令牌语法 —— 均非阻塞。

## 2026-06-06(续17)— UDA 名能否离线还原:定论=不能(DEHASH 有损短码 vs 字典真名)
- 按"按推荐继续":定位 UDA 名来源。反编译 `DEHASH`(0x1065B930)= 既做 base-27(普通属性,已校验 NAME/POS/WELD/PIPE 全对)又做 **UDA base-64 有损短码**(`v=(hash−0x171FAD39)%0x1000000`,`:`+4 字符 `chr((v%64)+32)`)。
- 实测短码含非标识符字符(`0xFFF7AC4F→:6\_U`、`0x2C00D55A→:A@2X`…)⇒ 非真名。反编译 `LXANAM(0x100C7992)→ATATXT(0x10467D70)→DB_Attribute::findAttribute`:真名取自**属性注册表**(UDA 由字典库装入),**不经 DEHASH** ⇒ 真名只在字典库 db。
- 结论:离线可得 UDA hash + 有损短码 + 强类型值;**真名/类型/单位需字典库 db**(catalogue 同性质)。工具:`e3d_attr_decoder.dehash_uda_code` + `db1_dehash` 注释。文档:格式规范 §7.10.1 增"UDA 名能否离线还原"小节;findings §10.4。
- **UDA 线索收束**:存储 + 强类型值 + 判定/读取链 + 名来源(定论)全部查清;真名解析需外部字典库文件(非阻塞,与 catalogue 同类)。

## 2026-06-06(续18)— Rust 移植推进:与 Python 工具链全面对齐(Phase 5 部分落地)
- 按"按推荐继续(Rust 推进)":把独立 std-only crate `tools/e3d_decode_rs/` 从"reader(noun/NAME/POS)"扩成**完整解码器+导出器**,移植 `_decode_one`(全隐式类型)、链式 `decode_da_list`、`_parse_attr_words`(文本/引用/整)、`decode_full_element`、跨库 refmap 引用解析、std-only JSON 导出。
- **关键修复**:旧版 `element_name` 只扫首个 DA 节点 → named=1122/noun_types=156(<2% 偏差)。新版按 `db4_get_list` 链式遍历 DA 节点(node word3=下一页/word4=下一页内偏移)+ 按 DA 字数定界 ⇒ **named=1144、noun_types=140,与 Python 精确一致**。
- **全面对齐验证**(`cargo run --release`,8s 编译 / 5s 运行):
  - `elements=6536 / named=1144 / noun_types=140 / refmap=29672` —— 与 Python `e3d_export` **逐项一致**。
  - top nouns(PAVE747/BOX580/CYLI543/VERT378…)**逐项一致**。
  - POS 逐字:`WELD /WB1=[9630,8072,5282.5]`、`EQUI /P1501A=[9340,12145,645]`。
  - 跨库引用:`--cat acp7002` → `NOZZ /E1302B-S1 → CREF:/150-B-6-B1, CATR:=15206/514`(与 Python 一致)。
  - JSON 导出经 `json.load` 校验合法,结构 `{file,element_count,named,noun_types,refmap_size,elements:[{refno,noun,name,owner,implicit,explicit,refs}]}`。
- `cargo clean` 清理产物(源码 `tools/e3d_decode_rs/{Cargo.toml,src/main.rs}` 为交付)。无编译告警。
- **结论**:Rust 端已是**可独立运行的完整离线读取/解码/导出器**,与 Python 工具链全面对齐(计数/POS/引用/JSON);待 `dpc-sync` 就绪即可并入 `pdms_io`(Phase 5 集成)。
- **自检测试(续18b)**:`tools/e3d_decode_rs` 加 `#[cfg(test)]` 集成测试(数据缺失则优雅跳过):`dehash_roundtrip`(NAME/POS/WELD/PIPE)+ `sam7200_counts_and_weld_pos_match_python`(断言 6536/1144/140 + `WELD /WB1 POS=[9630,8072,5282.5]`)。`cargo test --release` **2 passed**。`cargo clean` 已清理。

## 2026-06-06(续19)— pdms_io 集成落地(用户授权选项 2):模块就位 + 解阻 dpcsync,整 crate 构建受 NASM 环境阻塞
- 用户选 **2**(授权调整可选依赖 + 接入)。落地:
  - **解阻 dpcsync**(已授权):`Cargo.toml` 注释 `dpcsync` path 依赖 + `sync-archive` 改为 `["dep:blake2"]`(`sync` 模块本就 `#[cfg(feature="sync-archive")]` 全门控;`watch.rs` 的 `use dpcsync` 早已注释)。已写明恢复方式。⇒ Cargo 清单解析通过、开始编译整依赖树(确认 dpcsync 不再阻塞)。
  - **接入 E3D 解码**:新增 `src/e3d_decode.rs`(std-only 自包含模块:`SchemaSet`/`Edb`/`Element`/`decode_full`/`index_db`/`db1_dehash`/`dehash_uda_code`/`Val`/`Attr`),`lib.rs` 加 `#[allow(dead_code)] pub mod e3d_decode;`。
  - **模块级验证(edition 2024)**:用临时 `_modcheck`(edition 2024,`[lib] path=../src/e3d_decode.rs`,std-only)`cargo test` → **2 passed**(dehash_roundtrip / uda_short_code),证明模块与 pdms_io 同 edition 可编译;临时 crate 已删。无 lint。
- **整 crate `cargo check --lib` 仍失败**:`cargo tree -i aws-lc-sys` 查明链路 `aws-lc-sys ← aws-lc-rs ← jsonwebtoken ← surrealdb-core ← surrealdb ← aios_core(../rs-core) ← pdms_io`。即 **aios_core 硬依赖 surrealdb → 需 aws-lc-sys → 需 NASM**(`NASM command not found! Build cannot continue.`)。与本次代码/dpcsync 无关,是**环境工具链缺 NASM**。
- 探查中曾试 reqwest `rustls-tls→native-tls`(以为是 reqwest 引入 aws-lc),经 cargo tree 证实来自 aios_core→surrealdb,**已回退** reqwest 到原配置。
- **结论**:E3D 解码已**作为 pdms_io 模块就位并通过模块级编译/测试(edition 2024)**;整 crate `cargo build/test` 仅差 **NASM**(为 aws-lc-sys 汇编,经 aios_core→surrealdb)。装上 NASM(如 `winget install nasm` 或置于 PATH)后即可整 crate 构建并跑集成测试。

## 2026-06-06(续20)— 装 NASM 后整 crate 推进:卡在 rs-core↔surrealdb API 漂移(预存,非本次代码)
- 用户"按推荐"→ 装 NASM:`winget install NASM.NASM`(3.01,装到 `C:\Program Files\NASM`),会话 `PATH` 注入后 `nasm -v` OK。
- 重跑 `cargo check --lib`:**NASM 阻塞解除**,aws-lc-sys / surrealdb / 整依赖树编译通过,推进到 **`aios_core`(../rs-core)** 才失败:**8 个 `error[E0053]`**——rs-core 的 `FromValue::from_value` 返回 `anyhow::Result<Self>`,但其 git `surrealdb` 现要求 `Result<Self, surrealdb::Error>`(`rs-core/src/types/{named_attmap,refno,plant_aabb}.rs` 等)。
- ⇒ 这是 **rs-core 对其 surrealdb 依赖的预存 API 漂移**(surrealdb 分支更新了 `FromValue` 签名,rs-core 未跟),**与本次 E3D/dpcsync 改动无关**(全程未碰 rs-core)。印证 findings 早期"rs-core 重 git 依赖树难以可靠构建"。
- **现状**:E3D 集成本身完成(模块就位 + 模块级 edition-2024 测试通过);整 crate 构建尚差**修 rs-core 的 surrealdb 兼容**(改 8+ 处 `from_value` 返回类型 + 错误转换;属 ../rs-core 兄弟 crate,且可能有后续连锁),超出"E3D 接入"范围,留待裁决/单独处理。
- 交付改动(pdms-io 内):`Cargo.toml`(dpcsync 解阻,reqwest 已回退)、`src/e3d_decode.rs`、`src/lib.rs`。NASM 已装(系统级,持久)。

## 2026-06-06(续21)— 写侧前沿启动:页无校验和 + 安全在位定长值写(已验证)
- 按"按推荐继续(开启写入)":先查页完整性(写的前置门)。反编译 `db1_read_page`(0x10630C20=仅 FHDBRN,无 checksum)、`db1_write_page`(0x10633FB0=刷原始页,无 checksum)⇒ **DABACON 页无校验和**,就地改值字节(保持大端+低字在前)即字节合法、可读回。
- 新增 `docs/e3d 数据库分析/e3d_write.py`:`set_inline_value` 安全就地写定长内联值(实/整/引用,组件数不变,不动框架);**仅对副本**操作。
- **实测 PASS**(sam7200 副本):WELD `/WB1` POS→(1000.25,−2000.5,3000.75),读回新值、NAME 不变,字节 diff 全在该值 24B 区间。无 lint;临时脚本已清。
- 文档:格式规范 **§12**(离线写入/页完整性)、findings §11。
- 边界(写侧后续):文本/变长/DA/UDA 改写需重排+COW;完整写需复刻 `db5_save_work`(COW+会话+刷脏页+page0 重指向)。属更大新范围,留待裁决。

## 2026-06-06(续22)— 完整写提交机制权威化:db5_save_work(2.10)反编译
- 反编译 `db5_save_work`(0x105E9C80,"5.4.4"):提交=COW+追加新会话(sesno+1)+重指向 page0。步骤:claim→读 page0 当前会话/sesno→校验(会话 type3 否则664;sesno+1 否则665)→分配新会话页+写元数据→重映射 db-block 属性(索引根=0xCC47DF/13387743、基准=7618377)→刷脏/COW 页(db1_write_page/FHDBWN)→重指 page0 sesno→解锁。
- 认知:E3D 编辑=**多版本追加**(旧页留存、COW 新页、新会话指新 B 树根、原子重指 page0),与读侧会话链/每会话索引根自洽。
- 重要:仓库 `数据库写入架构.md` 是 **E3D 3.1**(0x5A 地址),本次给出**当前 2.10 权威版**;格式规范新增 **§12.4** + 离线完整写实现计划(页分配/COW/B 树写侧/新会话/重写 page0;高风险,建议独立里程碑)。findings §12。
- 现状:写侧 = 安全在位定长写(已实现验证,§12.1–12.3)+ 完整 COW 提交机制(已权威分析 + 计划,§12.4,未实现)。完整写实现属更大独立里程碑。

## 2026-06-06(续23)— B 树写侧(插入/分裂)2.10 复核:写机制分析全部完成
- 反编译 `db3_insert_page_entry`(0x1061B5C0,"3.2.4")+ `db3_split_node`(0x1061BA50,"3.2.6")。索引/表页(type5)写布局:7 字页头、`word6`=空闲字数、条目自 word7 升序、容量 `dword_10F68F4C`。插入:二分定位→腾位写入→`word6` 减;满则置 split。分裂:`db1_get_new_page` 新兄弟页→分裂点约半数→搬上半→修 `word6`→向父递归(最深50/533),校验 type5/表名/层级(659/660/661)。
- ⇒ 在位定长写不改 key/框架,故不碰 B 树(其安全性来源)。格式规范新增 **§12.5**;findings §12.1。
- **里程碑:E3D 离线"写"的全部机制(页无校验和 + COW + 刷页 + B 树插入/分裂 + 会话提交/page0 重指)均已权威分析**;读侧早已全闭环。完整写**实现**为独立高风险里程碑(§12.4 计划),需真 E3D 双读验证基线,建议显式开启再做。

## 2026-06-06(续24)— 安全在位写并入 pdms_io Rust 模块(读+写,4 测试通过)
- 把 Python `e3d_write.py` 的安全在位写移植进 `src/e3d_decode.rs`:新增 `set_inline_value`(real/int/ref 定长内联,组件数不变,sel=1;不动框架/键 ⇒ 不碰 B 树)+ `find_record_offset` + `Edb::from_bytes`/`bytes()`。
- 模块自检测试扩到 **4 个**(edition-2024 临时 crate `cargo test`,**4 passed**):`dehash_roundtrip`、`uda_short_code`、`read_counts_and_weld_pos`(实跑:6536/1144 + `WELD /WB1 POS=[9630,8072,5282.5]`)、`inline_write_roundtrip`(在位改 POS→round-trip,改动全在值区间内)。临时 crate 已删;无 lint。
- ⇒ 集成进 `pdms_io` 的 `e3d_decode` 模块现支持**读(全属性/NAME/引用)+ 安全在位写**,均有测试。整 crate 构建仍待 rs-core↔surrealdb 修复(与本模块无关)。

## 2026-06-06(续25)— 多库规模/健壮性验证 + 修复 amssys 崩溃
- 跨**全部可用元素库**跑解码器(规模/健壮性):
  - `sam7200`(设计,6.9MB):6536 元素/1144 命名/140 noun。
  - `acp7002`(目录,16.8MB):**34450 元素**/28528 命名/60 noun(DATA/TABQUE/DTSE…)。
  - `ams1112`(大设计,**103MB**):**182950 元素**/29362 命名/137 noun(PAVE 96484…),~30s(Python)。
  - `amssys`(系统,15.3MB):1212 元素/308 命名/32 noun(LTEA/CMMO/DB/TEAM/MDB — 系统库)。
- **发现并修复崩溃**:`amssys` 触发 `IndexError`——`e3d_attr_decoder.py::Schema.typedef` 对**短 skeleton-K(<10 字)** 直接 `range(skel[9])` 越界(Rust 端早有 `len<10` 守卫,Python 原版缺)。已加 `if len(skel) < 10: return None`。修后 amssys 正常解。无 lint。
- **附带发现**:`ams1112` 有 **23 个 sel=0(packed)记录**、`amssys` 3 个 —— 首次出现真实 sel=0 样本(§7.6.2 packed-float 路径此前仅代码确证 + 合成验证;这些可作未来 packed 路径的真实验证目标)。
- ⇒ 解码器经**设计/目录/系统库 + 103MB 大库**规模验证,健壮性 bug 已修。findings 续记。

## 2026-06-06(续26)— 真实 sel=0 样本核验:不充分(诚实记录)
- 尝试用 ams1112 的真实 sel=0 样本验证 §7.6.2 packed-float 路径。实测一例 `WALL`(sel=0,impl=226):唯一非零实属性 `GRAD=0.0`(无区分力);多个属性解出**不合理值**(如 type-4 引用 `JOIS=(1361709870,1072403063)`,dbno 达十亿级;GTYP=3219144129)。
- ⇒ 这些 sel=0 记录更像**非干净/变体记录**(非干净主记录),**不能据此干净验证 packed 路径**。与既有结论一致(sel=0 在干净设计/目录记录中不出现)。packed-float 路径仍为**代码确证(db4_get_ce_att 反编译)+ 合成往返验证**;真实干净样本验证为**遗留 niche 项**(需精确 packed 记录布局,可能整记录而非仅实型压缩)。
- 不修改格式规范结论(避免过度声称);仅此处诚实记录。无遗留临时文件。

### 续26b — 排除 schema-selection 假设(packed 布局确为根因)
- 查 schema 多重定义:1478 noun 中 93 个被多库定义,但均为**通用/共享 noun**(NAME/WORL/MNUM/TYPEX/DBMETA/UDATAB/UDAFTB/UDASTB…,出现在~全部 20 库,typedef 一致,"首选"无害)。
- **WALL 仅在 desvir.dat**(单库)⇒ 其 typedef 无歧义,garbled sel=0 解码**非 schema 选择问题**。JOIS/JOIE(schema 标 type4 引用)在该处解出**合理 double(0.89/0.54)**⇒ 确认 **sel=0/packed 记录物理布局与"main offset+1字 float"模型不同**(packed 压缩使后续字段整体位移)。
- ⇒ packed 记录布局为**确凿的深度 niche 遗留项**(仅非干净/系统记录涉及;干净设计/目录数据 100% 已成)。需专门 RE packed 记录构建/读取(逐字段压缩规则)方可干净解;优先级低,留待显式开启。

## 2026-06-06(续27)— 大库 walk cap 截断 bug(Rust+Python 修复;ams1112 真值校正)
- Rust 规模验证发现:`tools/e3d_decode_rs` 在 ams1112 只得 138574 元素(旧 cap 300k 截断 B 树遍历)。Python 之前的 182950 **同样是截断值**(walk cap 400k)。
- 修复:Rust `walk` cap 300k→**4,000,000**(`main.rs` + `src/e3d_decode.rs` index_db/find_record_offset);Python `e3d_export.py`(300k→4M)、`e3d_db_reader_v2.walk_index` 默认(100k→4M)。
- 修后双实现**一致**:Rust **elements=422336**、Python full-walk **422322**(差 14 ≈ 0.003%,NAME 边界);noun_types 155。⇒ ams1112 真实元素 ~**42.2 万**(此前 18.3 万为截断)。
- 性能:Rust 跑完 103MB/42 万元素 ~2s,Python ~30s(~15×)。标准 crate 测试仍 2/2(sam7200 小库不受 cap 影响)。无 lint。
- ⇒ 修了一个**真实大库截断 bug**(两端),并校正 ams1112 计数。`build` 产物已清。

### 续27b — "no-schema" 元素 = 误报记录(非 schema 缺口)
- 查 no-schema noun 名:`NONE`(×14)+ 一批**乱码串**(XYZMD/PXTJF/VHY A/ZYBIF/SQNUE…)。⇒ 这些是通过宽松 `looks_like_noun`(1-8 大写)的**误报记录**(乱码 noun hash,非干净记录),**非 schema 覆盖缺口**;占比 ~0.006%(25/422336),解出无属性、无害。decoder 行为正确(返回无 schema)。可选:收紧记录有效性过滤(风险:误删真实边缘元素),不必要。

## 2026-06-06(续28)— Rust↔Python 属性级对比 → 发现并修复 CJK 名 latin1 bug
- 跨实现属性级 diff(sam7200,按 refno 匹配):**6498/6501 元素逐属性完全一致(99.95%)**,**implicit 属性 0 mismatch**(核心 schema 解码完美对齐)。
- 3 处差异同源:**含中文的元素名**——Rust(`from_utf8_lossy`)正确解出 `/穹顶`、`/DC天花板`、`/天花板_0m层0.00`;**Python 用 `latin1` → 乱码** `/ç©¹é¡¶`。
- **修复**(真实正确性 bug,对中文工厂数据直接相关):`e3d_attr_decoder.py` `_parse_attr_words` 与 `extract_names` 的文本解码 `latin1`→**`utf-8`**(`extract_names` 过滤放宽到允许非 ASCII 可打印、剔除控制/U+FFFD)。
- 验证:重导出 sam7200,中文名 `穹顶/天花板` 正确出现(6 处),乱码残留 **0**。无 lint;无遗留临时文件。Rust 端本就 UTF-8(无需改)。
- ⇒ 跨实现对比既验证了深度一致性(implicit 0 diff),又抓出 + 修掉一个真实 CJK 编码 bug。

## 2026-06-06(续29)— 安全写:int/ref 路径补验(此前仅验 real/POS)
- 对 sam7200 副本 round-trip 验证 `set_inline_value` 的全部支持类型(WELD /WB1):`WLDN`(int t3)`[0]→[777]`、`SPRE`(ref t16)`(15192,231136)→(99,12345)`、`TSPE`(ref t4)`(0,0)→(7,88)`——均读回新值且字节 diff 全在值区间内,**ALL PASS**。
- ⇒ 安全在位写覆盖 real/int/ref **全部已声明类型**,均验证。注:Python `set_inline_value(buf,record_off,desc,sel,values)` 与 Rust `(buf,ss,record_off,attr_hash,val)` 签名不同(Python 低层、Rust 高层),均正确。无遗留临时文件。
- **永久化**:把 int/ref 写验证加进 `src/e3d_decode.rs` 测试(新增 `inline_write_int_and_ref`:WLDN int→777、SPRE ref→(99,12345),按名查 hash 后写回校验)。模块自检测试现 **5 个,edition-2024 全过**(dehash/uda/read_counts/inline_write_roundtrip/inline_write_int_and_ref)。无 lint;临时 crate 已删。⇒ 集成模块对**读 + 全类型安全写**均有永久回归覆盖。

## 2026-06-06(续31)— rs-core 集成阻塞精确评估(只读,未改 rs-core)
- 只读核查 rs-core 的 surrealdb 兼容阻塞(整 crate build 的唯一拦路):`FromValue::from_value` 实现共 **7+ 处**(`types/refno.rs`×2、`types/named_attmap.rs`、`types/plant_aabb.rs`、`shape/pdms_shape.rs`、`rs_surreal/geometry_query.rs`、`parsed_data.rs`、`accel_tree/acceleration_tree.rs`),签名 `anyhow::Result<Self>`,新 surrealdb 要求 `Result<Self, surrealdb::Error>`。
- 实现体用 `anyhow::anyhow!(...)` 构造错误 + `Ok(...)`;迁移需逐处改返回类型 + 用 `surrealdb::Error` 构造错误(需新 Error API),且 `FromValue` 仅是 surrealdb-3.1 API 漂移的**第一道墙**(修完可能再现更多 API 变更)。
- ⇒ **结论**:rs-core 阻塞 = **surrealdb-3.1 迁移**(7+ 实现 + 可能连锁),属**项目外兄弟 crate**对**移动中 git 分支**的迁移,**非快速修复**;应由团队(拥有 rs-core + 知 surrealdb 分支)处理,或经显式授权+限定范围。**未在无授权下改动 rs-core**(仅只读评估)。E3D 模块本身与此无关、已在 edition-2024 独立验证。

## 2026-06-06(续30)— 目录库(acp7002)Rust↔Python 属性级对比:100% 一致
- 扩展对比到不同 db 类型(目录库 acp7002,34450 元素,SCOM/SCYL/… noun)。结果:**34450/34450 = 100.00% 逐元素逐属性完全一致**(hdr/implicit/explicit mismatch 全 0)。
- ⇒ 双实现在**设计库(99.95%,implicit 0 diff)+ 目录库(100%)** 均确认逐属性忠实一致(设计库剩余差异即此前已修的 CJK 名,本次目录库无任何差异)。无新 bug——干净确认 Rust/Python 解码器一致且稳固。无遗留临时文件。

### 续27c — sel=0 定论:干净记录中从不出现(packed 路径确认不适用干净数据)
- 跨全部库分类所有 sel=0 记录(有效 schema + 干净 `/`名 = clean):sam7200 sel0=1/clean=0、acp7002 sel0=0、ams1112 sel0=40/clean=0、amssys sel0=3/clean=0。⇒ **干净 sel=0 记录 = 0(全库)**。
- ⇒ 干净元素 100% sel=1;packed(sel=0)路径已代码+合成验证,且**确认不适用任何干净数据**(真实样本验证为空集,非缺口)。先前 ams1112 WALL garbled 由此完全解释(非干净记录,非解码 bug)。格式规范 §7.6.2 已更新为定论。
- **packed 线程定性收口(definitive)**,非"待取证/深 niche 未决"。

### 续26c — packed 布局精确刻画(closure)
- 查 WALL typedef(desvir.dat,46 描述符):**main_off(desc5)≠alt_off(desc8) 的有 9/46**(GTYP@11/NUMB@12/BUIL@13 等实数之前的字段 main==alt;实数之后因 packed 半宽而位移)。⇒ packed(sel=0/main)与 unpacked(sel=1/alt)**确为两套布局**。
- 解码器对 sel=0 **已用 main offsets + 1 字 float**(与 db4 代码一致),机制+schema 层面**已理解**。残留:ams1112 单个 WALL 样本仍解出怪值(JOIS/JOIE 在 type4 处呈合理 double)——或该记录非干净,或非实型在 packed 下另有宽度细节;**干净真实样本验证**为唯一遗留(低优先)。
- ⇒ packed 路径:机制(db4)+ schema(main/alt 双布局)+ 合成验证 **已成**;干净真实样本验证待专门取证。技术线程到此精确收口。

## 2026-06-06(续32)— Page 0 头部两个 [存疑] 字段收口 + 多处误标更正(IDA 创建函数 + 4 样本)
- 按"继续分析 e3d 数据文件格式":攻格式规范 §8 仅剩的两个头部 [存疑](row2 `stored_page_count`、row3 `session_page_no`)。IDA 稳定(core.dll @127.0.0.1:13337)。
- 跨 4 样本(sam7200/acp7002/ams1112/amssys)数据探针 + 反编译 `db2_create_master`(0x1062B510,逐字段写 page0 `v26[0..16]`)、`db2_open_db`(0x1062F8F0)、`db5_save_work`(0x105E9C80):
  - **`0x38`(原 stored_page_count)= DBNO(refno_0)**:`v26[14]=(db_num&0x1FFF)|(((db_num&0x3E000)|1)<<13)`,4 样本公式逐一吻合(0x3C20/0x3B5A/0x2458/0x3FFF);ams1112 实 50677 页≠9304 ⇒ 绝非页计数。`db_num≤0x1FFF`(db2_open_db err512),bit13–17=库类型。
  - **`0x3C`(原 unknown_3=恒2)= refseq(refno_1)**:创建初值 0、save_work 由 db-block 属性 7 刷新;实测 2/39268/3/0。`(0x38,0x3C)`=db 根/world 引用 refno。
  - **`0x30`(原 session_page_no)= extract/extent 计数**:`v26[12]=ext_no+1`;反证页 3 为 type5 非会话;save_work 不写、open 不校验。
  - 顺带更正:**`0x20`=schema_type_id**(选 `*vir.dat`,实测 word8↔vir.dat 头 w2 一一对应;非创建时间)、**`0x24`=schema_version**、`0x0C/0x10`=ext_no副本/创建标志、**真正创建信息=0x44+ ASCII**。page0 是整页 db-control-block(头+ASCII+db-block 属性),会话/索引根/claim 权威源是 db-block 属性。
- 文档落地:`E3D_DB_文件格式规范.md` §2 头表重写 + 新增 §2.1(DBNO 编码 + db-control-block)+ §8 row2/3 标记已解决 + 新增 row3b;`findings.md` 新增 §13。
- 剩余(非阻塞):`0x30` 精确增量语义(`db2_create_extract` 未反编译)、`src/defines.rs::PdmsHeader` 旧误称字段名(改名有跨文件波及,留独立工程项)、0xFFF 族表达式 UDA 令牌语法(§10.2,下一候选)。

### 状态
- 格式规范 §8 两个头部 [存疑](stored_page_count / session_page_no)**全部收口**;另更正 4 处误标 + page0 结构认知。临时探针 `_hdr_probe.py` 已删。

## 2026-06-06(续33)— `0xFFF` 族 UDA 解码:= 序列化 PDMS 表达式(派生属性),收口"令牌语义待解"
- 按"按推荐继续下一步":攻 §7.10.3 / §10.2 标注的 `0xFFF` 族表达式 UDA 令牌语义。
- 关键发现:仓库**已有** PDMS 表达式 opcode 体系(`docs/expression_opcode_table.md` + `crates/parse_pdms_db/src/parser/attribute/expression_payload.rs`)。`0xFFF` 族 UDA 值正是该语法的序列化表达式(派生/计算属性规则),**非字面量**。
- 解明外层封装:`[len][0][count][sublen=len-2][1] <RPN...>`;RPN = `0x6A` 属性引用(+ `1601/1602`OF、`1701/1702/1703`WRT 限定符)、`0x65` 数值、`80x/100x` 运算符(`*`/`+`/`MIN`/`ABS`/`INT`…)。
- 新增只读工具 `docs/e3d 数据库分析/uda_expr_probe.py`(`expression_payload.rs::decode_words` 的 Python 端口)。sam7200 实测:17 distinct `0xFFF` hash / 655 条,标量算术+属性引用子集**完整解出 193 条**,典型 `ATTRIB CDPR FTHK OF WRT*2 + ATTRIB CDPR GTHK OF WRT + 37.5`、`ATTRIB HEIG OF CYLI WRT`、`MIN(ABS(300*N − INT(ATTRIB LDPR OLEN OF TMPL WRT/300)*300 − ...),12)`。
- 余 `0x6B/0x6C/0x6D`(数组/列表 opcode,含最高频 `0xFFF7AC4F ×312`)实参布局待补(core.dll `expract` 模块 ~`0x101E0000`)。可直接复用仓库 `expression_payload.rs`(剥 5 字头)做生产级解码。
- 文档:格式规范新增 §7.10.5、更新 §7.10.3/§8 row8;findings 新增 §14。临时探针已转正为 `uda_expr_probe.py`,无 lint。

### 状态
- UDA 线索 `0xFFF` 族(派生/表达式 UDA)**令牌语义收口**(标量/算术子集纯离线可读)。剩余:`0x6B/0x6C/0x6D` opcode 实参布局、UDA 真名(字典库)——均非阻塞。

## 2026-06-06(续34)— `0xFFF` UDA `0x6B/0x6C/0x6D` opcode 攻克:= DORTXT 几何字面量,解码率 193→647/655(98.8%)
- 按"按推荐继续":攻 §7.10.5 余下的 `0x6B/0x6C/0x6D`(此前猜"数组/列表")。
- 反编译 `EXRTPD`(0x10080F62,`exppdms/EXRTPD`,表达式→文本):opcode 用 ASCII 字符 switch;`'j'`(0x6A)=属性引用、**`'k'/'l'/'m'/'s'`(0x6B/0x6C/0x6D/0x73)→ `DORTXT(...)`** = 方向/取向/位置/坐标几何字面量(`'s'`=AT)。消费规则 `*a3 += word[*a3-1]`(计数前缀 `count=words[i+1]`,推进 `i+1+count`)。
- 实测:这些 UDA 的 count = 整条 body(如 `0xFFF7AC4F` count=97=全 98 字)⇒ **单一参数化几何值**,分量为子表达式。
- `uda_expr_probe.py` 加 `0x6B/0x6C/0x6D/0x73` 几何字面量分支(计数消费 + 递归解内部分量)→ sam7200 解码 **647/655(98.8%)**,解出参数化型材几何/壁厚公式(`GEOM_K(...SQRT(POW(ATTRIB CDPR DE OF WRT,2)+...))`、`...CDPR FTHK*2+CDPR GTHK+37.5`)。
- 文档:格式规范 §7.10.5 重写(0x6B=DORTXT 几何字面量、98.8%)、§8 row8、§9 加 EXRTPD;findings §14 更新。无 lint。

### 状态
- `0xFFF` 派生 UDA 解码 **98.8% 闭合**;opcode 全谱(标量+几何)语义清楚。剩余仅 8 条几何分量美化 + UDA 真名(字典库)——非阻塞。

## 2026-06-06(续35)— 表达式 UDA 解码器跨库验证
- 在 `acp7002`(catalogue)/`ams1112`(大设计)/`amssys`(系统)上跑 `uda_expr_probe.py` 验证规模/泛化:
  - `acp7002`:**34475/36141(95.4%)解出**(35 distinct hash)—— 目录库含大量参数化几何 UDA。
  - `ams1112`/`amssys`:**无 `0xFFF` UDA**(派生表达式 UDA 主要在设计元素引用目录参数 + 目录库本身)。
- ⇒ 表达式 UDA 解码器规模化通过(sam7200 98.8% + acp7002 95.4%/3.6万条)。findings §14.2 记。
- **格式分析现状:读侧全闭合 + 写侧机制全分析 + 头部字段厘定 + UDA(强类型/派生表达式)解码 —— E3D 数据文件格式分析基本完成。** 唯一剩余大块 = 写侧**实现**落地(§12.4,高风险,需显式开启)。

## 2026-06-06(续36)— 选项 B:把头部发现回灌代码(安全,已验证)
- 用户"按推荐继续"(推荐为 B/D)→ 执行 B(把今天的头部发现工程化,纯安全)。
- **schema 自动选库**(操作化 0x20=schema_type_id 发现):`e3d_attr_decoder.SchemaSet` 加 `by_templ`(template_id→schema)+ `schema_for_db(db_buf)`(读 header 0x20 选定 `*vir.dat`)。实测 sam7200 header `0x20=0xB0692` → **自动选定 desvir.dat** ✓。
- **`e3d_db_reader_v2.py` 头部字段正名**:新增 `dbno`/`dbno_refseq`/`schema_type_id`/`schema_version`/`extent_counter` 正确字段 + 旧名别名(`creation_time`/`session_page_no`/`stored_page_count`,避免破坏调用方);修正 `== 文件头 ==` 打印(`dbno=0x3C20 refseq=2 schema_type_id=0xB0692 extent_counter=3`)+ `--attrs` 增打印自动选定的主 schema + JSON 头补正确字段。
- **`src/defines.rs::PdmsHeader`**:**仅改注释**(字段名跨 13+ 文件引用,且整 crate 不可构建,改名风险高 → 不改名),逐字段标注真实语义(0x20=schema id、0x24=schema 版本、0x30=extent 计数、0x38=DBNO、0x3C=refseq)+ 结构级校正说明。
- 验证:`e3d_db_reader_v2.py --attrs` 运行通过,头部字段与主 schema 自动选定均正确;两个 Python 文件无 lint。
- 写侧完整实现(A)**未动**(高风险,待显式授权)。

### 状态
- 选项 B 完成(安全工程化:schema 自动选库 + 头部正名 + defines 注释校正,均已验证)。E3D 格式分析全线收口;后续仅写侧实现(A,需授权)为大项。

### pdms_io 集成阻塞根因(续18b 查明,read-only)
- `Cargo.toml:43` `dpcsync = { path = "../dpc-sync", optional = true }`,但 `D:\work\plant\dpc-sync` **不存在**。`dpcsync` 仅 `sync-archive` feature 用(`src/sync/*`,`lib.rs` 已 `#[cfg(feature="sync-archive")]` 门控;`watch.rs` 的 `use dpcsync` 已注释)。⇒ **代码门控正确,默认 build 不需 dpcsync**;唯一阻塞是 **Cargo 对不存在的 path 依赖做清单解析即失败**(与 feature 无关)。
- ⇒ 解阻路径(任一):(a) 提供 `../dpc-sync`;(b) 把该可选 path 依赖改为可解析(git/占位 crate)或暂注释 + 确认 `sync` 全门控。二者均属**构建配置变更**,需用户裁决(其真实环境可能有 dpc-sync,擅改会破坏 `sync-archive`)。
- 其余依赖(`aios_core`/`surrealdb` git 树)能否在本环境构建仍待验证(findings 早期标注偏重)。故 Phase 5 整 crate 集成保持"待裁决/待依赖"。

## 2026-06-07 — 写侧**实现**启动(用户授权选项 A):Slice 1 离线 COW + 新会话提交(已实现 + 多版本验证)
- 用户"按推荐继续"= 授权写侧完整实现里程碑(§12.4)。采用**安全方式**:只对副本 + 往返/双读验证,Python 原型分步(vertical slice)。
- **Slice 1 = 离线 COW + 新会话提交**(把 `db5_save_work` 落地)。新增 `docs/e3d 数据库分析/e3d_write_full.py`:
  - 持久化/COW B 树:COW 数据页(`set_inline_value` 改值)→ COW 叶→根路径(条目改指新子页)→ 追加新会话页(`sesno+1`/`index_root=新根`/`last_ses`/`end`)→ 重指 page0 `w10`(0x28)。兄弟子树共享,仅新增 O(深度) 页。
  - **修坑**:refno 可有多条叶项(主记录 + 二级/成员);提交按精确(数据页,字偏移)匹配主记录,读回按 `refno + is_main`(`w0` 干净 u16 impl + noun 可 dehash)过滤(实测 `/WB1` 在 pg2799 无 POS、pg807 为主记录)。
- **验证(sam7200 副本,/WB1,连提两次 POS)**:`sesno 36→37→38`、`index_root 3377→3387→3392`、数据页 `807→3384→3389`;沿会话链读回 **sesno38=新值2 / 37=新值1 / 36=原值(9630,8072,5282.5)**;**字节 diff 仅 page0 会话指针字 0x28..0x2B**(其余全为尾部追加)⇒ COW 旧数据不可变逐字节坐实。demo PASS、无 lint、temp 已清。
- **开库契约 IDA 确证**(core.dll @127.0.0.1:13337):`db2_get_db_int_att`(0x10622F20)从内存 db-block 条目(40B/extract)取 `sesno=+12(字3)/end=+20(字5)/index_root=+28(字7)/claim=+36(字9)`,`db2_find_current_db_block`(0x10622DC0)= `v11+40*v8`。这些偏移**与会话页字布局逐一对齐** ⇒ **db-block 开库时从 `page0[w10]` 所指会话页装入** ⇒ "改会话页 + 重指 page0 w10" 对真实 E3D **机制完备**(待真机 round-trip;`w11` 保守不改)。校正:§12.4 旧称"索引根=属性 13387743/claim=7618377"措辞不准,实际用小 case id(index_root=case10)。
- 文档:格式规范新增 **§12.6**;findings 新增 **§15**。

## 2026-06-07(续)— 写侧 Slice 2:变长 DA/显式文本改写(已实现 + 多版本验证)
- 用户"按推荐继续"= 继续写侧下一 slice(变长/DA 改写)。把提交逻辑重构出**共用核心** `_commit_edited_data_page`(COW B 树路径 + 新会话 + 重指 page0),Slice 1/2 仅"如何产出改后数据页"不同。
- **Slice 2 = `cow_commit_da_text`**:同页单 DA 节点重建——解析 DA payload 词表 → 按 hash 定位条目 → 用 `[byte_len][UTF-8 4字符/字,高字节在前]` 重编码文本 → 拼回 → 更新节点头 `w0` + 记录 `rec[10]` DA 字数。**允许增长**(COW 页仅被改后元素引用,页尾即空闲;上界=页尾/同页成员区起点,超出则拒绝=relocation slice 2b)。
- **修坑(记录自包含)**:首版改名读回仍是旧名 —— 根因:COW 后记录 `word6`(DA 页指针)仍指旧页 → 读回旧 DA。提交核心通用修正:COW 后把记录页内指针 `word6`(DA)/`word8`(成员)由旧页改指新页。
- **验证**(sam7200 副本,`/WB1`→`/WB1-COW-RENAMED`,增长):DA 字数 38→41;**新会话=新名+POS不变 / 旧会话=旧名+POS不变**;字节 diff 仅 page0 会话指针。双 slice 自检 demo 均 **PASS**、无 lint、temp 已清。
- 文档:格式规范 §12.6 增 Slice 2;findings §15.4/§15.5。

## 2026-06-07(续2)— 写侧 Slice 3:新增元素(B 树键插入,已实现 + 验证)
- 用户"按推荐继续"= 继续写侧 Slice 3(新增元素)。重构出共用 `_append_session` + `_rewrite_da_text_in_page`;新增 `cow_insert_element`。
- **Slice 3 = 克隆既有元素新增**(新 refno=当前最大+1、新 NAME):克隆源页改 refno+NAME → 追加数据页 + 自包含修正;沿**最右路径**(避开 word7 的 `0x80000001` 哨兵=最左子树)到最右叶,在 word6 末尾追加索引项、`word6-=4`;**最大键追加不改分隔键**(=子树最小键),祖先仅 COW 重接最右子指针到根 → 新会话提交。
- **关键发现(B 树)**:索引页条目数由页头 `word6`(空闲字数)界定 = `(512−7−word6)/4`,**非空终止**(根=3 条 ✓;pg3264=40 条而非空终止扫到的 123)。
- **顺带发现读取器缺陷**:`E3DDb.walk_index` 空终止扫描会**越读** word6 之后脏条目 → 原始条目计数不可信(下游过滤,故 6536 元素数不受影响)。验证改用 word6 界定。留作读取器改进项(改 word6 界定)。
- **修坑**:首版验证用 `len(walk_index)` 比较计数(因越读脏条目)误判;改为权威 `word6` 计数(最右叶 word6 457→453=+1 条目)。
- **验证**(sam7200 副本,克隆 `/WB1`→`/NEW-ELEM-COW`,新 refno `(0x5C20,0x3D8F)`):新会话解出新元素(noun=WELD/owner 同源/NAME 新);旧会话无此 refno;最右叶 word6 457→453;字节 diff 仅 page0 指针。**三 slice 自检 demo 全 PASS**、无 lint、temp 已清。
- 文档:格式规范 §12.6 增 Slice 3 + word6/读取器注;findings §15.5/§15.6。

## 2026-06-07(续3)— 写侧 Slice 4:元素删除(CRUD 闭环,已实现 + 验证)
- 用户"按推荐继续"= 写侧 Slice 4(删除,补全 CRUD)。新增 `cow_delete_element`:从 B 树移除元素**主记录叶项**(`find_leaf_path` 按 refno+`is_main` 定位)→ 叶内**左移压实** + `word6+=4` → COW 路径到根 → 新会话。数据页保留(旧会话仍解析=多版本);不做下溢合并(PDMS 容忍欠满)。
- **CRUD 闭环 demo**(sam7200 副本):创建临时元素 → 删除;跨 3 会话 `原始=无 / 插入后=有 / 删除后=无`;最右叶空闲字 **457→453→457 完美 round-trip**;字节 diff 仅 page0 指针。**四 slice 自检 demo 全 PASS**、无 lint、temp 已清。
- 文档:格式规范 §12.6 增 Slice 4 + CRUD 闭环注;findings §15.6/§15.7。

### 状态:E3D 离线写侧 CRUD 打通
- 写侧:在位定长写(§12.1–12.3)+ **COW 提交全 CRUD**:Slice 1 改内联值 / Slice 2 改变长 DA 文本 / Slice 3 新增元素(B 树键插入)/ Slice 4 删除元素,均 §12.6 已实现 + 多版本验证;开库契约 IDA 确证(对真实 E3D 机制完备)。
- 读侧早已全闭环;**读 + 写(CRUD)纯离线均打通**(写侧未做真机 round-trip / 仍有 B 树分裂等遗留)。
- **下一步(写侧后续,均非阻塞)**:① B 树**节点分裂/合并**(最右叶满 insert / 删后下溢)+ 中间键插入 ② 跨页/链式 DA + 成员重排(多页 COW)③ UDA 容器改写 ④ 真 running-E3D round-trip 取证(需你侧 E3D)⑤ 移植写侧进 Rust `src/e3d_decode.rs`(并把 `walk_index` 改 word6 界定)。

## 2026-06-07(续4)— 写侧 Slice 5:任意键 B 树插入 + 节点分裂/长高(已实现 + 验证 + 文档收口)
- 按"按推荐继续下一步"= 写侧下一 slice ①(B 树节点分裂 + 中间键插入)。承接上轮已写入的 Slice 5 代码,本轮**验证 + 收口文档 + 清理**。
- **实现**(`e3d_write_full.py`):`_btree_insert`(递归插入 + 溢出对半切 + 分隔键=上半最小键)、`cow_insert_leaf`(根溢出长高新根 + 哨兵 `0x80000001`→旧根)、`cow_insert_element_split`(Slice 5 元素插入,推广 Slice 3 的"仅最右最大键"为任意键 + 分裂)。复刻 `db3_change_table_entry`(3.3.1)/`db3_split_node`(3.2.6)/`db3_split_root`(3.2.7)。新增校验器 `_btree_check`(`nav_ok`+balanced+sorted+无重复)/`_btree_descend`/`_idx_entries`/`_emit_index_page`/`_key_le`。
- **关键认知收口(分隔键宽松约定)**:正确性判据是 **`nav_ok`(PDMS 二分下降能到每个键)**,**非** "分隔键==子树最小"。诊断发现**原始 sam7200 树**本就有宽松分隔键(`pg3264 entry39 sep=0x3D08 < 子树最小 0x3D75`)——PDMS 删除不收紧分隔键仍合法。`_btree_check` 据此校验。临时诊断 `_btree_probe.py` 用后已删。
- **节点合并(删后下溢)刻意不做**:PDMS 容忍欠满,`cow_delete_element` 删除已正确 ⇒ 与真实 db3 一致,非缺口。
- **验证**(`python "docs/e3d 数据库分析/e3d_write_full.py"`,**七 demo S1–S5c 全 PASS**,~26s):
  - **5a 合成**(cap=3、60 随机键):递归分裂 + 根长高 **4 次**、`height=4`/`leaf_pages=27`/`entries=60`、balanced/sorted/nav_ok/无重复/全键齐。
  - **5b 真实 sam7200**(120 最大键→溢出最右叶):**真实叶分裂** `leaf_pages 166→167`、`entries 10392+120=10512`、原条目全留 + 新键全在、balanced/nav_ok/sorted、前会话不变、diff 仅 page0。
  - **5c 真实 sam7200**(中间键 `0x158F<max 0x3D8E`):新会话解出新元素(WELD/owner 同源/`/MID-INSERT-COW`)、旧会话无、balanced/nav_ok/sorted、diff 仅 page0。
- `py_compile` OK、无 lint、无遗留 temp(demo 自清 `_e3d_write_full_*.bin`;`_btree_probe.py` 已删)。文档:格式规范 **§12.6**(Slice 5)、findings **§16**。

### 状态:E3D 离线写侧 = CRUD + B 树分裂/长高 全打通
- 写侧:在位定长写(§12.1–12.3)+ **COW 提交 CRUD + B 树分裂**:S1 内联值 / S2 变长 DA 文本 / S3 新增(最大键无分裂)/ S4 删除 / **S5 任意键插入 + 节点分裂/长高**,均 §12.6 实现 + 多版本验证;读侧早已全闭环。
- **剩余遗留(均非阻塞)**:② 跨页/链式 DA + 成员重排(多页 COW)③ UDA 容器改写 ④ 真 running-E3D round-trip 取证(需你侧 E3D)⑤ 移植写侧进 Rust `src/e3d_decode.rs`(并把 `walk_index` 改 word6 界定)。节点**合并**刻意不做(对齐 PDMS 容忍欠满)。

## 2026-06-07(续5)— 写侧 Slice 6:跨页/链式 DA 文本改写(多页 COW relocation,已实现 + 验证 + 文档)
- 按"按推荐继续下一步"= 写侧遗留②(跨页/链式 DA)。本轮**新实现 + 验证 + 文档**(从零起,非承接已写代码)。
- **设计**:推广 Slice 2(同页/单节点/受边界约束)为 **DA 整体重定位**,统一覆盖三类:DA 已在别页 / DA 本就跨页链式 / 改写超出同页空闲(增长)。依据:reader `decode_da_list` 沿 `w3/w4` 走链**先拼接全部载荷再解析** ⇒ relocation 可任意分块、单/多节点均可读回。
- **实现**(`e3d_write_full.py`):`_da_payload_words`(链式拼完整载荷,镜像 reader)→ `_replace_text_in_payload`(扁平载荷按 hash 定位文本条目替换)→ `_emit_da_chain`(新载荷重写为全新节点链,单节点或按 `force_chunk`/页容量 `PW−12` 分块多节点,节点间 `w3=下一页`/`w4=off<<13`,节点在 off=7 复用 7 字页头)→ 改写记录 `rec[6]/rec[7]/rec[10]`(保留成员位)→ 复用 `_commit_edited_data_page`(COW 记录页 + B 树路径 + 新会话)。新增 `cow_commit_da_text_xpage` + `_da_node_chain_len`。
- **关键正确性**:① 专用 DA 页机制合法——reader 抵达 DA 只经 `rec[6]`+节点头、不校验 DA 页头,且 DA 页绝不被 B 树遍历(不会误判索引/记录页);新页完整页型保真留待真机 round-trip。② 重定位后 `rec[6]≠旧数据页`,提交核心"自包含修正"对 `rec[6]` 自动 no-op(保住跨页指针),`rec[8]` 成员仍正确改指新记录页。
- **验证**(`python "docs/e3d 数据库分析/e3d_write_full.py"`,**九 demo S1–S6b 全 PASS**,~36s):
  - **6a 跨页重定位**(`/WB1`→`/WB1-XPAGE-COW`):DA 载荷 38→41 字、**DA 页 3384 ≠ 记录页 3385(cross-page=True)**、单节点;新会话新名+POS 不变 / 旧会话旧名 / diff 仅 page0。
  - **6b 强制多节点链**(`force_chunk=4`,`/WB1`→`/WB1-CHAINED-DA`):41 字拆成 **11 节点跨 11 页链**,走链实测 11;**链式 reader 仍读回新名**(写侧分块链 ↔ 读侧链走一致)、POS 不变、旧会话旧名、diff 仅 page0。
- `py_compile` OK、无 lint、demo 自清(无遗留 temp)。文档:格式规范 **§12.6**(Slice 6)、findings **§17**、模块 docstring SCOPE 更新。

### 状态:E3D 离线写侧 = CRUD + B 树分裂/长高 + 跨页/链式 DA 全打通
- 写侧:在位定长写(§12.1–12.3)+ **COW 提交**:S1 内联值 / S2 同页变长 DA / S3 新增 / S4 删除 / S5 任意键插入+分裂/长高 / **S6 跨页·链式·增长 DA 文本重定位**,均 §12.6 实现 + 多版本验证;读侧早已全闭环。
- **剩余遗留(均非阻塞)**:② 的**成员区(member word8 链)重排**(与 DA 同构,可复用 relocation,type=2)③ UDA 容器改写 ④ 真 running-E3D round-trip 取证(需你侧 E3D)⑤ 移植写侧进 Rust `src/e3d_decode.rs`(并把 `walk_index` 改 word6 界定)。

## 2026-06-07(续6)— 写侧 Slice 7:成员(子 refno)列表改写(type-2 节点链,复用 S6 重定位)
- 按"按推荐继续下一步"= 写侧遗留②剩余的**成员区重排**。本轮 RE + 实现 + 验证 + 文档。
- **RE(真实数据探针,只读,已删 `_member_probe.py`)**:成员区与 DA **同构**——type-2 节点链,`rec[8]`=页/`off=(rec[9]>>13)&0xFFF`/成员字数=`rec[10]&0x3FFF`;节点 `[w0=(载荷+5)|(2<<16)][w1..2=本元素 refno][w3..4=链接][载荷=扁平(r0,r1)子 refno 数组]`(字数=2×子数)。探针:10392 主记录 **2864 带成员**(30 个本就跨页);**子元素 owner==本元素(8/8)** ⇒ 载荷确为子 refno 列表。
- **实现**(`e3d_write_full.py`,大量复用 S6):泛化 `_da_payload_words→_list_payload_words(which)`、`_emit_da_chain→_emit_node_chain(list_type,refno)`(节点 w1..2 现置元素 refno,DA 端回填更忠实、reader 不读故 S6 不受影响);新增 `cow_members_set(buf,db,record_bo,children)`(重写 type-2 链→改 `rec[8]/rec[9]`+`rec[10]` 成员低14位→复用 `_commit_edited_data_page`;统一覆盖重定位/加子/删子)+ 读侧 `read_members`。
- **验证**(`python "docs/e3d 数据库分析/e3d_write_full.py"`,**十一 demo S1–S7b 全 PASS**,~28s):
  - **7a 重定位+加子**(WORL 8 子,`force_chunk=2`):成员字数 16→18、**成员页 3384≠记录页 3393(cross-page=True)**、**9 节点链**;新会话=原 8+`(0xABCD,0x1234)`、旧会话 8 子、diff 仅 page0。
  - **7b 增删 round-trip**:跨 3 会话 `8→9(含 marker)→8`、成员字数 `16→18→16` round-trip、diff 仅 page0。
- `py_compile` OK、无 lint、demo 自清、临时探针已删。文档:格式规范 **§12.6**(Slice 7)、findings **§18**、模块 docstring SCOPE。

### 状态:E3D 离线写侧 = CRUD + B 树分裂/长高 + 跨页/链式 DA + 成员列表 全打通
- 写侧:在位定长写(§12.1–12.3)+ **COW 提交**:S1 内联值 / S2 同页变长 DA / S3 新增 / S4 删除 / S5 任意键插入+分裂/长高 / S6 跨页·链式·增长 DA 重定位 / **S7 成员(子 refno)列表重定位·增删**,均 §12.6 实现 + 多版本验证;读侧早已全闭环。
- **剩余遗留(均非阻塞)**:③ UDA 容器改写(强类型值/派生表达式 §7.10,载荷亦节点化,可再复用 relocation)④ 真 running-E3D round-trip 取证(需你侧 E3D)⑤ 移植写侧进 Rust `src/e3d_decode.rs`(并把 `walk_index` 改 word6 界定)。

## 2026-06-07(续7)— 写侧 Slice 8:UDA / DA 区条目值改写(通用 set/add/remove,复用 S6 重定位)
- 按"按推荐继续下一步"= 写侧遗留③ UDA 容器改写。本轮 RE + 实现 + 验证 + 文档。
- **认知**:UDA 值即 **DA 区普通条目**(hash > `0x171FAD39`=UDA_THRESHOLD,`PDMS_Hash::IsUDA`),框架同任何 DA 条目 `[hash][ctrl=type<<26|n][value]` ⇒ "UDA 容器改写"= DA 区按 hash 改/增/删条目,直接复用 S6 重定位核心。
- **RE 探针(只读,已删 `_uda_probe2.py`)**:sam7200 58 主记录带强类型 UDA;ctrl 类型 `{6:14,10:49,4:48,2:1}`(另 type-7=派生表达式)。值编码:type4=ref `(db,seq)` 2 字、type10=text、type2=real 2 字 IEEE 低字在前、type6=多字结构。元素 HANG(0x5C20,0x1D2A) 同带 ref(0x2C00D55A)+text(0x2C00D566)UDA。
- **实现**(`e3d_write_full.py`):抽出共享 `_relocate_da_payload`(S6/S8 共用);新增 `read_uda`(链式取 hash>阈值条目)、`_set_entry_in_payload`/`_remove_entry_in_payload`(扁平载荷按 hash 改/增/删,余条目逐字保留)、`cow_da_set_entry(record_bo,attr_hash,type_code,value_words)`/`cow_da_remove_entry`(通用,UDA 为 motivating use)+ `read_uda_via_root`/`_find_uda_element`。透明继承 S6 跨页/增长。
- **验证**(`python "docs/e3d 数据库分析/e3d_write_full.py"`,**十三 demo S1–S8b 全 PASS**,~24s):
  - **8a 改强类型 UDA 值**(HANG type-4 ref `0x2C00D55A` `(15195,2418)→(99,12345)`):新会话该 UDA=新值、**同元素另一 text UDA `0x2C00D566` 逐字保留**、旧会话原值、is_uda=True、diff 仅 page0。
  - **8b UDA 增删 round-trip**(`/WB1` 加合成 `0x2C00FFFF`=type4 `(0xAAAA,0xBBBB)` 再删):跨 3 会话 `0→1→0` UDA、diff 仅 page0。
- `py_compile` OK、无 lint、demo 自清、临时探针已删。文档:格式规范 **§12.6**(Slice 8)、findings **§19**、模块 docstring SCOPE。

### 状态:E3D 离线写侧 = CRUD + B 树分裂/长高 + 跨页/链式 DA + 成员列表 + UDA/DA 条目 全打通
- 写侧:在位定长写(§12.1–12.3)+ **COW 提交**:S1 内联值 / S2 同页变长 DA / S3 新增 / S4 删除 / S5 任意键插入+分裂/长高 / S6 跨页·链式·增长 DA 重定位 / S7 成员(子 refno)列表重定位·增删 / **S8 UDA·DA 区条目值改/增/删**,均 §12.6 实现 + 多版本验证;读侧早已全闭环。**13 自检 demo 全 PASS**。
- **剩余遗留(均非阻塞)**:④ 真 running-E3D round-trip 取证(需你侧 E3D)⑤ 移植写侧进 Rust `src/e3d_decode.rs`(并把 `walk_index` 改 word6 界定);`0xFFF` 族表达式 UDA 的 AST 级语义编辑(原始字重写可,语义编辑不在范围)。

## 2026-06-07(续8)— 写侧 Slice 9:Rust 移植起步(COW 提交核心 + S1 内联值,`src/e3d_decode.rs`)
- 按"按推荐继续下一步"= 遗留⑤ 移植写侧进 Rust。本轮落地**基础层**(COW 核心 + S1),并确认 `walk` 早已 word6 界定。
- **先勘查**:`src/e3d_decode.rs`(697 行)此前 = 读(全属性/NAME/引用)+ 安全在位 `set_inline_value`,**无 COW**;`Edb` 持有 `Vec<u8>`(利于 COW append);`walk` 条目数 `=(pw−7−word6)/4` **本就 word6 界定**(测试断言 10392)⇒ 遗留⑤的"walk_index 改 word6"在 Rust/Python 两端均已完成(注记此前为 stale)。
- **移植**(对照 `e3d_write_full.py`):新增 `commit_edited_data_page`(共享核心:追加改后数据页→自包含修正 rec[6]/rec[8]→自叶到根 COW B 树路径 `find_leaf_path_by_loc`→追加新会话 sesno+1→重指 page0 0x28)、`cow_commit_inline`(S1:复制数据页→`set_inline_value`→提交核心)、`session_roots`/`record_off_via_root`(多版本读回)、`Edb::{latest_root,append_page,n_pages}`。
- **验证**(edition-2024 临时 `_modcheck` crate `cargo test`,rustc 1.98-nightly,**6 passed** 无 warning,crate 已删):既有 5 测试续过 + 新增 `cow_inline_commit_multiversion`——`/WB1` 连提两次 POS,会话根链读回 **新=v2 / 前=v1 / 原始=原值(9630,8072,5282.5)**、`session_roots`+2、**原字节区仅 page0 0x28..0x2C 改动**、文件增长。与 Python S1 demo 等价。
- 全 crate `cargo build` 仍受 rs-core↔surrealdb-3.1 阻塞(项目外),故沿用临时 crate 验证。`src/e3d_decode.rs` 无 lint;`_modcheck` 已删。文档:格式规范 §12.6(Rust 移植注)、findings **§20**。

### 状态:写侧 Python 全打通(S1–S8,13 demo);Rust 移植起步(S1 COW 已移植 + 验证)
- Python 写侧:CRUD + B 树分裂/长高 + 跨页/链式 DA + 成员列表 + UDA/DA 条目(S1–S8,13 自检 demo 全 PASS)。
- Rust 写侧(`src/e3d_decode.rs`):读 + 安全在位写 + **S1 COW 提交(多版本)**;`walk` word6 界定。**6 模块测试通过**。
- **剩余遗留(均非阻塞)**:④ 真 running-E3D round-trip 取证(需你侧 E3D)⑤ Rust 端 S2–S8 移植(DA relocation/B 树插入分裂/成员/UDA,逐一对照 Python);`0xFFF` 表达式 UDA 的 AST 级语义编辑。

## 2026-06-07(续9)— 写侧 Slice 10:Rust 移植续(DA 重定位核心 + S2/S6 DA 文本 + S8 UDA)
- 按"按推荐继续下一步"= Rust 端继续移植。本轮一次覆盖 **S2(同页 DA 文本)+ S6(跨页/链式/增长)+ S8(UDA/DA 条目)**——三者共用 DA relocation 核心。
- **移植**(对照 `e3d_write_full.py`):共享 `list_payload_words(which)`(链式取原始载荷,泛化 `decode_da_list`)、`emit_node_chain(list_type,refno,force_chunk)`(新页节点链)、`relocate_da_payload`(emit + 改 rec[6]/rec[7]/rec[10]-DA 位 + 复用 §20 `commit_edited_data_page`);条目编辑 `find_entry_span`/`set_entry_in_payload`/`remove_entry_in_payload`/`pack_text`;公共 API `cow_commit_da_text`(S2/S6)、`read_uda`/`cow_da_set_entry`/`cow_da_remove_entry`(S8)。`CowReport` 增 `da_page_new`/`da_nodes`。
- **验证**(edition-2024 临时 `_modcheck` crate `cargo test`,**9 passed** 无 warning,crate 已删):§20 的 6 测试续过 + 新增 3:
  - `cow_da_text_rename_multiversion`:`/WB1`→`/WB1-RS-RENAMED`,新会话新名/旧会话旧名/**POS 不变**/**DA 跨页**/原字节仅 page0 0x28。
  - `cow_da_text_chained`:`force_chunk=4` 多节点链(链长≥2),链式 reader 读回 `/WB1-CHAIN`。
  - `cow_uda_add_remove_roundtrip`:`/WB1` 加合成 UDA `0x2C00FFFF` 再删,读回 存在→不存在。
- `src/e3d_decode.rs` 无 lint;`da_node_chain_len`(仅测试)标 `#[cfg(test)]`;`_modcheck` 已删。文档:格式规范 §12.6(Rust 移植注)、findings **§21**。

### 状态:Python 写侧 S1–S8 全打通(13 demo);Rust 移植 S1+S2/S6+S8(9 测试)
- Rust 写侧(`src/e3d_decode.rs`):读 + 安全在位写 + **S1 COW + S2/S6 DA 文本重定位 + S8 UDA/DA 条目改增删**;`walk` word6 界定。**9 模块测试通过**。
- **剩余遗留(均非阻塞)**:Rust 端 **S3/S5(B 树插入/分裂/长高)+ S7(成员 type-2)** 移植;④ 真 running-E3D round-trip 取证(需你侧 E3D);`0xFFF` 表达式 UDA 的 AST 级语义编辑。

## 2026-06-07(续10)— 写侧 Slice 11:Rust 移植完成(S3/S5 B 树插入分裂 + S7 成员列表)
- 按"按推荐继续下一步"= Rust 端最后两块写侧移植(§21.3 遗留)。本轮把 Python 的 **B+ 树键插入/节点分裂/根长高(S3/S5)+ 成员 type-2 列表改写(S7)** 移植进 `src/e3d_decode.rs`。至此 Rust 写侧与 Python `e3d_write_full.py` **S1–S8 全部对齐**。
- **移植**(对照 Python):
  - 抽出共享 `append_session`(`db5_save_work` 尾:克隆会话页+`sesno+1`+重指 page0 `0x28`),`commit_edited_data_page` 重构复用(去重)。
  - **S7**:`read_members`/`cow_members_set`(复用 §21 `emit_node_chain(list_type=2)`;改 `rec[8]/rec[9]`+`rec[10]` 成员低 14 位;空列表→页0;统一重定位/加子/删子)。
  - **S3/S5**:`idx_entries`(word6 界定+word2 层级)/`emit_index_page`/`key_le`(哨兵 −∞)/`btree_insert`(递归插+对半切+分隔键=上半最小键)/`cow_insert_leaf`(根溢出长高+哨兵 `0x80000001`→旧根)/`rightmost_path`/`clone_element_page`/`rewrite_da_text_in_page`;公共 API `cow_insert_element`(S3 最大键)+`cow_insert_element_split`(S5 任意键+分裂)。`CowReport` 增 `tree_grew`。
- **关键点**:`idx_entries` 返回 owned `Vec`(借用即放)⇒ `btree_insert` 可递归持 `&mut Edb`;内部条目 `v=1`(off=0)使既有 reader(按 off)与写侧(按 word2 level)判内部一致;校验器 `btree_check` 以 **`nav_ok`** 为正确性判据(非"分隔键==子树最小",对齐 PDMS 宽松分隔键,§16)。
- **验证**(edition-2024 临时 `_modcheck` crate `cargo test --release`,**14 passed**,无 warning,crate 已删):§21 的 9 测试续过 + 新增 5:
  - `btree_synthetic_split_grow`(无需数据,cap=3/60 乱序键):递归分裂+根长高、`height≥2`、`balanced/sorted/nav_ok/无重复/keyset 全齐`。
  - `cow_insert_split_real`(sam7200,溢出最右叶):**真实叶分裂**、新会话 `count=orig+N`、原键全留+新键全在、`balanced/nav_ok/sorted`、前会话不变、diff 仅 page0。
  - `cow_insert_mid_real`(sam7200,中间键 `<max`):新会话解出新元素(noun/owner 同源+`/MID-INSERT-RS`)、旧会话无、`count+1`、`balanced/nav_ok/sorted`、diff 仅 page0。
  - `cow_members_add_remove_roundtrip`(加子再删):跨 3 会话 `原始→+marker→原始`、diff 仅 page0。
  - `cow_members_xpage_chain`(`force_chunk=2`):成员**跨页**+ **≥2 节点 type-2 链**、新会话列出原子+marker、前会话不变。
- `src/e3d_decode.rs` 无 lint;测试辅助 `node_chain_len(which)`/`btree_descend`/`btree_check`/`find_member_element` 标 `#[cfg(test)]`;`_modcheck` 已删。文档:findings **§22**、格式规范 §12.6(Rust 移植注)。

### 状态:Rust 写侧 = Python S1–S8 全对齐(写侧 Python↔Rust 移植里程碑完成)
- Rust 写侧(`src/e3d_decode.rs`):读 + 安全在位写 + **S1 内联值 / S2·S6 DA 文本重定位 / S3·S5 元素新增(最大键+任意键分裂长高)/ S7 成员列表 / S8 UDA·DA 条目**;`walk` word6 界定。**14 模块测试通过**(edition-2024)。整 crate `cargo build` 仍受 rs-core↔surrealdb-3.1 阻塞(项目外,与本模块无关)。
- **剩余遗留(均非阻塞)**:④ 真 running-E3D round-trip 取证(需你侧 E3D);`0xFFF` 表达式 UDA 的 AST 级语义编辑。节点合并(删后下溢)刻意不做(对齐 PDMS 容忍欠满)。

## 2026-06-07(续11)— 模块读侧补齐:跨库引用解析 `resolve_refs`(读侧与独立 crate/Python 对齐)
- 按"按推荐继续下一步"= 收口 `src/e3d_decode.rs` 读侧最后一处与独立 crate `tools/e3d_decode_rs` / Python `e3d_export.py --cat` 的差距:模块缺把引用属性(隐式 type 4/8/16 = `(dbno,refseq)`)解析为目标名的 `resolve_refs`(`index_db` 早已建 refmap,只差解析步)。
- 移植自独立 crate `resolve_refs`(逐字):隐式 type 4/8/16 → 查 refmap → 命中返回目标名 / 未命中回退 `=dbno/refseq`;跨库灌同一 refmap(设计库 collect=true + catalogue collect=false)。findings §22.5。
- 验证:新增 `resolve_refs_in_db`(sam7200 仅本库,断言 ≥100 引用解析到 `/`-名,对应 Python 776 in-db);**15 模块测试通过**(edition-2024,无 warning);doctest 示例标 `ignore`。`src/e3d_decode.rs` 无 lint;`_modcheck` 已删。
- ⇒ **`src/e3d_decode.rs` = 完整离线 读(全属性/NAME/引用解析)+ 写(S1–S8)模块**,读写均与独立 crate/Python 对齐;待 rs-core↔surrealdb 就绪并入 pdms_io。

### 状态:E3D 离线读写格式分析 + Rust 模块化全部完成
- 读侧全闭环 + 写侧 CRUD/B 树/DA/成员/UDA(Python S1–S8 + Rust S1–S8)+ 引用解析,均 Python/Rust 双实现对齐并测试。
- **剩余遗留(均阻塞或显式范围外)**:④ 真 running-E3D round-trip 取证(需你侧 E3D);并入 pdms_io(待 rs-core↔surrealdb-3.1,项目外);`0xFFF` UDA AST 级语义编辑(范围外);节点合并(刻意不做)。
