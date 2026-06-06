//! Fully-offline E3D/PDMS DABACON element decoder (std-only, no extra deps).
//!
//! Ported from the validated standalone tool `tools/e3d_decode_rs` + the Python
//! toolchain (`e3d_attr_decoder.py` / `e3d_export.py`), itself derived from live
//! IDA reversing of AVEVA Everything3D 2.10 `core.dll`. See `docs/e3d 数据库分析/`.
//!
//! Pipeline: header -> latest session -> B-tree index walk -> per-element decode of
//! noun / NAME / refno / owner / all implicit (typedef) attributes / DA (explicit)
//! attributes, plus cross-(db) reference resolution. Big-endian on disk.
//!
//! Schema (`*vir.dat`) type-defs supply each attribute's storage offset (§7.7); the
//! per-type decode + sel(packed/unpacked) + UDA rules are §7.6/§7.7.7/§7.10.
//!
//! This module is intentionally self-contained (only `std`) so it can be built and
//! tested independently of the heavier `pdms_io` dependencies.

use std::collections::{HashMap, HashSet};
use std::fs;

const SCHEMA_PAGE: usize = 2048;
const DATA_WORDS: usize = 511;
const BASE27: u32 = 0x81BF1;
const UDA_THRESHOLD: u32 = 0x171FAD39;
const NAME_HASH: u32 = 0x9C18E;
const INDEX_NOUN: u32 = 0xCC47DF;
const POS_HASH: u32 = 0x853B1;

fn be_u32(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn rd_words(b: &[u8], off: usize, n: usize) -> Vec<u32> {
    let n = n.min(b.len().saturating_sub(off) / 4);
    (0..n).map(|i| be_u32(b, off + 4 * i)).collect()
}

/// Chained read for schema dbs (511 data + 1 link word / 512-word page; page P @ (P-1)*2048).
fn chain(b: &[u8], start_page: u32, total: usize) -> Vec<u32> {
    let off = |p: u32| (p as usize - 1) * SCHEMA_PAGE;
    let mut out = Vec::new();
    let (mut page, mut rem) = (start_page, total);
    while rem > DATA_WORDS {
        let wp = rd_words(b, off(page), 512);
        if wp.len() < 512 {
            return out;
        }
        out.extend_from_slice(&wp[..DATA_WORDS]);
        page = wp[DATA_WORDS];
        rem -= DATA_WORDS;
    }
    out.extend_from_slice(&rd_words(b, off(page), rem));
    out
}

/// base-27 (+0x81BF1) reversible name hash; UDA (>0x171FAD39) -> stable ':UDA_0x..' tag.
/// The real UDA name lives in the UDA dictionary db (see §7.10), not in the hash.
pub fn db1_dehash(mut h: u32) -> String {
    if h > UDA_THRESHOLD {
        return format!(":UDA_0x{:X}", (h - UDA_THRESHOLD) % 0x100_0000);
    }
    if h <= BASE27 {
        return String::new();
    }
    h -= BASE27;
    let mut s = String::new();
    while h > 0 {
        let d = (h % 27) as u8;
        s.push(if d == 0 { ' ' } else { (d + 64) as char });
        h /= 27;
    }
    s
}

/// Lossy ':'+base-64 UDA short-code (core.dll DEHASH 0x1065B930). NOT the real name.
pub fn dehash_uda_code(value: u32) -> Option<String> {
    if value <= UDA_THRESHOLD {
        return None;
    }
    let mut v = (value - UDA_THRESHOLD) % 0x100_0000;
    let mut out = String::from(":");
    for _ in 0..4 {
        let d = v % 64;
        out.push(if d == 0 { ' ' } else { (d as u8 + 32) as char });
        v /= 64;
    }
    Some(out.trim_end().to_string())
}

fn looks_like_noun(h: u32) -> bool {
    let n = db1_dehash(h);
    (1..=8).contains(&n.len()) && n.chars().all(|c| c == ' ' || c.is_ascii_uppercase())
}

fn f64_lowfirst(lo: u32, hi: u32) -> f64 {
    let (h, l) = (hi.to_be_bytes(), lo.to_be_bytes());
    f64::from_be_bytes([h[0], h[1], h[2], h[3], l[0], l[1], l[2], l[3]])
}

fn f32_word(w: u32) -> f64 {
    f32::from_be_bytes(w.to_be_bytes()) as f64
}

/// Decoded attribute value.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Reals(Vec<f64>),
    Ints(Vec<u32>),
    Refs(Vec<(u32, u32)>),
    Bool(bool),
    Text(String),
    Null,
}

