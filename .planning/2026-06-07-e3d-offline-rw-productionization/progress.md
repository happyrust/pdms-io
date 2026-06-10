# Progress Log

## 2026-06-07 — 规划会话(planning-with-files)
- 按 `planning-with-files` 约定创建隔离计划 `.planning/2026-06-07-e3d-offline-rw-productionization/`,并设为 `.active_plan`(原 `2026-06-05-e3d-db-offline-attr-parser` 离线读+写格式分析已基本完成)。
- 恢复上下文:前序计划已把 E3D 离线**读**(全属性/NAME/owner/引用/UDA/导出)与**写**(COW + 新会话 CRUD,S1–S8)逆清并双实现(Python `e3d_write_full.py` 13 demo + Rust `src/e3d_decode.rs` 15 测试)。本方案聚焦"分析成果 → 生产能力"。
- 产出:
  - `task_plan.md` — 5 阶段方案(真机 round-trip 验证基线 → Rust 写侧 API 硬化 → 整 crate 集成解阻〔推荐 sub-crate 旁路〕→ CLI 工具化 → 收尾),含目标、完成判据、决策、风险与回退。
  - `findings.md` — 方案输入(已验证 read+write 状态 + 两大待解阻塞 + 约束 + 地址速查)。
  - `progress.md` — 本日志。

### 状态
- 方案状态:**proposed**(待用户评审/批准后执行)。
- 执行起点(若批准):**Phase 1**(真机 round-trip,需用户提供可用 E3D);若 E3D 暂不可用,可并行先做 **Phase 3 方案 B(sub-crate 解阻)+ Phase 4 CLI**(不依赖 E3D)。

### 待办/阻塞
- 等待是否批准 + 优先级裁决(尤其:Phase 1 是否现在可拿到真实 E3D;Phase 3 选方案 A〔授权改 rs-core〕还是 B〔sub-crate 旁路,推荐〕)。

## 2026-06-07 — 执行 Phase 3 方案 B(sub-crate 旁路)：抽 `crates/e3d_io`(已验证)
- 用户"按推荐继续下一步"⇒ 执行方案推荐的 E3D-无关起点 = **Phase 3 方案 B**(把 E3D 离线读写抽成独立可构建 crate,绕开 rs-core↔surrealdb 阻塞)。
- 落地:
  - 新建 `crates/e3d_io/`(std-only,edition 2024,无 `[workspace]`、无第三方依赖,**对齐 `crates/parse_pdms_db` 约定**)。
  - `git mv src/e3d_decode.rs → crates/e3d_io/src/lib.rs`(保留历史;唯一代码引用方=`src/lib.rs`)。
  - `pdms_io`:`Cargo.toml` 加 `e3d_io = { path = "crates/e3d_io" }`;`src/lib.rs` 把 `pub mod e3d_decode;` 改为 `pub use e3d_io as e3d_decode;`(call sites `pdms_io::e3d_decode::*` 路径不变)。
- **验证**:`cd crates/e3d_io && cargo test --release` ⇒ **15 passed**(0 failed,doctest `ignore`),无 warning;`cargo clean` + 删 `Cargo.lock`(depless lib 不提交锁)。⇒ E3D I/O 现为**工作区内常驻、可独立 cargo test 的 crate**(取代一次性临时 `_modcheck`),满足完成判据 (b) 的"sub-crate 旁路可测"部分。
- **未验证(预存阻塞,非本步引入)**:`pdms_io` 整 crate 仍因 rs-core↔surrealdb-3.1 不能 build,故 `pdms_io` 侧的 re-export + path 依赖为**约定正确但端到端待验**(待 surrealdb 兼容修复)。
- 文档:总结 §5/§9、索引交付物表更新指向 `crates/e3d_io`。

### 状态(更新)
- **Phase 3 方案 B:核心达成**(独立 crate + 工作区内 15 测试通过;pdms_io 复用已接线,待整 crate 可 build 端到端验)。
- 下一可并行(E3D-无关):**Phase 4 CLI**(读导出/写 CRUD 命令,基于 `crates/e3d_io`)、**Phase 2** API 硬化。**Phase 1**(真机 round-trip)仍待用户 E3D。

