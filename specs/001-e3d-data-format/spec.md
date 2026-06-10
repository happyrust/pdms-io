# Feature Specification: E3D / PDMS DABACON 数据格式离线读写规范

**Feature Branch**: `001-e3d-data-format`

**Created**: 2026-06-08

**Status**: Draft (基于已闭环的逆向成果整理为规范;详见 plan.md / data-model.md / research.md)

**Input**: User description: "使用 spec-kit 继续编写对数据格式的分析"

> 说明:本规范把前序 `.planning/2026-06-05-e3d-db-offline-attr-parser`、`2026-06-07-e3d-offline-rw-productionization` 已**逆清并双实现验证**的成果,固化为 spec-kit 结构下的需求规范。"WHAT/WHY"在本文件;字节级"HOW"在 `data-model.md`,逆向依据在 `research.md`。

## User Scenarios & Testing *(mandatory)*

### User Story 1 - 离线读取元素全部属性 (Priority: P1)

工具/集成作者拿到一份 E3D 设计库或目录库文件(无运行中的 E3D、无 AVEVA 安装),希望**纯文件离线**解出任意元素的完整属性:`noun`(类型)、`NAME`、`refno`、`owner`、全部隐式属性、全部显式(DA)属性、UDA(用户自定义属性)、以及引用属性的目标。

**Why this priority**: 读是写、导出、模型重建的基础;没有可信的读,后续一切无从谈起。这是最小可用价值(MVP)。

**Independent Test**: 仅凭一份 `sam7200_0001` 文件 + 模式库 `desvir.dat`,对某个 WELD 元素解出 `POS=(9630,8072,5282.5)`、`NAME=/WB1` 及其属性集,与结构化实测逐字吻合即通过。

**Acceptance Scenarios**:

1. **Given** 一份设计库文件 + 对应模式库目录, **When** 指定一个元素 refno/NAME, **Then** 返回其 noun、NAME、owner、全部隐式属性(含 POS/ORI)及类型与值。
2. **Given** 同一元素含 DA/显式属性与引用, **When** 解码, **Then** 文本属性(NAME/DESC)按 UTF-8 正确还原(含中文名),引用属性给出 `(dbno, refseq)` 并可解析到目标名。
3. **Given** 元素含强类型 UDA 与 `0xFFF` 派生表达式 UDA, **When** 解码, **Then** 强类型值(real/int/text/ref)直接给出,派生表达式还原为可读 PDMS 表达式。

---

### User Story 2 - 离线非破坏写入(CRUD) (Priority: P2)

编辑工具作者希望在**不破坏原文件**的前提下,对库副本做改值 / 改名 / 新增 / 删除 / 改成员 / 改 UDA,并以"一次保存"的语义提交,且旧版本仍可读。

**Why this priority**: 把"只读分析"升级为"可编辑数据源",是接入真实工程数据流(写回 E3D)的前置;但依赖 P1 的读作为校验 oracle。

**Independent Test**: 对 `sam7200_0001` 副本改 `/WB1` 的 POS 并提交,沿会话链读回:新会话=新值、旧会话=原值,且整文件字节 diff 仅落在 page0 会话指针处。

**Acceptance Scenarios**:

1. **Given** 库副本, **When** 改某元素内联值并提交, **Then** 新会话读回新值、前序会话读回原值、原有页逐字节不变(仅 page0 指针变)。
2. **Given** 库副本, **When** 新增(克隆)一个元素后再删除, **Then** 跨三会话呈现 `不存在 → 存在 → 不存在`,B 树保持可二分下降(nav_ok)、平衡、有序。
3. **Given** 库副本, **When** 改写变长 DA 文本/成员列表/UDA 条目(可能跨页、增长), **Then** 读回新内容,同元素其它属性逐字保留,前序会话不变。

---

### User Story 3 - 整库导出与模型重建 (Priority: P3)

数据集成者希望把整库枚举导出为结构化 JSON,重建 owner 层级树,并解析跨库(设计↔目录)引用,以便接入下游分析/可视化。

**Why this priority**: 规模化交付价值(整库而非单元素),但建立在 P1 单元素解码之上。

**Independent Test**: 对 `sam7200_0001 --cat acp7002_0001` 导出 JSON,元素计数稳定、根为 ZONE、`EQUI /P1501A` 下挂几何与 NOZZ,跨库引用解析到目录名。

**Acceptance Scenarios**:

