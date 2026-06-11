# Research: 写回管道存量盘点 + grill 决策记录

**Date**: 2026-06-11 | **Spec**: [spec.md](./spec.md)

> 固化两件事:① 写回管道两端的**存量能力证据**(逐条可复查);② grill Q1~Q6 决策记录(全按推荐,用户拍板)。写语义字节级真相一律引用 001,本文不重复。

## 1. 存量能力盘点(证据)

### 1.1 文件侧:e3d_io 写能力(001 已验证,38+1 测试)

| 能力 | API | 验证 |
|---|---|---|
| 六写原语 | `EdbWriter::{rename,set_pos,set_inline,set_members,delete,insert_clone}`(name 导向)+ 底层 `cow_*`(offset/refno 导向,pub) | 001 US2,S1~S8 全覆盖 |
| 批量单会话原子 | `EdbWriter::batch(|w|...)` — 多笔合一 sesno,出错整批回滚 | `batch_three_edits_single_session`/`batch_rollback_on_error` |
| 预览 | `EdbWriter::dry_run` + `element_diff` → `{added,removed,modified}` | `dry_run_matches_real_commit` |
| 自校验 | `verify_commit(orig,edited,ss,expects)` 四类(B 树/COW 不可变/读回/引用) | `verify_commit_sound_and_bad` |
| 护栏 | `delete_guards`(HasMembers/Referenced),force 分级 | `delete_guards_block_parent` |
| refno 寻址 | `record_off_via_root(db,root,refno)`(pub)/`Rdb::record_off_via_root` | 002 穷举等值测试 |

### 1.2 库侧:003 落库形态

- `pe` 最新态行:`{refno, owner, dbnum, sesno, noun, name, attrs: NamedAttrMap, explicit_attrs, children, deleted}`——编辑意图的数据来源面。
- 幂等机制先例:确定性字符串复合键 + upsert + 水位,`IngestReport` 可观测——`writeback_queue` 沿同款模式。
- kv-mem 测试基建(`surreal_mem.rs`):共享 rt + 串行锁 + 独立 ns/db(陷阱 ×2 已入档 003-T101)。

### 1.3 已知风险/缺口(实现期重点核查)

- **R1 无名元素寻址**:`EdbWriter` 高层为 name 导向;refno 寻址需经 `record_off_via_root` → 底层 `cow_*` 组合,或 batch 闭包内自行解析。**若组合面不足以覆盖六原语 refno 寻址 = e3d_io 红线决策点,停下上报**(禁止顺手改 e3d_io)。
- **R2 InsertClone 的新 refno 分配**:001 语义 = 该 dbno 最大 refseq+1 自动分配;写回回执必须把新 refno 带回队列行(库侧后续才能引用)。
- **R3 SetInline 值类型面**:inline 值有 real/int/ref 等型;EditOp payload 用强类型枚举,禁止裸字节。
- **R4 回声时序**:写回后若 watcher 在副本(而非原文件)上不可见 → 回声仅在 inplace/换文件场景发生;测试用"读副本再 ingest"模拟。

## 2. grill 决策记录(2026-06-11,全按推荐)

| # | 问题 | 决策 | 要点 |
|---|---|---|---|
| Q1 | 变更源形态 | **C→A 两层**:纯函数入口 `Vec<EditOp>`(tracer)+ `writeback_queue` 取数层 | 入口可离线测;队列显式可审计;否决 B(diff pe vs 文件:贵且意图含糊) |
| Q2 | 编辑操作面 | **A. 001 原语全集** | 全部已验证,映射层薄;砍面留死角 |
| Q3 | 落盘安全 | **默认副本 + verify_commit 强制;inplace 显式 opt-in + 二次确认** | 沿 001 CLI 语义(宪法 IV) |
| Q4 | 回环抑制 | **A. 接受回声** | 幂等 upsert 下库收敛于写回意图;回声=生效证明;不建标记表耦合 |
| Q5 | 验收基线 | **kv-mem + sam7200 round-trip + CLI smoke** | ams1112 量级可选 |
| Q6 | 范围 | **严格管道**:真机/并发/多 extent/重映射/UI/e3d_io 改动/C1 变更 全排除 | 留 005+ / 001-T039 |

## 3. 责任边界(目标态一句话)

> **e3d_io 拥有"怎么写"(COW/事务/校验,001 已验证,004 零改动);`surreal_writeback` 拥有"写什么"(EditOp 映射/队列状态机/回执);`PdmsIO` 门面与 003 ingest 不动——回声经既有增量→落库路径自然收敛。**
