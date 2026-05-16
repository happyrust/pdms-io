# Next Session Plan — Slice 4 Followups

> Originally produced 2026-05-16 at end of session "Steps 6-10".
> **Updated 2026-05-16** after Steps 11–15:
> * **#1 ✅ DONE** on the runtime/decoder side (Step 11)
> * **#2 ✅ VERIFIED ALREADY SATISFIED** (Step 12)
> * **#1b ✅ DONE** in Step 13 (parse_file auto-populates default_value for 111/111 kind=2 scalar entries)
> * **#3 IDA scaffold ✅ DONE** in Step 14 (`docs/ida-3.1-structures.md §14` + `noun_coverage_analysis` baseline)
> * **#3a Rust loader ✅ DONE** in Step 15: `e3d-io::record::template` (NounTemplate / TemplateAttr / from_attlib_noun_map) + `ReadOnlyEngine::summarize_element_with_template` lands; NXTR end-to-end yields 9 attrs matching the ATGTDF-path output (proves the wiring); ATGTSX-fallback comparison test SKIPs cleanly on the fixture (no candidate noun has ATGTSX coverage).
> 76/76 tests green across `e3d-io` (58) + `e3d-attlib` (18). Three repos clean.

## Followup Triage

| # | Followup | Value | IDA evidence ready? | New IDA session required? | Code effort | Status |
|---|---|---|---|---|---|---|
| 1 | **Default-value pool extraction** (`RAW_USE_DEFAULT = 0xFFFFFFFF` → real default) | Turns sentinel zeros into semantically meaningful defaults; improves `summarize_element` readability | YES — `progress.jsonl` entry #20 gives the double-indexed formula `dword_6C21390[dword_6C21200[atgtdf_idx - 1] - 1]` | NO (light fixture verification only) | LOW: `AttrDefEntry.default_value` field + ATGTDF default-pool parser pass | **✅ DONE (Step 11) — runtime path + API + fixture-validated end-to-end.** |
| 1b | **ATGTDF default-pool on-disk layout in `attlib.dat`** (populate `AttrDefEntry::default_value` automatically during `parse_file`) | Eliminates the manual `set_attr_default` injection step so real defaults surface for every fixture without code-level intervention | YES (acquired in Step 13 via `user-ida-pro-mcp.decompile` on `sub_55F53B8`) | n/a (IDA evidence acquired in this session) | LOW: re-interpret the trailing word that `parse_atgtdf` was already consuming as a typed `AttrDefault` | **✅ DONE (Step 13) — `parse_file` auto-populates default_value for all 111 kind=2 scalar entries on the fixture. e3d-attlib `67571d3` + e3d-io `19c3b15`.** |
| 1c | **ATGTDF kind=2 type=4 array defaults** (variable-length array default values in the runtime pool) | Surfaces array defaults for the small number of variable-length attributes that carry pre-populated default arrays | YES (same IDA decompile as #1b) | n/a | LOW–MED: requires an `IntArray` (or similar) `AttrDefault` variant, which would break the current `Copy` constraint — needs a small restructure of `AttrDefEntry` storage or a side table keyed by hash | PENDING (low priority — fixture has 0 kind=2 type=4 entries) |
| 2 | ~~**UDA name resolution**~~ (the original framing assumed hashes > `0x171FAD39` which do not exist in this fixture) | n/a — the property "every UDA-source attribute has a readable name" is already true on this fixture | n/a | n/a | n/a | **✅ VERIFIED ALREADY SATISFIED (Step 12) — pinned by `e3d-io tests/uda_name_resolution_verified.rs` (2 tests). 0 ATGTIX hash > 0x171FAD39; 1477/1477 UDA-source records resolved; 0 hex fallback.** |
| 2b | **ATGTIX-pointed payload decoder** (decode the (page, word_offset) byte region for UDA-source ATGTIX entries) | The bytes hold type/kind/length headers + length-prefixed ASCII strings — likely enum-value or default-value tables, not name strings. Decoding them might unlock UDA value catalogs | NO — needs a fresh IDA pass to determine the record layout (the empirical dump shows `(type, kind, length)` + chars, but the actual semantics need IDA confirmation) | YES (small/medium focused IDA session) | MED: `e3d-attlib::parse_atgtix_payload()` + payload variant decoders + per-fixture validation | PENDING (carved out from Step 12 investigation) |
| 3 | **DB-internal noun template loader** (`dword_6A54024 + 60 × template_id + 16`) | **Structural unlock** — current code uses global ATGTDF position as the atnlog slot index, which is not noun-correct for many nouns; the per-DB template table is the authoritative noun-correct layout source | YES — **IDA-side scaffold complete** in Step 14: `sub_5AF6AB0` / `sub_5B03900` / `sub_5AF0640` / `sub_5AECBC0` / `sub_5AA9270` all decompiled; full call chain + payload layout + descriptor table + child entry format documented in `docs/ida-3.1-structures.md §14`. Only `dword_6A54028` initialization in `db_open` (#3b) remains to fully nail file-side storage | NO for #3a (Rust); MAYBE for #3b (small IDA pass) | MED–HIGH: split into #3a (Rust loader + engine integration) and #3b (small IDA trace) — see below | **🟡 PARTIAL — IDA scaffold DONE (Step 14); Rust loader pending as #3a** |
| 3a | **Rust `e3d-io::record::template` module + engine integration** | Consume the IDA scaffold from Step 14 / `docs §14` to surface per-noun attribute layout in `summarize_element`; measure improvement against the `noun_coverage_analysis` baseline | n/a — already in `docs §14` | NO | MED: new module + per-noun loader + 3-phase implementation per `docs §14.5`; engine falls back to ATGTDF-position when template unavailable | **✅ DONE (Step 15) — data model + engine method + structural ATGTSX fallback + 6 new tests; NXTR end-to-end yields 9 attrs matching ATGTDF-path baseline. e3d-io `32bc9da`.** |
| 3b | **Trace `dword_6A54028` initialization in `db_open` path** | Identify which DB-file page(s) carry the raw template payload, so the Rust loader can populate `NounTemplate` directly from `ams1112_0001` instead of relying on the ATGTSX fallback (which only covers 39 nouns and uses uniform stride=1) | NO — small targeted IDA pass | YES (small IDA window) | LOW–MED: trace dword_6A54028 init point + locate template-bearing DB page(s) + write `NounTemplate::load_from_db_file(engine, noun_hash)` | PENDING (recommended next focused session) |

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

### Step 13 Delivery Summary (Followup #1b — Closed via IDA decompile)

- **IDA evidence**: `decompile` of `sub_55F53B8` (the ATGTDF on-disk reader) showed each kind=2 entry already stores the default-value word that ends up in the runtime pool `dword_6C21390[dword_6C21200[idx-1]-1]`. Pre-Step 13 e3d-attlib code already CONSUMED that word but mis-filed it as `size`.
- **Code**: `parse_atgtdf` now constructs `AttrDefEntry` with a populated `default_value: Option<AttrDefault>` for every kind=2 type≠4 entry via the new `decode_default_from_raw(type, raw)` helper (type=1 → Int, type=3 → Bool, others → Raw). Legacy `size` field semantics are intentionally preserved for EXMAP backward compat.
- **Tests**: 3 new (1 e3d-attlib unit + 1 e3d-attlib integration + 1 e3d-io integration); baseline 66 → **69 GREEN**.
- **Fixture coverage**: 111 / 111 kind=2 scalar entries on the fixture get an auto-populated default; e3d-io's `decode_attribute_by_hash` consumes them end-to-end without any test-side `set_attr_default` injection.
- **Carved-out residue**: `#1c` covers kind=2 type=4 array defaults (currently 0 occurrences on this fixture; would require relaxing `AttrDefault: Copy`).

### Step 14 Delivery Summary (Followup #3 — IDA Scaffold + Coverage Baseline)

- **IDA evidence acquired** in this session via `user-ida-pro-mcp.decompile`:
  - `sub_5AF6AB0` — template_lookup with descriptor binary search + 3-chunk cache
  - `sub_5B03900` — GALFE high-level entry; uses default template via `dword_6A54024 + 60 * dword_6A54024[2] + 16`
  - `sub_5AF0640` — chunk reader wrapping `FHDBRN` with 3-tier retry
  - `sub_5AECBC0` / `sub_5AA9270` — db state check + dispatcher context-swap
- **Documentation**: `docs/ida-3.1-structures.md` gains a new **Section 14** "Noun Template / Attribute Schema 调用链" with full call-chain diagram, payload layout (`+36 count / +56 hash / +60 stride / +64 aux`), descriptor table format (24 bytes/entry), child entry format (28 bytes/child), active template index semantics, and 3-phase implementation recommendations.
- **Coverage analysis**: New `e3d-io/tests/noun_coverage_analysis.rs` probes 11 candidate RefNos and reports per-noun (elem_count, impl_w, attrs, readable, hex) — observes 5 unique nouns (VERT, PLOO, ZONE, NXTR, FIXING), 24 attrs total, **100% readable / 0 hex fallback** on the current ATGTDF-position path. This is the "before" baseline against which a future Rust template loader can be measured.
- **Tests**: 1 new (e3d-io integration); baseline 69 → **70 GREEN**.
- **No new production code**: Step 14 is intentionally pure scaffold + diagnostic — the Rust loader is carved out as `#3a` to avoid landing a half-finished implementation.

### Step 15 Delivery Summary (Followup #3a — Rust loader landed)

- **Code**: `e3d-io::record::template` module (TemplateAttr, NounTemplate, from_attlib_noun_map, total_stride); `ReadOnlyEngine::summarize_element_with_template` walks the supplied template with cumulative stride and decodes via `decode_value_with_meta`. SYNO chain walk intentionally not performed in this first cut.
- **Tests**: 6 new (4 unit + 2 integration); baseline 70 → **76 GREEN**.
- **Fixture sanity**: on NXTR refno 17496/9621 with a stand-in template built from `attlib.attr_defs_unique().take(30)`, the template path yields 9 non-zero attrs (AVAIDB, NAME, RPTX, DESTEX, QTXT, VISI, DEPEND, UNIT, QSET) — identical to the ATGTDF-path baseline. The compare test SKIPs cleanly when no ATGTSX-covered noun is reachable.
- **What's missing for true "noun-correct" decoding**: the ATGTSX-fallback templates use uniform stride=1, which is the same assumption as the ATGTDF-position path — so the template path can't yet outperform the legacy path. The DB-file template loader (#3b) is needed to source real `(hash, stride, aux)` triples with non-uniform stride.

### Updated Recommendation (post-Step 15)

- **#3b** (`dword_6A54028` initialization trace + real `NounTemplate::load_from_db_file`) is the single highest-value remaining item — once it lands, the template path actually beats ATGTDF-position on nouns with variable-stride attribute records.
- **#1c** (array defaults) and **#2b** (ATGTIX payload decoder) remain as smaller, lower-priority cleanups.

## Recommended Order — Updated post-Step 15

**#1 ✅ / #1b ✅ / #2 ✅ / #3 IDA scaffold ✅ / #3a Rust loader ✅.** Remaining order:

- **#3b** (recommended next): small IDA trace of `dword_6A54028` initialization in `db_open` to identify the DB-file page(s) carrying the raw template payload; then `NounTemplate::load_from_db_file(engine, noun_hash)` populates a real noun-correct template (variable stride, real aux tokens). Without #3b the template path is wire-equivalent to the ATGTDF-position path on non-ATGTSX nouns.
- **#1c** array defaults — low priority.
- **#2b** ATGTIX payload decoder — medium priority.

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
