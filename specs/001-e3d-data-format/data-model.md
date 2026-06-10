# Data Model: E3D / PDMS DABACON 数据文件格式

> Phase 1 输出。字节级结构定义(规范节选 + 交叉引用)。权威全文见 `docs/e3d 数据库分析/E3D_DB_文件格式规范.md`;此处为 spec-kit feature 内自洽的结构化摘要。
> 约定:**大端(big-endian)**;"字(word)" = 4 字节;偏移以字为单位时记 `w[N]`,以字节记 `0xNN`。

## 0. 文件与页模型

| 项 | 值 |
|---|---|
| 页大小 | 2048 字节(= header `0x34` 字段 × 4) |
| 字节序 | 大端 |
| 元素库页定位 | `page P @ P*2048`(page0 = PdmsHeader/db-control-block) |
| 模式库页定位 | `page P @ (P-1)*2048`(page1 = 头) |
| 链式页 | 511 数据字 + 第 512 字 = 下一页页号 |

数据库角色(4 类文件):

| # | 角色 | 文件 | 关键内容 |
|---|---|---|---|
| 1 | 元素库(设计/目录/系统) | `<proj><dbno>_0001` | 元素记录:隐式区 + DA/显式区 + 成员区 |
| 2 | 模式库 / 模板库 | `%AVEVA_DESIGN_EXE%/*vir.dat` | 每 noun 的 type-def(**offset 来源**)K + 默认 I/J |
| 3 | 属性库 | `attlib.dat` | 属性名/类型/单位/类别(读取**不**依赖,offset 不在此) |
| 4 | 名称哈希 | (算法) | `db1_hash` base-27 + `0x81BF1`(可逆 DEHASH) |

## 1. Page 0 / DB-Control-Block(头部)

整页为 db-control-block(头 + ASCII + db-block 属性)。关键字段(已厘定,§13):

| 偏移 | 字段 | 语义 |
|---|---|---|
| `0x20` | `schema_type_id` | 选定 `*vir.dat`(实测 word8 ↔ vir.dat 头 w2 一一对应) |
| `0x24` | `schema_version` | 模式版本 |
| `0x28` | `session_pointer` (w10) | 指向**当前会话页**(写提交时原子重指 = 唯一变更点) |
| `0x2C` | `w11` | 次会话指针(保守不改) |
| `0x30` | `extent_counter` | extract/extent 计数(`v26[12]=ext_no+1`) |
| `0x34` | `page_size_words` | 页大小(字)→ ×4 = 2048 |
| `0x38` | `DBNO` (refno_0) | `(db_num&0x1FFF)|(((db_num&0x3E000)|1)<<13)`;bit13–17=库类型 |
| `0x3C` | `refseq` (refno_1) | db 根/world 引用 refseq;save_work 由 db-block 属性 7 刷新 |
| `0x44+` | creation info | ASCII 创建信息 |

> 注:`src/defines.rs::PdmsHeader` 旧字段名(creation_time/session_page_no/stored_page_count)为**误称**,已在注释校正(改名跨 13+ 文件,未改名)。

## 2. 会话链(多版本)

- page0 `w10` → 当前会话页;会话页持有 `sesno`、B 树 `index_root`、`end`、`claim`(对应 db-block 条目 `+12/+28/+20/+36`)。
- 每次提交 append 新会话页(`sesno+1`、新 `index_root`),旧会话只读保留 ⇒ **多版本**;沿会话链可读任一历史版本。

## 3. B 树索引页(type5)

| 项 | 规则 |
|---|---|
| 页头 | 7 字 |
| `word2` | 层级(level) |
| `word6` | **空闲字数**(权威条目计数源) |
| 条目数 | `(2048/4 - 7 - word6)/4`(**word6 界定**,非空终止) |
| 条目 | 自 word7 升序;每条 `[key=refno, child/loc...]` |
| 最左子树哨兵 | `word7 == 0x80000001`(−∞ 分隔键) |

