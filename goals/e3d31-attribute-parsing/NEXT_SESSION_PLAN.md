# Next Session Plan — Slice 4 Followups

> Originally produced 2026-05-16 at end of session "Steps 6-10".
> **Updated 2026-05-16** after Step 11 (default-value pool API) — Followup #1 is now ✅ **DONE** on the runtime/decoder side; only the attlib.dat on-disk layout for the default pool remains as a small future parser pass.
> 64/64 tests green across `e3d-io` (48) + `e3d-attlib` (16). Three repos clean.

## Followup Triage

| # | Followup | Value | IDA evidence ready? | New IDA session required? | Code effort | Status |
|---|---|---|---|---|---|---|
| 1 | **Default-value pool extraction** (`RAW_USE_DEFAULT = 0xFFFFFFFF` → real default) | Turns sentinel zeros into semantically meaningful defaults; improves `summarize_element` readability | YES — `progress.jsonl` entry #20 gives the double-indexed formula `dword_6C21390[dword_6C21200[atgtdf_idx - 1] - 1]` | NO (light fixture verification only) | LOW: `AttrDefEntry.default_value` field + ATGTDF default-pool parser pass | **✅ DONE (Step 11) — runtime path + API + fixture-validated end-to-end. Only the on-disk layout parser pass in `attlib.dat` remains (see #1b below).** |
| 1b | **ATGTDF default-pool on-disk layout in `attlib.dat`** (populate `AttrDefEntry::default_value` automatically during `parse_file`) | Eliminates the manual `set_attr_default` injection step so real defaults surface for every fixture without code-level intervention | NO — needs a fresh IDA pass on the ATGTDF-buffer tail (the runtime `dword_6C21200` / `dword_6C21390` arrays are loaded from somewhere in `attlib.dat`, but the on-disk slice isn't yet recovered) | YES (small targeted IDA session, possibly 30–60 min) | LOW: add a parser pass after `parse_atgtdf` that reads default-index + default-value arrays and calls `set_attr_default` per hash | PENDING |
| 2 | **UDA name resolution** (hash > `0x171FAD39` → follow UDA-ATGTIX page-offset pointer → length-prefixed string) | 6493 UDA hashes currently render as `0x????????`; unlocks user-engineering names | YES — entries #13 / #16 confirm ATGTIX is 2-word records `(hash, page*512 + word_offset)` pointing to a length-prefixed string region | NO (light IDA only to confirm length encoding) | LOW–MED: `e3d-attlib::lookup_uda_name()` + UDA-ATGTIX page deref + length-prefixed string reader | PENDING |
| 3 | **DB-internal noun template loader** (`dword_6A54024 + 60 × template_id + 16`) | **Structural unlock** — current code uses global ATGTDF position as the atnlog slot index, which is not noun-correct for many nouns; the per-DB template table is the authoritative noun-correct layout source | PARTIAL — descriptor offsets known (`+36 count`, `+56 hash`, `+60 stride`, `+64 aux`) per entry #16, but template-page physical storage in the DB file and `sub_5AF6AB0` decompile still pending (entries #18 / #19) | **YES** — dedicated IDA session (1–2h focused reversing) | MED–HIGH: new `e3d-io::record::template` module + `engine::summarize_element` migration with old-path fallback + cross-noun fixture validation | PENDING |

### Step 11 Delivery Summary (Followup #1)

- **Code**: `AttrDefault` enum + `AttrDefEntry::{new, default_value}` + `AttlibData::set_attr_default()` + `attr_default_to_value()` translator + `decode_attribute_by_hash` substitution branch.
- **Tests**: 9 new (4 e3d-attlib unit + 4 e3d-io unit + 1 e3d-io integration); baseline 55 → **64 GREEN**, 0 regressions, 0 warnings.
- **Fixture validation**: NXTR refno 17496/9621, slot 26 (hash `0x000FCD44`) confirmed as `RAW_USE_DEFAULT`; with `set_attr_default(AttrDefault::Int(0x424242))` the decoder returns `Int(0x424242)` instead of the generic `Bool(false)` sentinel decode.
- **Remaining as #1b**: a future small parser pass populates `default_value` automatically during `parse_file`; this is the only piece needing additional IDA work to close #1 fully.

## Recommended Order — Light → Heavy (Updated post-Step 11)

**#1 ✅ DONE on the runtime/decoder side.** Remaining order:

- **#2 UDA name resolution** is now the recommended next step: low IDA effort, low–medium code effort, immediate user-facing impact (UDA hashes get readable names everywhere).
- **#1b** (default-pool on-disk layout in `attlib.dat`) is a small targeted IDA pass — can be folded into the same session as #2 if convenient, or done separately as a quick win.
- **#3 DB-internal noun template loader** remains the heavy-IDA / structural-unlock item; best done in its own focused session after #2 and #1b land.

## Ready-to-Paste Startup Prompts

### Prompt #1 — Default-Value Pool (recommended first)

```
继续 e3d31-attribute-parsing Slice 4 收尾：实现 default-value pool 抽取。
背景：progress.jsonl entry #20 已给出 IDA 公式
  dword_6C21390[dword_6C21200[atgtdf_idx - 1] - 1]
仓库：D:/work/plant-code/e3d-attlib（扩展 AttrDefEntry + ATGTDF parser）
            D:/work/plant-code/e3d-io（atnlog::decode_value_by_exptype 在 raw_word==RAW_USE_DEFAULT 时取默认）
验收：fixture 上至少 1 个 default 槽位被命中并输出非哨兵值；e3d-attlib 12+ / e3d-io 43+ 测试不回归。
```

### Prompt #2 — UDA Name Resolution

```
继续 e3d31-attribute-parsing：实现 UDA 名字解析。
背景：hash > 0x171FAD39 的 UDA 当前在 system_names 查不到名字；
     UDA-ATGTIX 项的 (page, word_offset) 指向一段 length-prefixed UDA name string
     （见 progress.jsonl entry #13 / #16 — 已确认 ATGTIX 是 2-word 记录: hash + page*512+word_offset）。
仓库：D:/work/plant-code/e3d-attlib
     - 在 AttlibData 上新增 lookup_uda_name(hash) -> Option<String>
     - 实现 UDA-ATGTIX page-offset 解引用 + length-prefixed string 读取
     - 注意 PDMS 字节序与字长（参考已有 read_int/read_f64 约定）
     D:/work/plant-code/e3d-io
     - engine::summarize_element 输出名字时优先 system_names → 再尝试 attlib.lookup_uda_name → 最后 fallback to hex hash
验收：fixture 上 NXTR 或 STWALL 元素至少 1 个 UDA 槽位输出可读名（非 0x???? 形式）；
     新增 1 个 e3d-attlib 单元测试覆盖 UDA name reader；测试全绿。
```

### Prompt #3 — DB-internal Noun Template Loader (needs dedicated IDA session)

```
开启专门 IDA session：恢复 noun-correct attribute layout。
为什么：当前 summarize_element 用全局 ATGTDF position 当 atnlog slot index，对很多 noun
       并非该 noun 的真实 schema；DB-internal 模板才是 noun-correct layout 的权威源。

阶段 A — IDA 工作（预计 1-2h）：
  - decompile sub_5AF6AB0（template resolver，progress.jsonl entry #18 标记 pending）
  - 定位 template page 在 DB 文件（如 ams1112_0001）的物理存储位置
  - 验证已知 descriptor offsets: +36=count, +56=hash, +60=stride, +64=aux（entry #16）
  - 把发现写进 docs/ida-3.1-attributes.md 新章节 + ida_exports/3.1/db_templates.json

阶段 B — Rust 实现：
  - 新模块 D:/work/plant-code/e3d-io/src/record/template.rs
    定义 NounTemplate { count: u32, attrs: Vec<(hash, aux, stride)> }
    实现 load_template(noun_hash) → 读 DB 文件中 template page → 解析 descriptor
  - engine::summarize_element 切换为：noun_hash → NounTemplate → per-attr 字节 offset
  - 保留旧 ATGTDF-position 路径作为 fallback（防止回归）

验收：
  - NXTR fixture 元素的 attribute 列表与现有 9 个 unique attrs 对齐或超集；
  - 新增 noun（如 STWALL、PIPE）的 attribute 名字与值通过 template 路径解码；
  - 跨仓库提交，55+ 测试基线不回归。
```

## Where to Look First in a New Session

1. `goals/e3d31-attribute-parsing/progress.jsonl` — full chronological record, entries #1–#31.
2. This file — followup triage + ready-to-paste startup prompts.
3. `goals/e3d31-attribute-parsing/SLICE4_IMPLEMENTATION_PLAN.md` — the original Slice 4 turn-key plan (now historical, but useful for context on the ATNLOG formula and validation checkpoints).
4. `docs/ida-3.1-attributes.md` — architecture documentation (EXMAP, DBE_Value family, DB_Attribute layout, expType enum, revision log).
5. `ida_exports/3.1/attribute_types.json`, `attribute_names.json` — type table + 6377 system name entries.
6. `D:/work/plant-code/e3d-attlib/src/{parser.rs, exmap.rs, system_names.rs}` — current attlib parser, EXMAP dispatcher, name lookup.
7. `D:/work/plant-code/e3d-io/src/{engine.rs, record/atnlog.rs}` — current engine + ATNLOG byte reader.

## Session Close Note

This session produced 26 progress.jsonl entries (steps 6–10 across 4 daily sub-sessions), 55 green tests, and 9 commits across 3 repos. Slice 4 byte decode is in user-callable shape via `engine.summarize_element`. The three remaining followups are all clearly scoped; #1 #2 are mechanical IDA-evidence consumption, #3 alone needs a dedicated IDA session.
