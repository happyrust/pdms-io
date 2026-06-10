# 开发方案:E3D 离线读写 — 生产化与集成

> 计划 ID:`2026-06-07-e3d-offline-rw-productionization`
> 创建:2026-06-07  状态:**proposed**(待评审/批准后执行)
> 前序计划:`2026-06-05-e3d-db-offline-attr-parser`(离线读+写格式分析,**已基本完成** —— 读侧全闭环、写侧 CRUD,Python + Rust 双实现并测试)
> 基线:AVEVA Everything3D 2.10 `core.dll`(已实时逆向 + 真实样本验证)

## 背景 / 动机

前序计划已把 E3D/PDMS DABACON 数据库的**离线读 + 写**格式彻底逆清并落地为可运行实现:
- **读**:`noun · NAME · refno · owner · 全部隐式/DA 属性 · UDA(强类型 + `0xFFF` 派生表达式)· 跨库引用 · owner 层级树 · 整库 JSON 导出`。
- **写**:离线 **COW + 新会话提交**(`db5_save_work`)全 **CRUD** —— 改内联值 / 变长 DA 文本(同页·跨页·链式) / 成员列表 / UDA·DA 条目 / 新增(B 树键插入+分裂/长高) / 删除;多版本、原文件不可变。
- **双实现**:Python 工具链(`e3d_write_full.py` 13 demo)+ Rust std-only 模块 `src/e3d_decode.rs`(S1–S8,15 测试)。

但距离"**生产可用**"仍差三点(均为本方案要解决的):
1. **真实 E3D round-trip 未取证** —— 写侧仅由"写后自读 + 字节不可变 + 开库契约 IDA 反编译"验证,尚未在真实 E3D 中打开 COW 写出的库确认加载/显示/无损。
2. **未集成进 `pdms_io` 整 crate** —— `src/e3d_decode.rs` 仅以 edition-2024 临时 crate 验证;整 crate 构建受**项目外** `rs-core↔surrealdb-3.1` API 漂移阻塞。
3. **缺稳定 API / CLI / 生产级硬化** —— 现有为模块函数 + 散落 Python 探针,缺统一公共 API、错误类型、CLI 工具与边界覆盖。

## 目标(Goal)

把已验证的离线 E3D 读+写,推进为 **`pdms_io` 中生产可用、经真实 E3D round-trip 取证、具稳定 API + CLI + 测试 + 文档** 的离线 E3D I/O 能力。

**完成判据(可验证)**:
- (a) 至少 1 组 CRUD 操作(改值 / 改名 / 新增 / 删除)写出的 E3D 库在**真实 E3D 中正确打开并显示新状态**,且其余元素无损;
- (b) E3D I/O 代码在 `pdms_io` 工作区内 `cargo test` **绿灯**(整 crate 或独立 sub-crate 旁路);
- (c) 提供一个可运行 **Rust CLI**,覆盖读(导出/树/引用)与写(rename/set/insert/delete + COW commit),取代散落 Python 探针。

## 现状(作为本方案输入,详见前序计划 findings §1–§22)

- 读侧全闭环、写侧 S1–S8 全打通,Python + Rust 双实现、逐项对齐并测试(Rust 15 测试 / Python 13 demo)。
- `src/e3d_decode.rs` = std-only,无第三方依赖;`walk` word6 界定;`CowReport` 报告 COW 结果。
- 已知阻塞:`pdms_io` → `aios_core(rs-core)` → `surrealdb-3.1`(`FromValue::from_value` 等 API 漂移,7+ 处),属**项目外兄弟 crate** 对移动中 git 分支的迁移;NASM 已装。

## 阶段(Phases)

### Phase 1 — 真实 E3D round-trip 验证基线（最高优先,去风险）  状态:proposed
> 写侧一切"机制完备"结论的**最终判据**。一旦真机验证通过,后续硬化/集成才有意义;若发现不兼容(如新 DA/list 页的完整页型保真、`page0[w11]` 语义),需回流修正写侧。
- [ ] 取一真实工程库副本,用本工具做 1 组 CRUD(优先:改 POS / 改 NAME / 新增克隆元素 / 删除),COW + 新会话提交。
- [ ] **在真实 E3D 中打开**该副本:确认加载无错、目标元素显示新值/新名/新增可见/删除消失,其余元素无损;`db5_save_work`/刷新一次确认 E3D 接受会话结构。
- [ ] 建立"**写后双读**"回归流程(本读取器 + 真 E3D 对照),并把发现回流 `findings`。
- **前置**:需用户提供可用 E3D 环境 / 可写工程库。
- **风险/回退**:若无 E3D,降级为"写后自读 + 字节不可变 + 开库契约"现有判据,并在文档显著标注"未真机取证";不阻塞 Phase 2/4。

