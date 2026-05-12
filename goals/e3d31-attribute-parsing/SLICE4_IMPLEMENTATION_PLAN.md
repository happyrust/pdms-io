# Slice 4 — Byte Decode Implementation Plan

> Produced 2026-05-12 at end of an extensive IDA reverse-engineering session.
> All upstream blockers are resolved; this document gives the next session a
> turn-key implementation path.

## TL;DR

Implement `e3d_io::record::atnlog::read_attribute(element, attr_hash) -> Option<RawSlot>`
that reproduces the IDA `ATNLOG` reader (`sub_55BC98B`, jumped to from
`sub_55BC8DC` at `0x55BC9DC`).

## Reading formula (recovered from IDA)

```rust
fn read_attribute(element_implicit_data: &[u8], atgtdf_index: u32) -> u32 {
    // The header occupies the first 6 words (impl_len, refno×2, noun_hash, owner×2).
    // Attribute slots begin at word 6 (byte offset 24).
    // ATGTDF index is 1-based.
    let word_idx = 6 + (atgtdf_index - 1) as usize;
    let byte_off = word_idx * 4;
    u32::from_be_bytes(element_implicit_data[byte_off..byte_off + 4]
        .try_into().unwrap())
}
```

Caveat: the IDA formula is

```
page[(v22-1) * 512 + (v21-1) + v16]
```

where `v21` is `ATGTIX[handle].word_offset` (within page) and `v16` is the
ATGTDF index (1-based). Whether `v21` already accounts for the 6-word
record header is **TBD** — the implementation must accept a `header_word_count`
parameter and the integration test below validates against fixture data.

## Inputs required (all already in place)

| Input | Source | File |
|---|---|---|
| Element implicit bytes | `e3d_io::find_element(refno).raw_bytes` then `ParsedElement::parse` | `e3d-io/src/record/element.rs` |
| Attribute hash → ATGTDF index | `e3d_attlib::AttlibData::attr_def_map.get(&hash)` (must add `Vec<AttrDefEntry>` index) | `e3d-attlib/src/parser.rs` |
| Attribute hash → name | `e3d_attlib::lookup_system_name(hash)` or `db1_dehash` | `e3d-attlib/src/system_names.rs` |
| Attribute (type, kind, size) | `AttrDefEntry { data_type, kind, size }` | `e3d-attlib/src/parser.rs` |
| Data-type → reader | `e3d_io::record::attrs::{read_int, read_f64, read_pos_f64, read_refno}` | `e3d-io/src/record/attrs.rs` |

## Concrete implementation steps

1. **Add `atgtdf_position_of(hash) -> Option<usize>` to `AttlibData`**:
   the ATGTDF index is the 1-based position in `attr_defs` (already populated
   by `parse_atgtdf`). Verify the index matches what IDA `sub_5392270` uses
   — likely a sorted-hash binary search.

2. **Create `e3d-io/src/record/atnlog.rs`**:

   ```rust
   pub struct AtnlogSlot { pub raw_word: u32 }

   pub fn read_slot(
       element: &ParsedElement,
       atgtdf_position_1based: u32,
       header_word_count: usize, // try 6 first
   ) -> Option<AtnlogSlot> { … }

   pub enum AttrValue { Bool(bool), Int(i32), Real(f64), Ref(RefNo), Text(String), Position([f64; 3]), Direction([f64; 3]), Orientation([f64; 9]), Array(Vec<AttrValue>) }

   pub fn decode_value(
       element: &ParsedElement,
       atgtdf: &AttrDefEntry,
       atgtdf_position: u32,
       header_word_count: usize,
   ) -> Option<AttrValue> { /* dispatch on atgtdf.data_type (EXMAP type 1..12) */ }
   ```

3. **Integration test**: pick the fixture's first element (refno 17496/9621,
   noun NXTR, impl_len_words=32) and iterate `attr_defs` 1..32; for each,
   print `(atgtdf_index, hash, system_name_lookup, data_type, raw_word)`.
   Look for matches like `0x0FE5C3D2 / 0x11E75543` (which `DB_Noun::ReadData`
   reads from this very noun-class — they should yield non-trivial values).

## Validation checkpoints

- For a Bool attribute (data_type=3 kind=1), `raw_word` should be 0 or 1.
- For an Int attribute (data_type=1 kind=1), `raw_word` should be in a small
  range (typically 0..1000 for E3D enums; refnos for refs).
- For a Real attribute (data_type=2 kind=1), it spans 2 words; combine with
  `attrs::read_f64` (Fortran double convention).
- For a Reference (data_type=5 kind=1 or similar), 2 words = `RefNo`.

## Fall-back / chain (sub_55BD884, hash `&unk_5DAEB9C`)

When the direct slot is 0 (default value not stored), `ATNLOG` retries with
the **chain attribute** (hash from `&unk_5DAEB9C`). This is likely an
extension-pointer attribute; deferral for a v2 of this slice. Implementation
should just return `None` when slot == 0 and document the limitation.

## When raw_word is `-1` (= `0xFFFFFFFF`)

The IDA code looks up `dword_6C21390[dword_6C21200[atgtdf_index - 1] - 1]`
(default-value pool). e3d-attlib does not currently expose these defaults —
add a parser pass on the remaining UDA-ATGTDF buffer slots to recover them
in a follow-up.

## Out of scope for Slice 4

- UDA name resolution from UDA-ATGTIX page-offset pointer (Slice 5).
- Multi-extent element records (`refno.word0 & bit14`).
- Session-walk for `previousSession` link (separate goal).

## Files touched in this implementation

- `e3d-attlib/src/parser.rs` (small additions only — expose `atgtdf_position_of`)
- `e3d-io/src/record/atnlog.rs` (NEW)
- `e3d-io/src/record/mod.rs` (re-export)
- `e3d-io/tests/atnlog_decode_first_element.rs` (NEW integration test)

## Test expectations

`cargo test -p e3d-io --test atnlog_decode_first_element -- --nocapture`
should print a table where 5-10 attributes show readable values (not all
zero) for the first fixture element. If most slots are 0, the
`header_word_count` parameter is wrong — try 5, then 7.

## Cross-reference (IDA evidence)

- `sub_55BC8DC` @ 0x55BC8DC → trampoline into `sub_55BC98B` @ 0x55BC9DC
- MTR trace name: `ATNLOG`
- Data buffers (UDA tables): see `goals/e3d31-attribute-parsing/progress.jsonl`
  entry `slice_7_step_6_atnlog_decoded` for the full data-buffer map.

## Definition of done

- `cargo test -p e3d-io --test atnlog_decode_first_element` shows ≥ 5
  meaningful (non-zero, plausibly-typed) attribute values for the first
  fixture element.
- `attribute_names.json` lookup yields readable names for ≥ 3 of those.
- Code compiles clean (no warnings).