impl Val {
    pub fn is_null(&self) -> bool {
        matches!(self, Val::Null)
    }
}

#[derive(Clone, Copy)]
struct Desc {
    typ: u32,
    size: u32,
    off: u32,
    bit: u32,
    alt: u32,
    altbit: u32,
}

/// One decoded named attribute.
pub struct Attr {
    pub hash: u32,
    pub name: String,
    pub typ: u32,
    pub val: Val,
}

/// Decode one inline (implicit) attribute value from an element record (§7.6.2).
fn decode_one(w: &[u32], d: &Desc) -> Val {
    let sel = (w[10] >> 29) & 1;
    let (off, bit) = if sel == 1 { (d.alt, d.altbit) } else { (d.off, d.bit) };
    let off = off as usize;
    if off == 0 || off >= w.len() {
        return Val::Null;
    }
    if d.typ == 5 {
        return Val::Bool(((w[off] >> bit) & 1) != 0);
    }
    let prefixed = d.size > 1 || matches!(d.typ, 14 | 15 | 18 | 19);
    let (cnt, data) = if prefixed {
        let c = w[off];
        if c >= 4096 {
            return Val::Null;
        }
        (c as usize, off + 1)
    } else {
        (d.size as usize, off)
    };
    match d.typ {
        2 | 6 => {
            let mut v = Vec::new();
            for j in 0..cnt {
                if sel == 1 {
                    let k = data + 2 * j;
                    if k + 1 < w.len() {
                        v.push(f64_lowfirst(w[k], w[k + 1]));
                    }
                } else {
                    let k = data + j;
                    if k < w.len() {
                        v.push(f32_word(w[k]));
                    }
                }
            }
            Val::Reals(v)
        }
        4 | 8 | 16 => {
            let mut v = Vec::new();
            for j in 0..cnt {
                let k = data + 2 * j;
                if k + 1 < w.len() {
                    v.push((w[k], w[k + 1]));
                }
            }
            Val::Refs(v)
        }
        _ => {
            let mut v = Vec::new();
            for j in 0..cnt {
                if data + j < w.len() {
                    v.push(w[data + j]);
                }
            }
            Val::Ints(v)
        }
    }
}

/// Parse a flat [hash][ctrl:type<<26|wc][value...] entry stream (DA/explicit, §7.8).
fn parse_attr_words(words: &[u32], max_entries: usize) -> Vec<Attr> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while out.len() < max_entries && i + 1 < words.len() {
        let (h, ctrl) = (words[i], words[i + 1]);
        let typ = ctrl >> 26;
        let n = (ctrl & 0x3FFFFFF) as usize;
        if h == 0 || n == 0 || n > 256 || i + 2 + n > words.len() {
            break;
        }
        let val = if matches!(typ, 10 | 14 | 15) {
            let ln = words[i + 2] as usize;
            let mut bytes = Vec::new();
            for w in &words[i + 3..i + 2 + n] {
                bytes.extend_from_slice(&w.to_be_bytes());
            }
            if ln <= bytes.len() {
                Val::Text(String::from_utf8_lossy(&bytes[..ln]).into_owned())
            } else {
                Val::Null
            }
        } else {
            Val::Ints((0..n).map(|k| words[i + 2 + k]).collect())
        };
        out.push(Attr { hash: h, name: db1_dehash(h), typ, val });
        i += 2 + n;
    }
    out
}

/// Decode the DA/explicit list (which=1) following the cross-page node chain (§7.9).
fn decode_da_list(buf: &[u8], rec_off: usize, ps: usize) -> Vec<Attr> {
    let rec = |i: usize| be_u32(buf, rec_off + 4 * i);
    let w10 = rec(10);
    let (mut page, loc, words) = (rec(6), rec(7), ((w10 >> 14) & 0x3FFF) as i64);
    if words == 0 || page == 0 {
        return Vec::new();
    }
    let mut off = (loc >> 13) & 0xFFF;
    let mut payload: Vec<u32> = Vec::new();
    let mut remaining = words;
    let mut guard = 0;
    while page != 0 && remaining > 0 && guard < 128 {
        guard += 1;
        let node = page as usize * ps + off as usize * 4;
        if node + 20 > buf.len() {
            break;
        }
        let hdr = be_u32(buf, node);
        if ((hdr >> 16) & 0xF) != 1 {
            break;
        }
        let plen = (hdr & 0xFFFF) as i64 - 5;
        if plen <= 0 {
            break;
        }
        let avail = (buf.len().saturating_sub(node + 20) / 4) as i64;
        let take = plen.min(remaining).min(avail).max(0) as usize;
        for k in 0..take {
            payload.push(be_u32(buf, node + 20 + 4 * k));
        }
        remaining -= plen;
        page = be_u32(buf, node + 12);
        off = (be_u32(buf, node + 16) >> 13) & 0xFFF;
    }
    parse_attr_words(&payload, 512)
}