### Phase 2 — Rust 写侧 API 硬化 + 边界覆盖  状态:**核心已达成**(2026-06-07)
- [x] 稳定**公共 API** `EdbWriter`(name 导向:`set_inline`/`set_pos`/`rename`/`set_members`/`delete`/`insert_clone` + `open`/`save`/`element`/`offset_of`)+ 类型化错误 `enum E3dError`(ElementNotFound/Write/Io)。CLI 已 dogfood。
- [x] **多库写覆盖**:catalogue 库(acp7002)改名往返测试 `edbwriter_catalogue_write` 通过(证写侧不止设计库)。
- [ ] 后续(可选):大库(ams1112)写压测;`sel=0`/packed 写路径断言(干净数据不现);新建 DA/list 页**完整页型保真**(待 Phase 1 真机反馈);把全部 `cow_*` 内部 `String` 错误也迁到 `E3dError`(目前在 API 边界类型化)。

### Phase 3 — 整 crate 集成解阻（两方案择一)  状态:**方案 B 核心已达成**(2026-06-07)
- **方案 A**:修 `rs-core↔surrealdb-3.1` 兼容(7+ 处 `FromValue::from_value` 返回类型 + 错误构造)。**需用户/团队授权**(项目外兄弟 crate,改动有连锁与回归风险)。
- **方案 B(推荐,已执行)**:把 `e3d_decode` 抽成**独立可构建 sub-crate** `crates/e3d_io`(std-only,**不依赖 rs-core/surrealdb**),`pdms_io` 经 path 依赖 + `pub use e3d_io as e3d_decode` 复用;绕开阻塞、即时可 `cargo test`。
  - [x] 新建 `crates/e3d_io`(edition 2024,无 `[workspace]`/无第三方依赖,对齐 `crates/parse_pdms_db`);`git mv src/e3d_decode.rs → crates/e3d_io/src/lib.rs`;`pdms_io` 接线(`Cargo.toml` path dep + `lib.rs` re-export)。
  - [x] **完成判据(B)达成**:`cargo test` 在 `crates/e3d_io` **15 passed**(工作区内,取代一次性临时 crate)。
  - [⚠] `pdms_io` 整 crate 端到端 build 仍待 rs-core↔surrealdb-3.1(方案 A / 上游);pdms_io 侧 re-export 为约定正确、待验。

### Phase 4 — CLI / 工具化  状态:**核心已达成**(2026-06-07)
- [x] `crates/e3d_io` 加 `[[bin]] e3d-io`(std-only,手写参数解析):读 `show`/`refs`(隐式+DA+解析引用);写(COW,默认副本 `<db>.e3dout`,`--inplace` 覆盖)`rename`/`set-pos`/`delete`/`insert`;`--out`/`--cat`。
- [x] **顺带补齐 Rust S4(delete)缺口**:`cow_delete_element` + 公共 `decode_at` + `pub const NAME_HASH/POS_HASH`。CRUD 在 Rust 真正完整。
- [x] 端到端 smoke 验证(`show /WB1`、`rename`+回读)+ `cargo test` 18 passed。
- [x] **统一 `tools/e3d_decode_rs` 依赖 `crates/e3d_io`**(消除 ~500 行重复解码器;CLI/JSON 保留,输出逐项一致:10392/1209/145 + 合法 JSON)。解码器现为**单一真源**。
- [ ] 后续(可选):`e3d-io` 加 `export --json` / `tree`(读侧导出,目前由 `e3d_decode_rs` 提供);最终可只保留一个 CLI。

### Phase 5 — 收尾(可选,非阻塞)  状态:proposed
- [ ] UDA 真名:加载 UDA 字典库 `udalib`(`LXANAM/LXALEN/LXUNIT`)→ 解 UDA 名/类型/单位(与 catalogue 跨库同性质)。
- [ ] 更多 catalogue 库(dbno 15206/15207/15213…)以解析全部规格/材料引用。
- [ ] 性能/文档/示例;`0xFFF` 表达式 UDA 的 AST 级语义编辑(范围外,按需)。

## 决策记录(Decisions)

- **Phase 1 优先**:真机 round-trip 是写侧的最终判据,先去最大风险。
- **Phase 3 推荐方案 B(sub-crate 旁路)**:在无授权改 `rs-core` 的前提下即时解阻、可测;避免触碰移动中的项目外 surrealdb 迁移。
- **保持 std-only 纯净**:E3D I/O 核心不引入重依赖,利于独立构建/测试与复用。
- **写操作默认仅作用于副本**:破坏性 `--inplace` 需显式开关 + 二次确认。

## 风险与回退

| 风险 | 影响 | 回退 |
|---|---|---|
| 无可用真实 E3D(Phase 1 前置) | 无法真机取证 | 降级为现有自读/字节/契约判据,文档标注;不阻塞 P2/P4 |
| 真机发现写侧不兼容(页型保真 / `page0[w11]` 语义) | 写侧需回流修正 | P1 早发现早改;先验证最简单的"改值"再升级到结构性操作 |
| 方案 A 改 rs-core 连锁 | 波及整 crate / 回归 | 采用方案 B 旁路 |
| packed/`sel=0` 写(catalogue) | niche 边界 | 干净数据不现已定论;catalogue 写按需专门取证 |

## 备注

- 本方案不重做已闭环的格式分析(前序计划),聚焦"分析成果 → 生产能力"。
- 执行起点(若批准):**Phase 1**(需用户 E3D);若 E3D 暂不可用,可并行先做 **Phase 3 方案 B(sub-crate 解阻)+ Phase 4 CLI**(均不依赖 E3D)。