1. **Given** 一份设计库, **When** 整库导出, **Then** 产出 `{元素, named, noun_types, refmap}` 的合法 JSON,计数在双实现间一致。
2. **Given** 设计库 + 目录库, **When** 启用跨库引用解析, **Then** 设计库中指向目录库的引用解析为目标名(如 `.SPRE → SPCO …`),未加载的库以 `=dbno/refseq` 原样保留。
3. **Given** 导出 JSON, **When** 按 owner 重建树, **Then** 层级语义合理(ZONE→STRU/FRMW→SCTN→几何)。

---

### User Story 4 - 双实现对齐与可复用 (Priority: P3)

平台开发者希望 E3D I/O 是一个 std-only、可独立 `cargo test`、可被 `pdms_io` 复用的 crate,且与 Python 参考实现逐项对齐,作为格式正确性的持续保障。

**Why this priority**: 保障长期正确性与可集成性,非单次交付;依赖 P1–P3 的能力存在。

**Independent Test**: `cd crates/e3d_io && cargo test` 全绿;Rust↔Python 对同一库做属性级 diff,目录库 100%、设计库隐式 0 mismatch。

**Acceptance Scenarios**:

1. **Given** `crates/e3d_io`, **When** 运行测试, **Then** 全部用例通过且无第三方依赖。
2. **Given** 同一真实库, **When** 双实现解码并 diff, **Then** 满足对齐基线;任何差异可定位到具体元素与属性。

---

### User Story 5 - 安全的事务化离线编辑 (Priority: P2)

编辑工具/集成作者希望把若干待定改动作为**一次"保存"**原子提交(合成单新会话):提交前可 **dry-run 预览**(元素级 diff)、提交后**自动自校验**(B 树不变式 / COW 不可变 / 读回符合预期 / 引用不悬挂),并由**安全护栏**拦截危险编辑——把离线写从"单笔原语"升级为**生产可用**的事务编辑层。

**Why this priority**: 与 US2 同为 P2——US2 提供写原语,US5 把这些原语"生产化"为可安全用于真实工程数据的事务层;直接建立在 US2 之上(对应 active plan `2026-06-07-e3d-offline-edit-safety-batch`)。

**Independent Test**: 对 `sam7200_0001` 副本,把 rename+set_pos+insert 三笔编辑作为一个批次提交:`sesno` 仅 +1、三者均读回生效、前序会话逐字节不变、`verify_commit` 通过;再对一个故意构造的坏提交(破坏 B 树 / 改动无关元素),verify 必报出对应 Issue。

**Acceptance Scenarios**:

1. **Given** 库副本与 ≥3 笔混合编辑(rename+set_pos+insert), **When** 作为一个事务批量提交, **Then** 合成**单**新会话(`sesno` 仅 +1)、全部生效、前序会话逐字节不变、`verify_commit` 通过。
2. **Given** 一批待定编辑, **When** 执行 dry-run, **Then** 在**不写盘**前提下产出元素级 diff `{added, removed, modified:[{refno,attr,old,new}]}`,且与真实提交后的解码差异逐项一致。
3. **Given** 危险编辑(删有成员的父 / 制造悬挂引用 / 重复 refno), **When** 默认提交, **Then** 被安全护栏拒绝;`--force` 分级放行、`--inplace` 须二次确认且落盘前 `verify_commit` 通过才落盘。

---

### Edge Cases

- **短 skeleton-K(<10 字)**:模式库某些类型 type-def 过短,解码 MUST 守卫返回"无 schema"而非越界崩溃(amssys 触发过)。
- **脏 / 误报索引条目**:B 树叶 `word6` 之后的脏字、`looks_like_noun` 误报(乱码 noun)MUST 被有效性过滤剔除,不计入元素。
- **CJK 元素名**:文本 MUST 按 UTF-8 解码(`/穹顶`、`/天花板`),不得用 latin1。
- **`sel=0` / packed 记录**:干净设计/目录数据中**不出现**(已定论);packed 路径按代码+合成验证保留,但不据非干净样本"声称"。
- **多节点 DA / 成员链 + 跨页重定位**:读 MUST 顺 `word3/word4` 链先拼接全部载荷再解析;写增长超出同页空闲 MUST 走整体重定位到新页链。
- **大库遍历**:B 树遍历 MUST 以 `word6`(空闲字数)界定条目数,且遍历上界足够大(≥4,000,000)以免截断(ams1112 ~42.2 万元素)。
- **批量多路径 COW 合并**:一个事务内多笔编辑触及同页多记录 / 相交 B 树路径时,MUST 在同一"工作根"上逐笔 COW 串联、合并到**单**新根(append 新会话仅一次);互不相交记录可直接合并,相交路径需合并 COW(防止"每笔一会话"或新根分叉)。

## Requirements *(mandatory)*

### Functional Requirements