/// A single schema/template library (`*vir.dat`).
struct Schema {
    buf: Vec<u8>,
    index: HashMap<u32, (u32, u32)>,
}

impl Schema {
    fn open(path: &std::path::Path) -> std::io::Result<Schema> {
        let buf = fs::read(path)?;
        if buf.len() < 64 || be_u32(&buf, 0) != 6 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not a schema db"));
        }
        let hdr = rd_words(&buf, 0, 16);
        let count = hdr[5] as usize;
        let mut index = HashMap::new();
        if count > 0 && count < 100_000 {
            let tlu = chain(&buf, hdr[7], 7 * count);
            for i in 0..count {
                if 7 * i + 6 < tlu.len() {
                    let e = &tlu[7 * i..7 * i + 7];
                    index.insert(e[0], (e[1], e[2]));
                }
            }
        }
        Ok(Schema { buf, index })
    }

    fn typedef(&self, noun: u32) -> Option<HashMap<u32, Desc>> {
        let &(kp, kc) = self.index.get(&noun)?;
        let skel = chain(&self.buf, kp, kc as usize);
        if skel.len() < 10 {
            return None;
        }
        let dcount = skel[9] as usize;
        let mut out = HashMap::new();
        let mut i = 14usize;
        for _ in 0..dcount {
            if i + 8 >= skel.len() {
                break;
            }
            let (h, stride) = (skel[i], skel[i + 1]);
            out.insert(
                h,
                Desc {
                    typ: skel[i + 2],
                    size: skel[i + 3],
                    off: skel[i + 5] & 0xFFFFF,
                    bit: skel[i + 5] >> 20,
                    alt: skel[i + 8] & 0xFFFFF,
                    altbit: skel[i + 8] >> 20,
                },
            );
            if stride == 0 {
                break;
            }
            i += stride as usize;
        }
        Some(out)
    }
}

/// All schema libraries loaded from `%AVEVA_DESIGN_EXE%` (the `*vir.dat` files).
pub struct SchemaSet {
    schemas: Vec<Schema>,
    noun2: HashMap<u32, usize>,
}

impl SchemaSet {
    /// Load every `*vir.dat` in `dir` (mirrors db2_get_element_definition's registry).
    pub fn load(dir: &str) -> SchemaSet {
        let mut schemas = Vec::new();
        let mut noun2 = HashMap::new();
        let mut paths: Vec<_> = fs::read_dir(dir)
            .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
            .unwrap_or_else(|_| Vec::new());
        paths.sort();
        for p in paths {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.ends_with("vir.dat") {
                continue;
            }
            if let Ok(s) = Schema::open(&p) {
                if !s.index.is_empty() {
                    let idx = schemas.len();
                    for &noun in s.index.keys() {
                        noun2.entry(noun).or_insert(idx);
                    }
                    schemas.push(s);
                }
            }
        }
        SchemaSet { schemas, noun2 }
    }

    pub fn schema_count(&self) -> usize {
        self.schemas.len()
    }
    pub fn noun_count(&self) -> usize {
        self.noun2.len()
    }
    fn typedef(&self, noun: u32) -> Option<HashMap<u32, Desc>> {
        self.noun2.get(&noun).and_then(|&i| self.schemas[i].typedef(noun))
    }
}

/// An element database (design/catalogue `<proj><dbno>_0001`).
pub struct Edb {
    buf: Vec<u8>,
    ps: usize,
}

impl Edb {
    pub fn open(path: &str) -> std::io::Result<Edb> {
        Ok(Edb::from_bytes(fs::read(path)?))
    }
    pub fn from_bytes(buf: Vec<u8>) -> Edb {
        let mut ps = (be_u32(&buf, 0x34) as usize) * 4;
        if ![512usize, 2048, 4096].contains(&ps) {
            ps = 2048;
        }
        Edb { buf, ps }
    }
    pub fn page_size(&self) -> usize {
        self.ps
    }
    pub fn bytes(&self) -> &[u8] {
        &self.buf
    }
    fn u(&self, o: usize) -> u32 {
        be_u32(&self.buf, o)
    }
    fn is_index(&self, pg: usize) -> bool {
        pg > 0 && pg * self.ps + 8 <= self.buf.len() && self.u(pg * self.ps + 4) == INDEX_NOUN
    }
}