- 遍历:沿会话 `index_root` 二分下降到叶,叶项 → `(pgno, offset)` → `byte_off = pgno*2048 + offset*4`。
- **有效性过滤**(剔除脏/误报):要求 `(word0>>16)==0` 且 `8 ≤ count ≤ 512`;noun 可 dehash。
- **遍历上界** ≥ 4,000,000(防大库截断;ams1112 ~42.2 万元素)。

## 4. 元素记录(隐式区)

记录头关键字:

| 字 | 语义 |
|---|---|
| `w0` | (u16) 隐式字数 = 记录字数(如 WELD=46) |
| `w3` | noun hash(base-27) |
| `w4..w5` | owner refno `(dbno, refseq)` |
| `w6` | DA/显式页指针(page_no) |
| `w7` | DA 偏移:`(w7>>13)&0xFFF` |
| `w8..w9` | 成员页 / 偏移 |
| `w10` | `sel=bit29` + DA 字数(`>>14 & 0x3FFF`) + 成员字数(`& 0x3FFF`) |
| 隐式值区 | 按 type-def 描述符 `off` 索引 |

取值规则见 `research.md` D2(sel / 标量 vs 计数前缀 / Bool 位 / 实数低字在前)。

## 5. 模式库 `*vir.dat`(type-def = offset 来源)

**头(page1)**:`w0==6`(魔数)、`w5`=类型数、`w7`=类型索引(tlu)页。

**tlu(类型查找)**:每项 7 字 `[noun, Kpg, Kcnt, Ipg, Icnt, Jpg, Jcnt]`,按 noun **升序**(二分)。

**skeleton K(type-def,布局/offset)**:`w9`=描述符数、`w14`=描述符数组起。

**描述符(每条)**:

| idx | 字段 | 说明 |
|---|---|---|
| `[0]` | hash | 属性 hash |
| `[1]` | stride | 步长 |
| `[2]` | type | 类型枚举(见 §6) |
| `[3]` | size | 分量数 |
| `[5]` | 主 offset \| bit | `off=&0xFFFFF`,BOOL `bit=>>20`(sel=1/unpacked) |
| `[8]` | 备 offset \| bit | packed(sel=0) |

**skeleton I/J(默认值)**:I = 按 offset 的默认记录镜像(`default=I[off-11]`);J = 按 hash 的默认表 `[hash, sizeword, value]`(`type=sizeword>>26`、`count=&0x3FFFFFF`、步长 count+2)。

## 6. type 枚举(type-def `desc[2]`,全库实测 + db4 反编译)

| type | 含义 | 宽度/取值 |
|---|---|---|
| 2 / 6 | 实数 标量 / 向量 | sel=1 双精度 2 字(低字在前) |
| 3 / 7 | 整数 标量 / 数组 | 1 字 |
| 4 / 8 / 16 | 引用(word-ref) | `(dbno, refseq)` 2 字 |
| 5 | 布尔 | 位 `(record[off]>>bit)&1` |
| 10 / 15 | 文本 / NAME 文本 | `[len][packed]`;NAME hash=`0x9C18E` off=0 |
| 14 / 18 | UDA 表(UDATAB/UDAFTB / UDASTB) | 特殊表(off=0) |
| 9 / 17 | 特殊 | — |
| 19 | 未用 | — |

## 7. DA / 显式属性区(节点链)

- 定位:`record[6]`=page、`record[7]`→`(>>13)&0xFFF` 偏移、`record[10]>>14&0x3FFF`=DA 字数。
- 节点:5 字头 `[w0=(payload+5)|(type<<16)][w1..2][w3=下一页][w4=下一页内偏移]`;payload 自 +5 字。
- 跨页链式:顺 `w3/w4` 走链,**先拼接全部 payload 再解析**(单/多节点均可读回)。
- 条目格式:`[hash][ctrl: type<<26 | wordcount][value]`,步进 `wordcount+2`。
- 文本:`[byte_len][packed 4 字符/字,高字节在前]`,UTF-8。
- **NAME**:DA 区 hash `0x9C18E`、type 15。

## 8. 成员区(type-2 节点链)