**读(P1)**
- **FR-001**: 系统 MUST 解析元素库头部(page0 / db-control-block):页大小、DBNO、refseq、schema_type_id、schema_version、extent 计数、会话指针。
- **FR-002**: 系统 MUST 沿会话链定位当前 B 树索引根,并枚举 refno→记录偏移(以 `word6` 界定条目)。
- **FR-003**: 系统 MUST 依 `record[3]`(noun hash, base-27)在模式库 `*vir.dat` 二分定位 type-def(skeleton K),据描述符 `[hash,stride,type,size,off|bit]` 解出全部隐式属性。
- **FR-004**: 系统 MUST 正确实现取值规则:`sel=(record[w10]>>29)&1` 选主/备 offset;标量(size==1 非文本)直存 `record[off]`,否则计数前缀;Bool 取位;实数 sel=1 双精度(低字在前)。
- **FR-005**: 系统 MUST 解析显式/DA 区(`record[6/7/10]` 定位 + 5 字头节点链),含文本 `[len][packed]`、`NAME=0x9C18E`,并支持跨页链式遍历。
- **FR-006**: 系统 MUST 从记录头 `word4–5` 取 owner refno;引用属性(type 4/8/16)MUST 解释为 `(dbno, refseq)`。
- **FR-007**: 系统 MUST 提取 UDA(hash > `0x171FAD39`)强类型值(real/int/text/ref);MUST 将 `0xFFF` 族派生 UDA 还原为可读 PDMS 表达式(标量+几何字面量,目标解码率 ≥95%)。
- **FR-008**: 文本解码 MUST 使用 UTF-8。

**写(P2)**
- **FR-009**: 系统 MUST 提供 COW + 新会话提交(`db5_save_work` 语义):改动追加文件尾、仅原子重指 page0 会话指针;原有页字节不可变。
- **FR-010**: 系统 MUST 支持全 CRUD 原语:改内联值(S1)、改变长 DA 文本含跨页/链式/增长(S2/S6)、新增元素(最大键 S3 / 任意键+节点分裂/根长高 S5)、删除(S4)、成员列表(S7)、UDA·DA 条目改增删(S8)。
- **FR-011**: B 树写入正确性判据 MUST 为 `nav_ok`(二分可达每个键)+ 平衡 + 有序 + 无重复;节点合并(删后下溢)按 PDMS 容忍欠满,刻意不做。
- **FR-012**: 写操作 MUST 默认仅作用于副本;`--inplace` 须显式开关。
- **FR-013**: 每次提交后系统 MUST 可验证"新会话=新值 / 旧会话=原值 / 字节 diff 仅 page0 会话指针"。

**导出/集成(P3)**
- **FR-014**: 系统 MUST 支持整库枚举导出为 JSON(refno/noun/name/owner/implicit/explicit/refs)。
- **FR-015**: 系统 MUST 支持跨库引用解析(加载目录库构建全局 refmap;未命中库以 `=dbno/refseq` 原样保留)。
- **FR-016**: 系统 MUST 支持由 owner refno 重建层级树。
- **FR-017**: 核心 MUST 为 std-only,可独立 `cargo test`,并经 `pub use` 被 `pdms_io` 复用。
- **FR-018**: 系统 MUST 维持 Python↔Rust 属性级对齐(目录库 100% / 设计库隐式 0 mismatch),差异可定位。

**安全事务编辑(P2,US5)**
- **FR-019**: 系统 MUST 提供写后自校验 `verify_commit`:① B 树不变式(`nav_ok` / 平衡 / 有序 / 无重复,校验器从 test-only 提升为库内 API)② COW 不可变(原页字节仅 page0 会话指针变)③ 目标元素读回 == 预期 ④ 引用完整性(新增/改动的引用 `(dbno,refseq)` 在本库或已声明 catalogue 内可解,不悬挂)。
- **FR-020**: 系统 MUST 支持事务化批量提交:多笔编辑(rename/set/insert/delete/members/uda)合成**单**新会话(`sesno` 仅 +1),多数据页 / 多 B 树路径 COW 合并到**同一新根**;同一元素多次编辑按序合并。
- **FR-021**: 系统 MUST 支持 dry-run 预览:在副本上应用批量但**不写盘**,产出元素级 diff `{added, removed, modified:[{refno,attr,old,new}]}`,与真实提交后的解码差异逐项一致。
- **FR-022**: 系统 MUST 提供安全护栏:默认拒绝删有成员的父 / 制造悬挂引用 / 重复 refno;`--force` 分级放行;`--inplace` 二次确认 + 落盘前强制 `verify_commit` 通过。写后自校验与批量提交 MUST 维持 Python↔Rust 双实现对齐(宪法 III)。