fn walk(db: &Edb, pg: usize, out: &mut Vec<(u32, u32, usize, u32)>, seen: &mut HashSet<usize>, max: usize) {
    if seen.contains(&pg) || out.len() >= max || !db.is_index(pg) {
        return;
    }
    seen.insert(pg);
    let base = pg * db.ps;
    let mut w = 0x1C;
    while w + 16 <= db.ps && out.len() < max {
        let (r0, r1) = (db.u(base + w), db.u(base + w + 4));
        let cpg = db.u(base + w + 8) as usize;
        let v = db.u(base + w + 12);
        w += 16;
        if r0 == 0 {
            break;
        }
        if r0 == 0x80000001 && r1 == 0x80000001 {
            continue;
        }
        let off = v >> 12;
        if off == 0 && db.is_index(cpg) {
            walk(db, cpg, out, seen, max);
        } else {
            out.push((r0, r1, cpg, off));
        }
    }
}

/// Fully decoded element.
pub struct Element {
    pub refno: (u32, u32),
    pub noun_name: String,
    pub owner: (u32, u32),
    pub name: Option<String>,
    pub implicit: Vec<Attr>,
    pub da: Vec<Attr>,
}

impl Element {
    /// POS (0x853B1) as a real triple, if present inline.
    pub fn pos(&self) -> Option<&[f64]> {
        self.implicit.iter().find(|a| a.hash == POS_HASH).and_then(|a| match &a.val {
            Val::Reals(v) => Some(v.as_slice()),
            _ => None,
        })
    }
}

fn decode_full(
    db: &Edb,
    ss: &SchemaSet,
    bo: usize,
    td_cache: &mut HashMap<u32, Option<HashMap<u32, Desc>>>,
) -> Element {
    let impl_words = (db.u(bo) & 0xFFFF) as usize;
    let noun = db.u(bo + 12);
    let mut implicit = Vec::new();
    if (8..=4096).contains(&impl_words) {
        let w = rd_words(&db.buf, bo, impl_words.min(256));
        if w.len() >= 11 {
            let td = td_cache.entry(noun).or_insert_with(|| ss.typedef(noun));
            if let Some(t) = td {
                let sel = (w[10] >> 29) & 1;
                for (&h, d) in t.iter() {
                    let off = if sel == 1 { d.alt } else { d.off };
                    if off == 0 {
                        continue;
                    }
                    let val = decode_one(&w, d);
                    if !val.is_null() {
                        implicit.push(Attr { hash: h, name: db1_dehash(h), typ: d.typ, val });
                    }
                }
            }
        }
    }
    let da = decode_da_list(&db.buf, bo, db.ps);
    let name = da.iter().find(|a| a.hash == NAME_HASH).and_then(|a| match &a.val {
        Val::Text(s) => Some(s.clone()),
        _ => None,
    });
    Element {
        refno: (db.u(bo + 4), db.u(bo + 8)),
        noun_name: db1_dehash(noun).trim().to_string(),
        owner: (db.u(bo + 16), db.u(bo + 20)),
        name,
        implicit,
        da,
    }
}

/// Walk the latest-session B-tree and decode every valid primary element.
/// When `collect` is false only the refmap (refno -> (noun,name)) is built.
pub fn index_db(
    db: &Edb,
    ss: &SchemaSet,
    collect: bool,
    refmap: &mut HashMap<(u32, u32), (String, String)>,
) -> Vec<Element> {
    let mut td_cache: HashMap<u32, Option<HashMap<u32, Desc>>> = HashMap::new();
    let latest = db.u(0x28) as usize;
    let root = db.u(latest * db.ps + 0x1C) as usize;
    let mut leaves = Vec::new();
    let mut visited = HashSet::new();
    walk(db, root, &mut leaves, &mut visited, 4_000_000); // generous cap; large dbs (ams1112) exceed 300k leaves
    let mut elems = Vec::new();
    let mut seen = HashSet::new();
    for (r0, r1, pg, off) in &leaves {
        if *off == 0 {
            continue;
        }
        let bo = pg * db.ps + (*off as usize) * 2;
        if bo + 44 > db.buf.len() {
            continue;
        }
        let w0 = db.u(bo);
        if (w0 >> 16) != 0 || !(8..=512).contains(&(w0 & 0xFFFF)) {
            continue;
        }
        if !looks_like_noun(db.u(bo + 12)) || !seen.insert((*r0, *r1)) {
            continue;
        }
        let el = decode_full(db, ss, bo, &mut td_cache);
        if el.noun_name.is_empty() {
            continue;
        }
        if let Some(nm) = &el.name {
            refmap.insert((*r0, *r1), (el.noun_name.clone(), nm.clone()));
        }
        if collect {
            elems.push(el);
        }
    }
    elems
}