## 2026-06-07 — 执行 Phase 4（CLI）+ 修补 S4（delete）Rust 缺口
- 用户"按推荐继续"⇒ 接 Phase 4(CLI)。建 CLI 时**发现前序"Rust S1–S8 全对齐"措辞不准**:Rust 端实有 S1/S2/S3/S5/S6/S7/S8,**独缺 S4(delete)**(此前各轮把 delete 漏在 Python 端)。本轮一并补齐。
- **修补 S4**:`crates/e3d_io` 新增 `cow_delete_element(db, refno)`(复用 `record_off_via_root`→`find_leaf_path_by_loc` 定位主记录叶项 → 叶内左移压实 + word6+=4 → COW 路径到根 + 新会话;数据页留存=多版本;不做下溢合并,对齐 PDMS)。另加公共 `decode_at(db,ss,off)`(单元素解码 wrapper)+ `pub const NAME_HASH/POS_HASH`。
- **Phase 4 CLI**:`crates/e3d_io` 加 `[[bin]] e3d-io`(`src/main.rs`,std-only,手写参数):读 `show <name>`(隐式+DA+解析引用)/`refs <name>`;写(COW,默认写副本 `<db>.e3dout`,`--inplace` 显式覆盖)`rename`/`set-pos`/`delete`/`insert`(克隆,自动取该 dbno 最大 refseq+1);`--out`/`--cat` flags。
- **验证**:`cargo test` **18 passed**(15 + `decode_at_weld`/`cow_delete_real`/`cow_crud_roundtrip_real`〔克隆插入后删除,跨 3 会话 absent→present→absent + 键数 round-trip〕),CLI bin 编译通过、无 warning。**CLI 端到端 smoke**:`show /WB1`→18 隐式属性(POS=[9630,8072,5282.5]);`rename /WB1 /WB1-CLI --out tmp`→`sesno 36→37`,回读 tmp `show /WB1-CLI`→新名(refno 不变);临时文件已删。`cargo clean` + 无 Cargo.lock。

### 状态(更新)
- **Phase 3B 完成**(crates/e3d_io 常驻可测)+ **Phase 4 完成**(e3d-io CLI 读写 + smoke 验证)+ **S4 Rust 缺口补齐**(CRUD 在 Rust 真正完整 S1–S8)。**18 测试通过**。
- 下一可并行(E3D-无关):**Phase 2**(稳定 `EdbWriter` API + 错误类型 + catalogue/系统库写测试)。**Phase 1**(真机 round-trip)仍待用户 E3D。pdms_io 整 crate build 仍待 surrealdb-3.1(项目外)。

## 2026-06-07 — 执行 Phase 2(稳定 API + 典型错误 + 多库写测试)
- 用户"按推荐继续"⇒ Phase 2(写侧 API 硬化)。
- **typed error**:新增 `pub enum E3dError { ElementNotFound, Write, Io }`(impl Display/Error/From<io::Error>)。
- **ergonomic API**:新增 `pub struct EdbWriter<'a>`(name 导向,封装 `cow_*`):`open`/`from_bytes`/`bytes`/`into_bytes`/`save`/`offset_of`/`element` + 写 `set_inline`/`set_pos`/`rename`/`set_members`/`delete`/`insert_clone`(自动 refno=该 dbno 最大 refseq+1,返回 `(CowReport,new_refno)`)。另加 `Edb::into_bytes`。
- **CLI dogfood**:`e3d-io` 写命令(rename/set-pos/delete/insert)改用 `EdbWriter`,删去重复的 insert refno 逻辑与 `load_owned`;`write_out` 改收 `&[u8]`。
- **多库写覆盖**:新增 `edbwriter_catalogue_write`(对 **catalogue 库 acp7002** 改名首个命名元素并回读),证写侧不止设计库;`edbwriter_api_real`(clone→delete→rename→set_pos + `ElementNotFound` 类型断言)。
- **验证**:`cargo test` **20 passed**(18 + 2),无 warning;CLI smoke `insert /WB1 /WB1-P2`→新 refno (0x5C20,0x3D8F),回读 `show /WB1-P2` 正确;`cargo clean` + 无 Cargo.lock;无 lint。

### 状态(更新)
- **Phase 2/3B/4 + S4 补齐:均完成**。`crates/e3d_io` = std-only 读写库(读全属性/引用 + 写 S1–S8 CRUD)+ `EdbWriter` 稳定 API + `E3dError` 类型化错误 + `e3d-io` CLI;**20 测试**(含设计库 + catalogue 库写)。
- **剩余(均阻塞/需用户)**:**Phase 1** 真机 round-trip(需你侧 E3D)· **Phase 5** 收尾(UDA 真名字典库 / 更多 catalogue 库 / 性能;均可选)· pdms_io 整 crate build(待 rs-core↔surrealdb-3.1,项目外)。E3D-无关的生产化工作至此基本完成。