- 与 DA **同构**:`record[8]`=页、`off=(record[9]>>13)&0xFFF`、成员字数=`record[10]&0x3FFF`。
- 节点 `[w0=(payload+5)|(2<<16)][w1..2=本元素 refno][w3..4=链接][payload=扁平 (dbno,refseq) 子 refno 数组]`(字数=2×子数)。
- 子元素 owner == 本元素(与 owner 链冗余,用于校验)。

## 9. 引用(Reference)

- 值 = 2 字 `(dbno, refseq)`(首字 = dbno)。
- 本设计库 dbno → 可解析到本库名;catalogue 库 dbno(外部)→ 需对应文件;未加载库以 `=dbno/refseq` 原样。
- 连接类(CREF/HREF/TREF)指本设计库;规格/材料类(SPRE/MATR/ISPE)指 catalogue 库。

## 10. UDA(用户自定义属性)

- 判定:`hash > 0x171FAD39`(`PDMS_Hash::IsUDA`);UDA 值 = DA 区普通条目(带声明类型)。
- 强类型值:type4=ref `(db,seq)`、type10=text、type2/6=real(低字在前)、type3=int → 纯离线可解。
- `0xFFF` 族 = 序列化 PDMS 表达式(派生属性),封装 `[len][0][count][sublen][1]<RPN>`,opcode:`0x6A`=属性引用、`0x6B/6C/6D/73`=DORTXT 几何字面量、`0x65`=数值、`80x/100x`=运算符 → 复用 `expression_payload.rs` 解(98.8%)。
- **UDA 真名**:离线不可得(需 udalib 字典库,见 research D4)。

## 11. 写 / COW 模型(切片 S1–S8)

提交核心:改后数据页 → 自包含修正 `rec[6]`(DA 页)/`rec[8]`(成员页)→ 自叶到根 COW B 树路径 → append 新会话(`sesno+1`)→ 重指 page0 `0x28`。

| 切片 | 操作 | 要点 |
|---|---|---|
| S1 | 改内联值 | 定长就地(real/int/ref) |
| S2 | 同页变长 DA 文本 | 受同页空闲约束 |
| S3 | 新增(最大键) | 最右路径追加,不改分隔键 |
| S4 | 删除 | 叶内左移压实 + `word6+=4`;不下溢合并 |
| S5 | 任意键插入 + 分裂/长高 | 递归对半切,分隔键=上半最小键,根溢出长高 |
| S6 | 跨页/链式/增长 DA | 整体重定位到新节点链 |
| S7 | 成员列表 | type-2 链重定位/增删 |
| S8 | UDA/DA 条目改增删 | 扁平 payload 按 hash 改/增/删,余条目逐字保留 |

**不变式**:提交后"新会话=新值 / 旧会话=原值 / 字节 diff 仅 page0 会话指针";B 树 `nav_ok` + 平衡 + 有序 + 无重复。详见 `contracts/decode-contract.md`。

## 12. 事务批量 / 安全编辑(US5,写之上)

> 不引入新字节结构——事务 = 把多笔 §11 切片合并到**单**会话提交;以下为 API 行为模型。

- **批量提交**:N 笔编辑在同一"工作根"上逐笔 COW 串联,多数据页 / 多 B 树路径合并到**同一新根**,`append_session` 仅末尾调用一次 ⇒ `sesno` 仅 +1。互不相交记录直接合并;相交路径(同页多记录)需合并 COW。
- **写后自校验 `verify_commit`**:① B 树不变式(`nav_ok`/平衡/有序/无重复)② COW 不可变(原页字节仅 `0x28` 变)③ 目标元素读回 == 预期 ④ 引用完整性(不悬挂)。校验器从 test-only 提升为库内 API。
- **dry-run / diff**:副本应用批量但不写盘,old/new 解码对比 ⇒ `{added, removed, modified:[{refno,attr,old,new}]}`。
- **安全护栏**:默认拒绝删有成员的父 / 悬挂引用 / 重复 refno;`--force` 分级;`--inplace` 二次确认 + 落盘前 `verify_commit` 通过。
