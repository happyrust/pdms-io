# Findings:E3D 离线编辑 事务化/安全层（方案输入）

> 事实输入 = 前序计划成果。详见 `2026-06-05-e3d-db-offline-attr-parser/findings.md` §1–§22 与 `2026-06-07-e3d-offline-rw-productionization/`。此处只摘本方案前提。

## A. 已有写原语(`crates/e3d_io`,std-only,20 测试)
- 低层 `cow_*`:`cow_commit_inline`(S1)、`cow_commit_da_text`(S2/S6)、`cow_da_set_entry`/`cow_da_remove_entry`(S8)、`cow_members_set`(S7)、`cow_insert_element`/`cow_insert_element_split`(S3/S5)、`cow_delete_element`(S4)。**每个 = 一次 COW + 一个新会话**。
- 高层 `EdbWriter`(name 导向):`rename`/`set_pos`/`set_inline`/`set_members`/`delete`/`insert_clone` + `open`/`save`/`element`/`offset_of`;类型化 `E3dError`。
- 提交核心 `commit_edited_data_page`(改后数据页 → 自包含修正 rec[6]/rec[8] → 自叶到根 COW B 树路径 `find_leaf_path_by_loc` → `append_session`)。COW = `Edb` 持 `Vec<u8>`,改动 append;仅最后重指 page0 `0x28`。
- 校验器(**目前 test-only**):`btree_check`(`nav_ok`/balanced/sorted/dups/keyset)、`btree_descend`、`idx_entries`、`record_off_via_root`、`session_roots`。

## B. 缺口(本方案要补)
- **B.1 无批量/事务**:N 笔编辑 = N 个会话。需"多笔 → 单会话"。难点:多笔触及多数据页/多 B 树路径,需在同一"工作根"上逐笔 COW 串联到一个 session(`append_session` 仅在最后调一次)。互不相交记录较易;同页多记录/相交路径需合并 COW。
- **B.2 无写后自校验**:提交后未自动核验不变式/不可变/读回/引用完整性。`btree_check` 等已具备,只是 test-only,需提为库 API + 增"原页字节不可变"与"引用不悬挂"检查。
- **B.3 无 dry-run/diff**:无法预览批量改动(元素级属性 diff)。`decode_at`/`index_db` 可支撑:对副本应用后 old/new 解码对比。
- **B.4 无安全护栏**:未阻止删父留孤、悬挂引用、重复键等;无 force 分级。

## C. 关键约束
- 建立在 `crates/e3d_io`(std-only,无第三方依赖)之上,保持纯净。
- 写默认仅副本;落盘前强制 `verify_commit`。
- Phase 1–4 全 **E3D-无关**、本环境可做并验证(sam7200/acp7002 数据在);Phase 5(真机 round-trip / aios 接入)受**外部资源**阻塞(真 E3D / surrealdb-3.1,项目外)。

## D. 可复用件
- `commit_edited_data_page` / `find_leaf_path_by_loc` / `append_session`(批量需把 append_session 从"每笔"改为"每批一次")。
- `btree_check` / `record_off_via_root` / `session_roots`(校验)。
- `decode_at` / `index_db` / `resolve_refs`(diff + 引用完整性)。
- 真机验证基线:`db5_save_work` 契约(findings §15.3)。