## 2026-06-07 — Phase 4 收尾:消除 `tools/e3d_decode_rs` 重复解码器(依赖 crates/e3d_io)
- 用户"按推荐继续"⇒ 执行 Phase 4 收尾的"统一 `tools/e3d_decode_rs` 也依赖 crates/e3d_io,消除重复解码器"。
- `tools/e3d_decode_rs/src/main.rs` 此前是 lib 的**完整副本**(~500 行解码器)+ CLI/JSON。改为 `use e3d_io::{index_db, resolve_refs, Attr, Edb, SchemaSet, Val}`,**删去全部重复解码器**,仅保留 CLI/JSON(`fmt_f64`/`json_str`/`val_json`〔原 `Val::to_json` 改自由函数〕/`attrs_json`/`main`);`Cargo.toml` 加 `e3d_io = { path = "../../crates/e3d_io" }`(标准工作区隔离不变)。删去与 lib 重复的自检测试(lib 已覆盖)。
- **修正**:误删了 `tools/e3d_decode_rs/Cargo.lock`(bin crate 应提交)→ `cargo generate-lockfile` 恢复(现含 e3d_io)。
- **验证**:`cargo run`(release)编译通过、**输出与改前逐项一致**:schemas 20 / noun types 1478 / **elements=10392 named=1209 noun_types=145** / top nouns 同;`--json --cat acp7002` 导出合法 JSON(element_count 10392、refmap_size 32586);无 lint、无 target 残留。
- ⇒ **解码器现为单一真源** `crates/e3d_io`;`tools/e3d_decode_rs` 退化为其薄读取/导出 CLI 消费者(`e3d-io` 为读写 CLI)。Python 端 `e3d_*` 仍为参考/交叉验证。

### 状态(更新)
- 解码器单一真源(crates/e3d_io)+ 两个 CLI(`e3d-io` 读写 / `e3d_decode_rs` 读导出,均消费同一 lib)+ Python 参考实现。E3D-无关生产化 + 去重**全部完成**。
- **剩余(阻塞/需用户/可选)**:Phase 1 真机 round-trip(需 E3D);Phase 5(UDA 真名需 udalib 字典库 / 更多目录库 / 性能);pdms_io 整 crate(待 surrealdb-3.1,项目外)。

## 2026-06-07 — 资源盘点:确认剩余项均为"缺外部资源"阻塞(非待办)
- 盘点本环境可用 db:`test-file/`={acp7002_0001(目录)、ams1112_0001(设计)、amssys(系统)、attlib.dat};`pdms-test-data/`={sam7200_0001(+back)、att_data_0/1、ele_data_0/1};AVEVA exe 目录无 `*uda*`/`*lex*` 字典库。
- ⇒ 确认:**无 udalib/UDA 字典库**(Phase 5 UDA 真名缺文件,阻塞)· **无运行中的 E3D**(仅静态 db + IDA idb;Phase 1 真机 round-trip 缺环境,阻塞)· **目录库仅 acp7002**(已集成;"加载更多目录库" 无更多文件可加)· pdms_io 整 crate 待 surrealdb-3.1(项目外)。
- **结论**:E3D-无关、本环境可做的生产化与去重工作**已全部完成并验证**;所有剩余项均受限于**用户侧资源/决策**(提供 E3D / udalib / 授权改 rs-core),非继续编码可推进。本计划在收到资源/裁决前**暂停于此**。

## 2026-06-07 — 最终交叉验证(整套工具链 green)
- 跑 Python 写侧全量自检 `python "docs/e3d 数据库分析/e3d_write_full.py"`:**S1–S8b 十三 demo 全 PASS**,exit 0(~26s)。
- 合并本会话已验证项 ⇒ **整套双实现工具链认证 green**:Python 13 demo PASS · Rust `crates/e3d_io` **20 测试**通过 · `e3d-io` CLI smoke(show/rename/insert)· `e3d_decode_rs`(去重后)编译+运行逐项一致(10392/1209/145 + 合法 JSON)。
- ⇒ 结构性重构(抽 crate + 去重 + S4 补齐 + EdbWriter)**未引入任何回归**;交付物完整认证。

## 2026-06-10 — 后记:pdms_io 整 crate build 解阻(原"项目外"阻塞已消除)
- rs-core↔surrealdb 阻塞不复存在:全仓统一 `happyrust/surrealdb` **dev-3.1 单栈**(本仓提交 `5f27c6fb`)+ rs-core **0.3.2** 适配(8 处 `from_value` 签名已修)。
- 实测:`cargo check` 默认 / `--features surrealdb` 均 0 错误;`cargo build -j2` 两轮均 Finished(全并行首轮因内存+页面文件耗尽假性失败,非代码问题)。
- 详见 specs/001 `tasks.md` T040 与 active plan `2026-06-07-e3d-offline-edit-safety-batch/progress.md` 同日条目。本计划"pdms_io 整 crate 待 surrealdb-3.1"的遗留表述就此关闭。
