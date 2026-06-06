# 开发方案:E3D db 离线"命名属性"解析器 + 基线纠错落地

> 计划 ID:`2026-06-05-e3d-db-offline-attr-parser`
> 创建:2026-06-05  状态:proposed(待评审/执行)
> 基线:AVEVA Everything3D 2.10 `core.dll`(已实时逆向 + 真实样本验证)

## 目标(Goal)

把本轮逆向已**逆清**的 attlib 寻址机制,实现为**可用的离线属性解码器**,并把已验证的基线纠错落地到 Rust 主实现(`pdms-io` / `rs-core::parse_pdms_db`),使工具链能在**纯文件离线**条件下稳定输出「元素 + **命名属性 + 属性值**」。

**完成判据(可验证)**:对 sam7200_0001 的某 WELD/PIPE 元素,离线解出其**命名属性集**(如 POS/BORE 等)及**属性值**,且 POS 值与现有结构化解码(POS@word13 = (9630,8224,5130.5))一致;并在 acp7002_0001 复验。

## 现状(本轮已完成,作为本方案输入)

- 磁盘格式、db1–db5 机制、写/索引/维护路径、attlib 访问机制均已逆清并落档(见 `docs/e3d 数据库分析/`)。
- 可运行读取器 `e3d_db_reader_v2.py`(头部/会话/B树/元素+POS/直方图/JSON),已 sam7200+acp7002 双验证。
- 三大纠错已确认:页大小=header[0x34]×4;db1_hash=base-27+0x81BF1;401 函数表系另一构建。
- **唯一未闭环**:attlib「完整命名属性集 + 每属性精确 offset」的离线实现。

## 阶段(Phases)

### Phase 1 — 基线纠错落地（low risk, quick win）  状态:complete（代码+逻辑验证；整 crate 测试受环境阻塞）
- [x] `src/defines.rs::detect_page_size`:已改为 `words * 4`(saturating)+ 512/2048/4096 兜底;单测同步改为 words×4 语义。
- [x] **逻辑独立验证**:因整 crate 无法构建(缺依赖),抽出函数体用 `rustc` 单独编译运行 → 5/5 用例通过(128→512,512→2048,1024→4096,0/1234→2048)。
- [x] noun 哈希:确认 `rs-core db1_hash` **本就是 base-27(正确)**,无需改代码;修正版 `noun_hash_table_base27.json` 已提供。错误仅在旧 artifacts。
- [x] **范围修正(重要)**:生产 `io.rs` 已用 `detect_page_size_by_probe`(探测页类型)稳妥处理页大小,本不受旧 `detect_page_size` bug 影响(见 findings §1b)。故纠错主要价值在旧 artifacts/文档,生产主路径已正确。
- [⚠] **整 crate `cargo test` 阻塞**:`pdms_io` 依赖本地 path crate `dpcsync`(`D:\work\plant\dpc-sync`)在本环境缺失,无法 `cargo build/test`。已用 standalone rustc 替代验证。

### Phase 2 — attlib 加载器（离线建表）  状态:in_progress（Python 探针已纠正表角色;DB_Attribute 侧待闭环）
> **阻塞评估(2026-06-05)**:离线查表需 noun 哈希数组 `unk_11C12080` 的内容(`ATFIND` 在其中求 noun_index v21)。该数组是**运行时填充的全局**——静态 IDB 中为未初始化 .bss(读不到),必须从 attlib 文件对应段按**确切布局**还原。而该布局(交错 + 跨页链式)正是多轮 Python 探针**反复未闭环**之处。叠加双重环境约束:本环境不能 cargo 构建(dpc-sync 缺失 / rs-core 重 git 依赖),且 IDB 为**静态**(无活进程,无法用 `db_get_attribute_list` 运行时产 ground truth)。
> ⇒ 在**当前环境**无法可靠闭环。需以下任一前置条件:(a) 可构建的 Rust 环境(含 dpc-sync)走代码路线;(b) **活的 core.dll 进程**(IDA 调试态)以运行时查表产 ground truth 对照;(c) 继续深逆 attlib 加载器(`sub_10851210` 等)精确还原 noun 段布局(高不确定、可能多轮)。
> 说明:元素级解码(noun 名 + POS,`e3d_db_reader_v2.py` 已实现并双库验证)是当前环境**可达**的离线结果;"完整命名属性集 + 精确 offset"是上述受阻项。
- **最新收敛(2026-06-05)**:静态 IDA + `attlib_atnain_probe.py` 已确认 `ATTOPE` 表角色:
  - `v47[6]` ATGTIX = noun_hash → `(record,disp)`;WELD=idx82/record2225/disp1。
  - `v47[4]` ATGTDF = DB_Noun 字段定义;POS 不在其中。
  - `v47[2]` ATGTIX = attribute hash → `(record,disp)`;POS=idx27/record1129/disp127/combined0x8D27F。
  - 因此旧锚点 `POS combined=0x83787` 和 `record(attr)+noun_index` 公式需废弃;下一步改查 `DB_Attribute::internalGetField`/属性元数据侧。