### Key Entities *(include if feature involves data)*

- **元素库文件 (Element DB)**:设计/目录/系统库;page0=PdmsHeader/db-control-block,后续为 B 树索引页 + 数据页(2048B,大端)。
- **模式库 (Schema DB, `*vir.dat`)**:每 noun 的 type-def(K=布局/offset 来源)+ 默认值(I 镜像 / J 哈希表);offset 的**权威来源**。
- **元素记录 (Element Record)**:隐式区(typedef 布局)+ DA/显式区(节点链)+ 成员区;头部含 refno/owner/页指针/sel。
- **属性描述符 (Attribute Descriptor)**:`[hash, stride, type, size, off|bit(主/备)]`;type 枚举见 data-model。
- **引用 (Reference)**:`(dbno, refseq)`;本库可解 / 跨目录库需对应文件。
- **UDA 条目**:DA 区中 hash>阈值的条目;强类型值或 `0xFFF` 序列化表达式。
- **会话 (Session)**:多版本单元;page0 会话指针指向当前会话页,会话页持有 B 树根/sesno。
- **事务 / 批次 (Transaction / Batch)**:累积多笔编辑、合成单新会话原子提交的单元;多路径 COW 合并到同一新根(US5)。
- **提交校验 (Commit Verification)**:`verify_commit` 的不变式集合——B 树正确性 / COW 不可变 / 读回符合预期 / 引用完整性。
- **差异 (Diff)**:dry-run 产出的元素级变更集 `{added, removed, modified}`。

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 对 `sam7200_0001` 的 WELD `/WB1`,离线解出 `POS=(9630.0, 8072.0, 5282.5)`、`ORI=(0,90,0)`,与结构化实测**逐字吻合**。
- **SC-002**: 双实现属性级对齐:目录库 `acp7002` **100%**(34450/34450 元素逐属性一致)、设计库 `sam7200` 隐式属性 **0 mismatch**(NAME 探测边界导致的计数差异见 SC-007 / `contracts/decode-contract.md` C3.2)。
- **SC-003**: 大库规模:`ams1112`(103MB)解出 **~42.2 万**元素,Rust 端 ≤ 数秒级完成全库遍历。
- **SC-004**: 写侧:任一 CRUD 提交后,整文件字节 diff **仅** page0 会话指针(`0x28..0x2B`),其余逐字节不变;CRUD round-trip 跨三会话一致。
- **SC-005**: 自动化回归全绿:`crates/e3d_io` `cargo test` 全部通过;Python 写侧自检 demo 全 PASS。
- **SC-006**: `0xFFF` 派生表达式 UDA 解码率 ≥ 95%(sam7200 实测 98.8%)。
- **SC-007**: 整库导出产出经 JSON 解析器校验合法,元素计数在双实现间一致:差异 ≤ **0.05%** 契约容差(实测 **<0.01%**),且仅限 NAME 探测边界(权威阈值见 `contracts/decode-contract.md` C3.2)。
- **SC-008**: 事务批量:≥3 笔混合编辑(rename+set_pos+insert)合一提交后 `sesno` 仅 +1、全部生效、前序会话逐字节不变、`verify_commit` 通过(active plan 完成判据 a)。
- **SC-009**: `verify_commit` 对故意坏提交(破坏 B 树 / 改动无关元素字节 / 悬挂引用)**必**报出对应 Issue(完成判据 b)。
- **SC-010**: `dry_run` 不写盘产出的元素级 diff 与真实提交后的解码差异**逐项一致**(完成判据 c)。

## Assumptions

- 基线为 AVEVA Everything3D **2.10** `core.dll`;不同主版本(如 3.1)页布局/地址可能不同,需另行取证。
- 模式库 `*vir.dat`(`desvir`/`catvir` 等)随 EXE 目录可得;offset 解析依赖之。
- 干净主记录 `sel=1`;`sel=0`/packed 在干净设计/目录数据中不出现(已定论)。
- UDA **真名/类型/单位**需外部字典库(udalib:`LXANAM/LXALEN/LXUNIT`),离线仅可得 hash + 有损短码 + 强类型值——真名解析为**范围外**(与 catalogue 同性质,缺文件)。
- 真实 running-E3D round-trip 取证依赖用户侧可用 E3D 环境——为写侧最终判据,**gated**(本环境不可得)。
- `pdms_io` 整 crate 端到端构建依赖修复 `rs-core ↔ surrealdb-3.1` API 漂移(项目外兄弟 crate)——E3D I/O 经独立 sub-crate **旁路**,不被此阻塞。