/// Byte offset of the named element record in the latest session, or None.
pub fn find_record_offset(db: &Edb, ss: &SchemaSet, name: &str) -> Option<usize> {
    let latest = db.u(0x28) as usize;
    let root = db.u(latest * db.ps + 0x1C) as usize;
    let mut leaves = Vec::new();
    let mut visited = HashSet::new();
    walk(db, root, &mut leaves, &mut visited, 4_000_000); // generous cap; large dbs (ams1112) exceed 300k leaves
    let mut td_cache = HashMap::new();
    let mut seen = HashSet::new();
    for (r0, r1, pg, off) in &leaves {
        if *off == 0 {
            continue;
        }
        let bo = pg * db.ps + (*off as usize) * 2;
        if bo + 44 > db.buf.len() {
            continue;
        }
        let w0 = db.u(bo);
        if (w0 >> 16) != 0 || !(8..=512).contains(&(w0 & 0xFFFF)) {
            continue;
        }
        if !looks_like_noun(db.u(bo + 12)) || !seen.insert((*r0, *r1)) {
            continue;
        }
        if decode_full(db, ss, bo, &mut td_cache).name.as_deref() == Some(name) {
            return Some(bo);
        }
    }
    None
}

/// SAFE in-place edit of a **fixed-size inline (implicit)** attribute value (real/int/ref).
/// Returns the changed byte range `[start,end)`. Errors (never writes) if it would not be a
/// safe fixed-size inline edit: off==0, boolean, packed(sel=0) real, type/variant mismatch,
/// or a component-count change. NEVER touches record framing/length/keys, so it never needs
/// the B-tree (see format spec §12). Mirrors `docs/e3d 数据库分析/e3d_write.py`.
/// Always operate on a COPY of the db.
pub fn set_inline_value(
    buf: &mut [u8],
    ss: &SchemaSet,
    record_off: usize,
    attr_hash: u32,
    val: &Val,
) -> Result<(usize, usize), String> {
    let noun = be_u32(buf, record_off + 12);
    let td = ss.typedef(noun).ok_or("no typedef for noun")?;
    let d = *td.get(&attr_hash).ok_or("attr not in typedef")?;
    let sel = (be_u32(buf, record_off + 40) >> 29) & 1;
    let off = (if sel == 1 { d.alt } else { d.off }) as usize;
    if off == 0 {
        return Err("attribute not stored inline (off==0)".into());
    }
    if d.typ == 5 {
        return Err("boolean (bit-packed) write not supported".into());
    }
    let (cur_cnt, data_word) = if d.size > 1 {
        (be_u32(buf, record_off + 4 * off) as usize, off + 1)
    } else {
        (d.size as usize, off)
    };
    let start = record_off + 4 * data_word;
    fn put(buf: &mut [u8], byte: usize, w: u32) {
        buf[byte..byte + 4].copy_from_slice(&w.to_be_bytes());
    }
    match (d.typ, val) {
        (2 | 6, Val::Reals(vs)) => {
            if vs.len() != cur_cnt {
                return Err(format!("component count change {}->{}", cur_cnt, vs.len()));
            }
            if sel == 0 {
                return Err("packed(sel=0) real write unsupported".into());
            }
            for (j, &v) in vs.iter().enumerate() {
                let b = v.to_be_bytes(); // big-endian double = [hi32][lo32]
                put(buf, start + 8 * j, u32::from_be_bytes([b[4], b[5], b[6], b[7]])); // lo first
                put(buf, start + 8 * j + 4, u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
            }
            Ok((start, start + 8 * vs.len()))
        }
        (3 | 7, Val::Ints(vs)) => {
            if vs.len() != cur_cnt {
                return Err(format!("component count change {}->{}", cur_cnt, vs.len()));
            }
            for (j, &v) in vs.iter().enumerate() {
                put(buf, start + 4 * j, v);
            }
            Ok((start, start + 4 * vs.len()))
        }
        (4 | 8 | 16, Val::Refs(vs)) => {
            if vs.len() != cur_cnt {
                return Err(format!("component count change {}->{}", cur_cnt, vs.len()));
            }
            for (j, &(dbno, seq)) in vs.iter().enumerate() {
                put(buf, start + 8 * j, dbno);
                put(buf, start + 8 * j + 4, seq);
            }
            Ok((start, start + 8 * vs.len()))
        }
        _ => Err(format!("type {} / value variant not supported for inline write", d.typ)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dehash_roundtrip() {
        assert_eq!(db1_dehash(0x9C18E).trim(), "NAME");
        assert_eq!(db1_dehash(0x853B1).trim(), "POS");
        assert_eq!(db1_dehash(0x97247).trim(), "WELD");
        assert_eq!(db1_dehash(0x9CAF3).trim(), "PIPE");
    }

    #[test]
    fn uda_short_code() {
        assert!(dehash_uda_code(0x9C18E).is_none());
        assert!(dehash_uda_code(0x2C00D55A).unwrap().starts_with(':'));
    }

    const EXE: &str = r"D:\AVEVA\Everything3D2.10";
    const DBF: &str = r"D:\work\plant\pdms-io\pdms-test-data\sam7200_0001";

    fn data_present() -> bool {
        std::path::Path::new(&format!(r"{}\desvir.dat", EXE)).exists()
            && std::path::Path::new(DBF).exists()
    }

    #[test]
    fn read_counts_and_weld_pos() {
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let db = Edb::open(DBF).unwrap();
        let mut refmap = HashMap::new();
        let elems = index_db(&db, &ss, true, &mut refmap);
        assert_eq!(elems.len(), 6536);
        assert_eq!(elems.iter().filter(|e| e.name.is_some()).count(), 1144);
        let wb1 = elems.iter().find(|e| e.name.as_deref() == Some("/WB1")).unwrap();
        assert_eq!(wb1.pos(), Some(&[9630.0, 8072.0, 5282.5][..]));
    }

    #[test]
    fn inline_write_roundtrip() {
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let bytes = std::fs::read(DBF).unwrap();
        let off = find_record_offset(&Edb::from_bytes(bytes.clone()), &ss, "/WB1").expect("/WB1");
        let mut edited = bytes.clone();
        let new = vec![1000.25, -2000.5, 3000.75];
        let rng = set_inline_value(&mut edited, &ss, off, 0x853B1, &Val::Reals(new.clone())).unwrap();
        // every changed byte must lie inside the written value region (framing untouched)
        let confined = (0..bytes.len()).all(|i| bytes[i] == edited[i] || (rng.0 <= i && i < rng.1));
        assert!(confined, "edit escaped the value region");
        let db2 = Edb::from_bytes(edited);
        let el = decode_full(&db2, &ss, off, &mut HashMap::new());
        assert_eq!(el.pos(), Some(new.as_slice()));
        assert_eq!(el.name.as_deref(), Some("/WB1"));
    }

    #[test]
    fn inline_write_int_and_ref() {
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let bytes = std::fs::read(DBF).unwrap();
        let off = find_record_offset(&Edb::from_bytes(bytes.clone()), &ss, "/WB1").expect("/WB1");
        let el0 = decode_full(&Edb::from_bytes(bytes.clone()), &ss, off, &mut HashMap::new());
        let hash_of = |nm: &str| el0.implicit.iter().find(|a| a.name.trim() == nm).map(|a| a.hash);
        // INT attribute (WLDN, type 3)
        if let Some(h) = hash_of("WLDN") {
            let mut e = bytes.clone();
            set_inline_value(&mut e, &ss, off, h, &Val::Ints(vec![777])).unwrap();
            let d = decode_full(&Edb::from_bytes(e), &ss, off, &mut HashMap::new());
            assert_eq!(d.implicit.iter().find(|a| a.hash == h).unwrap().val, Val::Ints(vec![777]));
        }
        // REFERENCE attribute (SPRE, type 16) -> (dbno, refseq)
        if let Some(h) = hash_of("SPRE") {
            let mut e = bytes.clone();
            set_inline_value(&mut e, &ss, off, h, &Val::Refs(vec![(99, 12345)])).unwrap();
            let d = decode_full(&Edb::from_bytes(e), &ss, off, &mut HashMap::new());
            assert_eq!(d.implicit.iter().find(|a| a.hash == h).unwrap().val, Val::Refs(vec![(99, 12345)]));
        }
    }
}
