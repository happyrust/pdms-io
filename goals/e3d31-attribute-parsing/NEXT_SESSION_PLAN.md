# Next Session Plan — Slice 4 Followups

> Originally produced 2026-05-16 at end of session "Steps 6-10".
> **Updated 2026-05-16** after Step 11 (default-value pool API) and Step 12 (UDA name verification):
> * #1 ✅ DONE on the runtime/decoder side (only #1b on-disk parser pass remains)
> * #2 ✅ **VERIFIED ALREADY SATISFIED** — fixture has 0 ATGTIX hash above `0x171FAD39`; the existing `system_names → db1_dehash → hex` chain resolves all 6644 hashes and all 1477 UDA-source records to readable names. The originally-imagined "UDA name string" payload at the ATGTIX-pointed bytes turned out to be value/enum/length-prefixed data, carved out as #2b.
> 66/66 tests green across `e3d-io` (50) + `e3d-attlib` (16). Three repos clean.

## Followup Triage

| # | Followup | Value | IDA evidence ready? | New IDA session required? | Code effort | Status |
|---|---|---|---|---|---|---|
| 1 | **Default-value pool extraction** (`RAW_USE_DEFAULT = 0xFFFFFFFF` → real default) | Turns sentinel zeros into semantically meaningful defaults; improves `summarize_element` readability | YES — `progress.jsonl` entry #20 gives the double-indexed formula `dword_6C21390[dword_6C21200[atgtdf_idx - 1] - 1]` | NO (light fixture verification only) | LOW: `AttrDefEntry.default_value` field + ATGTDF default-pool parser pass | **✅ DONE (Step 11) — runtime path + API + fixture-validated end-to-end. Only the on-disk layout parser pass in `attlib.dat` remains (see #1b below).** |
| 1b | **ATGTDF default-pool on-disk layout in `attlib.dat`** (populate `AttrDefEntry::default_value` automatically during `parse_file`) | Eliminates the manual `set_attr_default` injection step so real defaults surface for every fixture without code-level intervention | NO — needs a fresh IDA pass on the ATGTDF-buffer tail (the runtime `dword_6C21200` / `dword_6C21390` arrays are loaded from somewhere in `attlib.dat`, but the on-disk slice isn't yet recovered) | YES (small targeted IDA session, possibly 30–60 min) | LOW: add a parser pass after `parse_atgtdf` that reads default-index + default-value arrays and calls `set_attr_default` per hash | PENDING |
| 2 | ~~**UDA name resolution**~~ (the original framing assumed hashes > `0x171FAD39` which do not exist in this fixture) | n/a — the property "every UDA-source attribute has a readable name" is already true on this fixture | n/a | n/a | n/a | **✅ VERIFIED ALREADY SATISFIED (Step 12) — pinned by `e3d-io tests/uda_name_resolution_verified.rs` (2 tests). 0 ATGTIX hash > 0x171FAD39; 1477/1477 UDA-source records resolved; 0 hex fallback.** |
| 2b | **ATGTIX-pointed payload decoder** (decode the (page, word_offset) byte region for UDA-source ATGTIX entries) | The bytes hold type/kind/length headers + length-prefixed ASCII strings — likely enum-value or default-value tables, not name strings. Decoding them might unlock UDA value catalogs | NO — needs a fresh IDA pass to determine the record layout (the empirical dump shows `(type, kind, length)` + chars, but the actual semantics need IDA confirmation) | YES (small/medium focused IDA session) | MED: `e3d-attlib::parse_atgtix_payload()` + payload variant decoders + per-fixture validation | PENDING (carved out from Step 12 investigation) |
| 3 | **DB-internal noun template loader** (`dword_6A54024 + 60 × template_id + 16`) | **Structural unlock** — current code uses global ATGTDF position as the atnlog slot index, which is not noun-correct for many nouns; the per-DB template table is the authoritative noun-correct layout source | PARTIAL — descriptor offsets known (`+36 count`, `+56 hash`, `+60 stride`, `+64 aux`) per entry #16, but template-page physical storage in the DB file and `sub_5AF6AB0` decompile still pending (entries #18 / #19) | **YES** — dedicated IDA session (1–2h focused reversing) | MED–HIGH: new `e3d-io::record::template` module + `engine::summarize_element` migration with old-path fallback + cross-noun fixture validation | PENDING |

### Step 11 Delivery Summary (Followup #1)

- **Code**: `AttrDefault` enum + `AttrDefEntry::{new, default_value}` + `AttlibData::set_attr_default()` + `attr_default_to_value()` translator + `decode_attribute_by_hash` substitution branch.
- **Tests**: 9 new (4 e3d-attlib unit + 4 e3d-io unit + 1 e3d-io integration); baseline 55 → **64 GREEN**, 0 regressions, 0 warnings.
- **Fixture validation**: NXTR refno 17496/9621, slot 26 (hash `0x000FCD44`) confirmed as `RAW_USE_DEFAULT`; with `set_attr_default(AttrDefault::Int(0x424242))` the decoder returns `Int(0x424242)` instead of the generic `Bool(false)` sentinel decode.
- **Remaining as #1b**: a future small parser pass populates `default_value` automatically during `parse_file`; this is the only piece needing additional IDA work to close #1 fully.

### Step 12 Delivery Summary (Followup #2 — Verification & Re-scoping)

- **No new production code** — Step 12 is investigation + validation only.
- **Tests**: 2 new (`e3d-io/tests/uda_name_resolution_verified.rs`); baseline 64 → **66 GREEN**.
- **Findings** (sourced from byte-dump exploration of UDA-source ATGTIX targets on the fixture):
  - 6644 / 6644 ATGTIX hashes are within base-27 dehash range; 0 are above `0x171FAD39`.
  - All 1477 UDA-source records already resolve to short readable names via `db1_dehash` (e.g. `CNBC`, `TEE`, `LUG`).
  - `summarize_element` on NXTR yields 0 hex-fallback names.
  - The bytes at the (page, word_offset) location pointed to by UDA-source ATGTIX entries hold `(type, kind, length) + length-prefixed chars` records (observed: `TEXTPRIMITIVE`, `TEXTPRIMITIVES`, `Multi-line ...`), which look like **enum-value / default-value tables**, not name strings. Decoding them requires fresh IDA evidence and is carved out as Followup **#2b**.

### Updated Recommendation

- **#1b** (small targeted IDA pass for default-pool on-disk layout) and **#2b** (ATGTIX-pointed payload decoder) are both small IDA tasks; either is a viable next step if a quick IDA window is available.
- **#3** (DB-internal noun template loader) is the highest-value structural unlock and remains the recommended dedicated-IDA-session focus when a longer window is available.
- If neither IDA option is on the table for the next session, the previous Slice 4 work is at a clean checkpoint and no in-tree code-only work is queued.

## Recommended Order — Light → Heavy (Updated post-Step 12)

**#1 ✅ DONE on the runtime/decoder side. #2 ✅ VERIFIED ALREADY SATISFIED.** Remaining order:

- **#1b** (default-pool on-disk layout in `attlib.dat`): small targeted IDA pass; finishes off #1 by removing the manual `set_attr_default` injection step.
- **#2b** (ATGTIX-pointed payload decoder): small/medium IDA pass; uncertain value until the payloads are confirmed to carry useful schema info beyond what ATGTDF already provides.
- **#3** (DB-internal noun template loader): heaviest item, structural noun-correct decoding unlock; best done in its own dedicated IDA session.

Choose **#1b or #2b** for a quick IDA window; **#3** for a longer focused session.

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