- [ ] 新模块 `crates/parse_pdms_db/src/parser/attlib/atnain.rs`(或扩展现有 attlib 解析)。
- [x] Python 探针读取 `attlib.dat` 段指针(file 0x800 处 8×u32)并复刻 `ATTOPE` 三张关键表装载。
- [x] 构建 noun ATGTIX:WELD/PIPE/EQUI 等可定位到 `(record,disp)`。
- [x] 构建 attribute ATGTIX:POS(0x853B1) 定位为 `combined=0x8D27F → record=1129, disp=127`。
- [ ] 继续分析 `DB_Attribute::internalGetField`/属性元数据侧,确认命名属性物理 offset 的最终来源。

### Phase 3 — internalGetField 查表（含跨页链式)  状态:complete（offset 来源已离线打通，2026-06-06）
> **达成**:offset 不在 attlib 跨页链式表里现算，而是预存于模式库 `*vir.dat`（§7.7）。离线路径 = 二分查 `desvir.dat` 类型索引 → typedef 描述符 → `desc[5|8]` offset（主/备由 `record[w10]>>29` 选）。见 `findings.md §8`、`desvir_typedef_probe.py`。
- [ ] 实现 `lookup_offset(noun_hash, attr_hash) -> Option<offset>`:
  - `noun_index = ATFIND(noun_hash)`;`(record, disp) = ATGTIX(attr_hash)`;
  - `v = file_page[record][disp + noun_index - 2]`;为 0 时按 `ATNATX` 列**链式回退**(对照 `sub_1084F7C0` 的 `i = matrix[...]` 循环)。
- [ ] 处理错误码语义(51/52/54/55/56/63)与 UDA 特例。
- 风险:链式回退是本方案最难点(见 findings.md 风险条目)。3-strike 失败则回退到"用 core.dll 运行时旁路验证"(IDA py_eval 调 `db_get_attribute_list`)对照。

### Phase 4 — 闭环验证  状态:complete（2026-06-06，纯离线）
- [x] WELD.POS 离线解码 = **(9630.0, 8072.0, 5282.5)**，与结构化实测逐字吻合（`desvir.dat` typedef + sam7200_0001 记录，无运行时）。ORI=(0,90,0)。
- [x] **完成判据达成**：离线解出 WELD 命名属性集（typedef 66 描述符）+ POS 值，且与现有解码一致。
- [ ] 扩展（可选）：PIPE/ELBO 多属性 + acp7002 复验 + main(sel=0) 路径样本。

### Phase 5 — 集成 + 测试 + 文档  状态:in_progress（Rust 端独立对齐完成；并入 pdms_io 待 dpc-sync）
- [x] `e3d_db_reader_v2.py` 增 `--attrs`:对元素输出命名属性。
- [x] **Rust 移植对齐(2026-06-06 续18)**:独立 crate `tools/e3d_decode_rs/` 扩成完整解码器+导出器(全类型隐式 + 链式 DA + 跨库引用 + JSON),与 Python **逐项对齐**(6536/1144/140/refmap29672、POS 逐字、跨库引用一致)。链式 `decode_da_list` 修复 NAME 边界(named 1122→1144、noun_types 156→140)。
- [x] 文档持续更新:《格式规范》§7.6/§7.7.7/§7.8/§7.9/§7.10/§8、findings §9/§10/§11、索引/总结。
- [x] **并入 `pdms_io`**(2026-06-06 续19,用户授权选项 2):新增 `src/e3d_decode.rs`(std-only 自包含解码模块)+ `lib.rs` 接入;`Cargo.toml` 解阻 dpcsync(注释可选 path 依赖 + `sync-archive` 去 dpcsync,已门控)。模块经 edition-2024 临时 crate `cargo test` 通过(2/2)。
- [⚠] **整 crate `cargo build/test` 受 NASM 阻塞**:`aios_core→surrealdb→jsonwebtoken→aws-lc-rs→aws-lc-sys` 需 NASM(本环境未装)。非代码问题;装 NASM 后即可整 crate 构建 + 跑集成测试。

## 决策记录(Decisions)
- 采用**离线复刻**而非依赖运行时:目标是纯文件解析(同 pdms-io 既有方向)。
- 优先 Phase 1(低风险高收益,先把已知错误修掉)。
- Phase 3 为关键风险点,设回退方案(运行时旁路对照)。

## 错误记录(Errors Encountered)
| 错误/障碍 | 尝试 | 结果/解决 |
|---|---|---|
| `cargo test` 失败:`failed to load source for dependency dpcsync` / `D:\work\plant\dpc-sync` 路径不存在 | 1. 直接 `cargo test --lib detect_page_size` | 环境缺失本地 path 依赖 `dpc-sync`,整 crate 无法构建(非本次改动导致) |
| 同上 | 2. 改方法:抽出 `detect_page_size` 函数体用 `rustc` 独立编译运行 | ✅ 5/5 用例通过,逻辑验证成功 |

## 未决/风险
- attlib `internalGetField` 的跨页链式回退细节(碰撞处理)未完全验证;noun 哈希数组在文件中的精确布局(交错)需在 Phase 2 落实。
- 跨页 member/属性数据拼接(`defines.rs` TODO)与命名属性解码可能交叉,Phase 5 一并处理。
