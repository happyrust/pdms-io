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

/// 页源抽象（spec 002 Phase 1）：`PageSource` trait + `InMemory`/`PagedFile` 双实现。
pub mod page_source;
/// 只读格式视图（spec 002 T103 读侧）：`Rdb<S: PageSource>` 导航 + 原始记录读取。
pub mod read_view;
use page_source::PageSource as _;

const SCHEMA_PAGE: usize = 2048;
const DATA_WORDS: usize = 511;
const BASE27: u32 = 0x81BF1;
const UDA_THRESHOLD: u32 = 0x171FAD39;
/// base-27 hash of the `NAME` attribute (DA-region text entry, type 15).
pub const NAME_HASH: u32 = 0x9C18E;
const INDEX_NOUN: u32 = 0xCC47DF;
/// base-27 hash of the `POS` attribute (implicit real triple).
pub const POS_HASH: u32 = 0x853B1;
const SENTINEL: u32 = 0x80000001; // B-tree leftmost separator key = -inf (leftmost spine only)

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
    /// 整文件 buffer 路径已下沉至 [`page_source::InMemory`]（spec 002 T102）：
    /// 页大小推断单源于 `page_source::page_size_from_header`，`Edb` 仅消费其
    /// 字节作为 COW 写所需的 flat buffer（行为零变化；短于头部的退化输入由
    /// 兜底 2048 取代旧实现的越界 panic）。
    pub fn from_bytes(buf: Vec<u8>) -> Edb {
        let src = page_source::InMemory::from_bytes(buf);
        let ps = src.page_size();
        Edb { buf: src.into_bytes(), ps }
    }
    pub fn page_size(&self) -> usize {
        self.ps
    }
    pub fn bytes(&self) -> &[u8] {
        &self.buf
    }
    /// Consume the db and return its (possibly COW-extended) byte buffer.
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
    fn u(&self, o: usize) -> u32 {
        be_u32(&self.buf, o)
    }
    fn is_index(&self, pg: usize) -> bool {
        pg > 0 && pg * self.ps + 8 <= self.buf.len() && self.u(pg * self.ps + 4) == INDEX_NOUN
    }
}

/// Enumerate B+-tree leaf entries (refno -> data location). Authoritative traversal
/// (matches db3_split_node/db3_change_table_entry): the entry count is bounded by the
/// page's word6 (free-word count), NOT null-terminated; and the leftmost child of every
/// internal node (the `0x80000001` sentinel separator = -inf) IS descended -- it holds
/// the smallest-key subtree (WORLD/SITE/ZONE/... low-refno elements). The earlier
/// null-terminated + sentinel-skipping walk under-counted by ~38% (sam7200 6536 vs the
/// correct 10392) while over-reading stale slots. See findings §16.
fn walk(db: &Edb, pg: usize, out: &mut Vec<(u32, u32, usize, u32)>, seen: &mut HashSet<usize>, max: usize) {
    if seen.contains(&pg) || out.len() >= max || !db.is_index(pg) {
        return;
    }
    seen.insert(pg);
    let base = pg * db.ps;
    let pw = db.ps / 4;
    let nent = (pw - 7 - db.u(base + 24) as usize) / 4; // word6 = free words -> entry count
    for k in 0..nent {
        if out.len() >= max {
            break;
        }
        let eo = base + (7 + 4 * k) * 4;
        let (r0, r1) = (db.u(eo), db.u(eo + 4));
        let cpg = db.u(eo + 8) as usize;
        let off = db.u(eo + 12) >> 12;
        if off == 0 && db.is_index(cpg) {
            walk(db, cpg, out, seen, max); // descend ALL children incl. sentinel leftmost
        } else if (r0, r1) != (0x80000001, 0x80000001) {
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

/// Resolve an element's reference attributes (implicit type 4/8/16 = `(dbno, refseq)`) to target
/// element names via a `refmap` (built by `index_db` over this db + any catalogue dbs). Returns
/// `(attr_name, [target_names])`; unresolved targets (e.g. an unloaded catalogue db) fall back to
/// the PDMS `=dbno/refseq` form. Recovers pipe connectivity (CREF/HREF/TREF) + catalogue links
/// (SPRE/ISPE -> SPCO). Port of `tools/e3d_decode_rs::resolve_refs`; see findings §8.12/§8.13.
///
/// To resolve across libraries, populate one `refmap` from every relevant db, e.g.:
/// ```ignore
/// let mut refmap = HashMap::new();
/// let elems = index_db(&design, &ss, true, &mut refmap);
/// index_db(&catalogue, &ss, false, &mut refmap); // merge catalogue refnos -> names
/// for el in &elems { let refs = resolve_refs(el, &refmap); /* ... */ }
/// ```
pub fn resolve_refs(el: &Element, refmap: &HashMap<(u32, u32), (String, String)>) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for a in &el.implicit {
        if !matches!(a.typ, 4 | 8 | 16) {
            continue;
        }
        if let Val::Refs(pairs) = &a.val {
            let mut tgts = Vec::new();
            for &(dbno, seq) in pairs {
                if (dbno, seq) == (0, 0) {
                    continue;
                }
                match refmap.get(&(dbno, seq)) {
                    Some((_, nm)) => tgts.push(nm.clone()),
                    None => tgts.push(format!("={}/{}", dbno, seq)),
                }
            }
            if !tgts.is_empty() {
                out.push((if a.name.is_empty() { format!("0x{:X}", a.hash) } else { a.name.clone() }, tgts));
            }
        }
    }
    out
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

// ─── Offline FULL write: copy-on-write + new-session commit (db5_save_work) ───────────────
//   Port of `docs/e3d 数据库分析/e3d_write_full.py` Slice 1 (inline value) + the shared
//   commit core. Never mutates an existing page: every changed page is appended (COW); only
//   page0's session pointer (0x28) is repointed at the very end, so prior sessions stay byte-
//   identical and fully readable (multi-version). See 格式规范 §12.4 / §12.6, findings §15.
//   Always operate on a COPY of the db (Edb owns its buffer; callers load a fresh Edb).

const SES_LAST: usize = 0x04; // session page: last_ses_pgno (link to previous session)
const SES_SESNO: usize = 0x0C; // session number
const SES_END: usize = 0x14; // end / last allocated page
const SES_ROOT: usize = 0x1C; // index_root_pgno
const HDR_LATEST: usize = 0x28; // page0: latest session pgno (word10)

fn put_u32(buf: &mut [u8], o: usize, w: u32) {
    buf[o..o + 4].copy_from_slice(&w.to_be_bytes());
}

impl Edb {
    fn n_pages(&self) -> usize {
        self.buf.len() / self.ps
    }
    fn latest_ses(&self) -> usize {
        self.u(HDR_LATEST) as usize
    }
    fn root_of_session(&self, ses_pg: usize) -> usize {
        self.u(ses_pg * self.ps + SES_ROOT) as usize
    }
    /// Index root of the latest session.
    pub fn latest_root(&self) -> usize {
        self.root_of_session(self.latest_ses())
    }
    fn append_page(&mut self, page: &[u8]) -> usize {
        debug_assert_eq!(page.len(), self.ps);
        let pg = self.buf.len() / self.ps;
        self.buf.extend_from_slice(page);
        pg
    }
    fn sesno_of(&self, ses_pg: usize) -> u32 {
        self.u(ses_pg * self.ps + SES_SESNO)
    }
    /// Collapse a batch into ONE session: append a session page (cloned from `base_ses`) with
    /// `sesno = base_sesno + 1`, `root = final_root`, linked to `base_ses`, and repoint page0.
    /// The per-edit intermediate sessions appended during the batch become unreferenced orphans.
    fn collapse_session(&mut self, base_ses: usize, base_sesno: u32, final_root: usize) -> u32 {
        let ps = self.ps;
        let mut ses = self.buf[base_ses * ps..(base_ses + 1) * ps].to_vec();
        put_u32(&mut ses, SES_SESNO, base_sesno + 1);
        put_u32(&mut ses, SES_ROOT, final_root as u32);
        put_u32(&mut ses, SES_LAST, base_ses as u32);
        let new_ses_pg = self.append_page(&ses);
        put_u32(&mut self.buf, new_ses_pg * ps + SES_END, new_ses_pg as u32);
        put_u32(&mut self.buf, HDR_LATEST, new_ses_pg as u32);
        base_sesno + 1
    }
}

/// Session index roots newest-first (latest -> last_ses -> ...), for multi-version read-back.
pub fn session_roots(db: &Edb) -> Vec<usize> {
    let (mut out, mut seen) = (Vec::new(), HashSet::new());
    let (mut pg, np) = (db.latest_ses(), db.n_pages());
    while pg != 0 && pg < np && seen.insert(pg) && out.len() < 256 {
        out.push(db.root_of_session(pg));
        pg = be_u32(&db.buf, pg * db.ps + SES_LAST) as usize;
    }
    out
}

/// Main-record byte offset for `refno` reachable from a specific session `root` (or None).
/// Filters to the clean primary record (impl-count word0 + dehashable noun), as the reader does.
pub fn record_off_via_root(db: &Edb, root: usize, refno: (u32, u32)) -> Option<usize> {
    let (mut leaves, mut seen) = (Vec::new(), HashSet::new());
    walk(db, root, &mut leaves, &mut seen, 4_000_000);
    for (r0, r1, pg, off) in &leaves {
        if (*r0, *r1) != refno || *off == 0 {
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
        if looks_like_noun(db.u(bo + 12)) {
            return Some(bo);
        }
    }
    None
}

/// Path (root->leaf) of `(index_pgno, entry_byte_off)` to the leaf entry whose child page+off
/// equal `(data_pg, data_off)`. Mirrors `find_leaf_path` (match by exact data location).
fn find_leaf_path_by_loc(db: &Edb, root: usize, data_pg: usize, data_off: u32) -> Option<Vec<(usize, usize)>> {
    fn rec(db: &Edb, pg: usize, depth: usize, dpg: usize, doff: u32, chain: &mut Vec<(usize, usize)>) -> bool {
        if !db.is_index(pg) || depth > 40 {
            return false;
        }
        let base = pg * db.ps;
        let pw = db.ps / 4;
        let nent = (pw - 7 - db.u(base + 24) as usize) / 4;
        for k in 0..nent {
            let eo = base + (7 + 4 * k) * 4;
            let cpg = db.u(eo + 8) as usize;
            let off = db.u(eo + 12) >> 12;
            if off == 0 && db.is_index(cpg) {
                chain.push((pg, eo));
                if rec(db, cpg, depth + 1, dpg, doff, chain) {
                    return true;
                }
                chain.pop();
            } else if cpg == dpg && off == doff {
                chain.push((pg, eo));
                return true;
            }
        }
        false
    }
    let mut chain = Vec::new();
    if rec(db, root, 0, data_pg, data_off, &mut chain) {
        Some(chain)
    } else {
        None
    }
}

/// Report from a COW commit.
pub struct CowReport {
    pub refno: (u32, u32),
    pub old_root: usize,
    pub new_root: usize,
    pub data_pg_old: usize,
    pub data_pg_new: usize,
    pub old_sesno: u32,
    pub new_sesno: u32,
    pub new_ses_pg: usize,
    pub da_page_new: usize, // DA/member relocation (Slice 2/6/7/8): fresh list page, 0 for inline (S1)
    pub da_nodes: usize,    // DA/member relocation: number of nodes in the emitted chain
    pub tree_grew: bool,    // B-tree insert (Slice 5): the tree gained a level (root split)
}

/// Shared commit core (`db5_save_work`): append the already-edited data page, COW the B-tree
/// path root->leaf to point at it, append a new session (sesno+1) and repoint page0. Mirrors
/// `e3d_write_full.py::_commit_edited_data_page`.
fn commit_edited_data_page(db: &mut Edb, record_off: usize, edited_page: &[u8]) -> Result<CowReport, String> {
    let ps = db.ps;
    if edited_page.len() != ps {
        return Err("edited page wrong size".into());
    }
    let old_ses_pg = db.latest_ses();
    let old_root = db.root_of_session(old_ses_pg);
    let data_pg = record_off / ps;
    let data_off = ((record_off % ps) / 2) as u32;
    let refno = (be_u32(&db.buf, record_off + 4), be_u32(&db.buf, record_off + 8));
    let path = find_leaf_path_by_loc(db, old_root, data_pg, data_off)
        .ok_or_else(|| format!("leaf entry for record @{} not found under root {}", record_off, old_root))?;

    let new_data_pg = db.append_page(edited_page);
    // self-contained: in-page DA(word6)/member(word8) pointers that referenced the old page
    let rip = record_off % ps;
    for fld in [6usize, 8] {
        let at = new_data_pg * ps + rip + 4 * fld;
        if be_u32(&db.buf, at) as usize == data_pg {
            put_u32(&mut db.buf, at, new_data_pg as u32);
        }
    }
    // COW the B-tree path bottom-up (rewire each child pointer; siblings shared)
    let (mut child_old, mut child_new) = (data_pg, new_data_pg);
    for &(idx_pg, entry_byte_off) in path.iter().rev() {
        let mut page = db.buf[idx_pg * ps..(idx_pg + 1) * ps].to_vec();
        let e = entry_byte_off - idx_pg * ps;
        let cur = be_u32(&page, e + 8) as usize;
        if cur != child_old {
            return Err(format!("path inconsistency: entry child {} != {}", cur, child_old));
        }
        put_u32(&mut page, e + 8, child_new as u32);
        let np = db.append_page(&page);
        child_old = idx_pg;
        child_new = np;
    }
    let new_root = child_new;
    let (old_sesno, new_sesno, new_ses_pg) = append_session(db, new_root);
    Ok(CowReport {
        refno,
        old_root,
        new_root,
        data_pg_old: data_pg,
        data_pg_new: new_data_pg,
        old_sesno,
        new_sesno,
        new_ses_pg,
        da_page_new: 0,
        da_nodes: 0,
        tree_grew: false,
    })
}

/// Append a new session page (clone of the latest) with sesno+1, the given index root, linked
/// to the previous session, and repoint page0. Returns `(old_sesno, new_sesno, new_ses_pg)`.
/// Shared by every commit (`db5_save_work` tail). Mirrors `e3d_write_full.py::_append_session`.
fn append_session(db: &mut Edb, new_root: usize) -> (u32, u32, usize) {
    let ps = db.ps;
    let old_ses_pg = db.latest_ses();
    let mut new_ses = db.buf[old_ses_pg * ps..(old_ses_pg + 1) * ps].to_vec();
    let old_sesno = be_u32(&new_ses, SES_SESNO);
    put_u32(&mut new_ses, SES_SESNO, old_sesno + 1);
    put_u32(&mut new_ses, SES_ROOT, new_root as u32);
    put_u32(&mut new_ses, SES_LAST, old_ses_pg as u32);
    let new_ses_pg = db.append_page(&new_ses);
    put_u32(&mut db.buf, new_ses_pg * ps + SES_END, new_ses_pg as u32);
    put_u32(&mut db.buf, HDR_LATEST, new_ses_pg as u32); // repoint page0 to the new session
    (old_sesno, old_sesno + 1, new_ses_pg)
}

/// Slice 1 (Rust): COW-commit one fixed-size inline (implicit) value edit (real/int/ref, same
/// component count) at the main record `record_off`. Appends a new session; prior sessions keep
/// the old value. Mirrors `e3d_write_full.py::cow_commit`. Operate on a fresh (owned) Edb.
pub fn cow_commit_inline(db: &mut Edb, ss: &SchemaSet, record_off: usize, attr_hash: u32, val: &Val)
    -> Result<CowReport, String> {
    let ps = db.ps;
    let data_pg = record_off / ps;
    let mut page = db.buf[data_pg * ps..(data_pg + 1) * ps].to_vec();
    let rip = record_off % ps;
    let rng = set_inline_value(&mut page, ss, rip, attr_hash, val)?;
    if rng.1 > ps {
        return Err("value crosses a page boundary (needs multi-page COW)".into());
    }
    commit_edited_data_page(db, record_off, &page)
}

// ─── DA-region rewrite via multi-page COW relocation (Slice 2/6 text, Slice 8 UDA) ─────────
//   The DA/explicit list is a type-1 node CHAIN (rec[6]=page, off=(rec[7]>>13)&0xFFF, total
//   payload=(rec[10]>>14)&0x3FFF); each node = [w0=(payload+5)|type<<16][w1..2=refno][w3=next
//   page][w4=(next off)<<13][payload]. The reader concatenates payloads then parses entries.
//   We read it chain-aware, edit, re-emit a fresh node chain on appended page(s), repoint
//   rec[6]/rec[7]/rec[10], and reuse the commit core. UDA values are ordinary DA entries
//   (hash > UDA_THRESHOLD). Port of e3d_write_full.py Slices 6/8.

/// Raw (unparsed) list payload across the node chain (which=1 DA, 2 members).
fn list_payload_words(buf: &[u8], rec_off: usize, ps: usize, which: u32) -> Vec<u32> {
    let rec = |i: usize| be_u32(buf, rec_off + 4 * i);
    let w10 = rec(10);
    let (mut page, loc, words) = if which == 1 {
        (rec(6), rec(7), ((w10 >> 14) & 0x3FFF) as i64)
    } else {
        (rec(8), rec(9), (w10 & 0x3FFF) as i64)
    };
    if words == 0 || page == 0 {
        return Vec::new();
    }
    let mut off = (loc >> 13) & 0xFFF;
    let (mut payload, mut remaining, mut guard) = (Vec::new(), words, 0);
    while page != 0 && remaining > 0 && guard < 128 {
        guard += 1;
        let node = page as usize * ps + off as usize * 4;
        if node + 20 > buf.len() {
            break;
        }
        let hdr = be_u32(buf, node);
        if ((hdr >> 16) & 0xF) != which {
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
    payload
}

/// Encode a DA/explicit text value: ctrl=(typ<<26)|len, value=[byte_len][UTF-8 4/word MSB-first].
fn pack_text(typ: u32, text: &str) -> (u32, Vec<u32>) {
    let sb = text.as_bytes();
    let mut padded = sb.to_vec();
    while padded.len() % 4 != 0 {
        padded.push(0);
    }
    let mut value = vec![sb.len() as u32];
    for k in 0..padded.len() / 4 {
        value.push(u32::from_be_bytes([padded[4 * k], padded[4 * k + 1], padded[4 * k + 2], padded[4 * k + 3]]));
    }
    let ctrl = (typ << 26) | (value.len() as u32 & 0x3FFFFFF);
    (ctrl, value)
}

/// Locate the `[hash][ctrl][value]` entry for `attr_hash`: (start, end, type) or None.
fn find_entry_span(payload: &[u32], attr_hash: u32) -> Option<(usize, usize, u32)> {
    let mut i = 0;
    while i + 1 < payload.len() {
        let (h, ctrl) = (payload[i], payload[i + 1]);
        let n = (ctrl & 0x3FFFFFF) as usize;
        if h == 0 || n == 0 || n > 256 || i + 2 + n > payload.len() {
            break;
        }
        if h == attr_hash {
            return Some((i, i + 2 + n, ctrl >> 26));
        }
        i += 2 + n;
    }
    None
}

/// End index of the valid entry run (insertion point for a new entry).
fn entries_end(payload: &[u32]) -> usize {
    let mut i = 0;
    while i + 1 < payload.len() {
        let (h, ctrl) = (payload[i], payload[i + 1]);
        let n = (ctrl & 0x3FFFFFF) as usize;
        if h == 0 || n == 0 || n > 256 || i + 2 + n > payload.len() {
            break;
        }
        i += 2 + n;
    }
    i
}

/// Replace (or append if absent) the `[hash][ctrl][value]` entry; other entries kept verbatim.
fn set_entry_in_payload(payload: &[u32], attr_hash: u32, ctrl: u32, value: &[u32]) -> Vec<u32> {
    let mut entry = vec![attr_hash, ctrl];
    entry.extend_from_slice(value);
    let mut out = Vec::new();
    if let Some((s, e, _)) = find_entry_span(payload, attr_hash) {
        out.extend_from_slice(&payload[..s]);
        out.extend_from_slice(&entry);
        out.extend_from_slice(&payload[e..]);
    } else {
        let i = entries_end(payload);
        out.extend_from_slice(&payload[..i]);
        out.extend_from_slice(&entry);
        out.extend_from_slice(&payload[i..]);
    }
    out
}

/// Remove the entry for `attr_hash` (Err if absent).
fn remove_entry_in_payload(payload: &[u32], attr_hash: u32) -> Result<Vec<u32>, String> {
    match find_entry_span(payload, attr_hash) {
        Some((s, e, _)) => {
            let mut out = Vec::new();
            out.extend_from_slice(&payload[..s]);
            out.extend_from_slice(&payload[e..]);
            Ok(out)
        }
        None => Err(format!("entry 0x{:X} not found in DA region", attr_hash)),
    }
}

/// Append fresh page(s) holding `payload` as a `list_type` node chain (1=DA, 2=members).
/// Returns (first_page, first_off_words, total_payload_words, n_nodes).
fn emit_node_chain(db: &mut Edb, header7: &[u8], payload: &[u32], list_type: u32, refno: (u32, u32),
                   force_chunk: Option<usize>) -> (usize, usize, usize, usize) {
    let ps = db.ps;
    let pw = ps / 4;
    let offw = 7usize;
    let cap = pw - offw - 5;
    let chunk = force_chunk.map(|c| c.clamp(1, cap)).unwrap_or(cap);
    let groups: Vec<Vec<u32>> = if payload.is_empty() {
        vec![Vec::new()]
    } else {
        payload.chunks(chunk).map(|c| c.to_vec()).collect()
    };
    let mut pages = Vec::new();
    for g in &groups {
        let mut page = vec![0u8; ps];
        page[..28].copy_from_slice(&header7[..28]);
        put_u32(&mut page, offw * 4, (g.len() as u32 + 5) | (list_type << 16));
        put_u32(&mut page, (offw + 1) * 4, refno.0);
        put_u32(&mut page, (offw + 2) * 4, refno.1);
        for (k, &wv) in g.iter().enumerate() {
            put_u32(&mut page, (offw + 5 + k) * 4, wv);
        }
        pages.push(db.append_page(&page));
    }
    for w in 0..pages.len().saturating_sub(1) {
        let base = pages[w] * ps;
        put_u32(&mut db.buf, base + (offw + 3) * 4, pages[w + 1] as u32);
        put_u32(&mut db.buf, base + (offw + 4) * 4, ((offw as u32) & 0xFFF) << 13);
    }
    (pages[0], offw, payload.len(), pages.len())
}

/// Re-emit `new_payload` as a fresh DA (type-1) node chain on appended page(s), repoint the
/// record's rec[6]/rec[7]/rec[10] DA bits, then COW the record page + B-tree + new session.
fn relocate_da_payload(db: &mut Edb, record_off: usize, new_payload: &[u32], force_chunk: Option<usize>)
    -> Result<CowReport, String> {
    let ps = db.ps;
    let data_pg = record_off / ps;
    let refno = (be_u32(&db.buf, record_off + 4), be_u32(&db.buf, record_off + 8));
    let header7 = db.buf[data_pg * ps..data_pg * ps + 28].to_vec();
    let (da_pg, da_off, total, n_nodes) = emit_node_chain(db, &header7, new_payload, 1, refno, force_chunk);
    let mut page = db.buf[data_pg * ps..(data_pg + 1) * ps].to_vec();
    let rip = record_off % ps;
    let rec7 = be_u32(&page, rip + 7 * 4);
    put_u32(&mut page, rip + 6 * 4, da_pg as u32);
    put_u32(&mut page, rip + 7 * 4, (rec7 & !(0xFFF << 13)) | (((da_off as u32) & 0xFFF) << 13));
    let rec10 = be_u32(&page, rip + 10 * 4);
    put_u32(&mut page, rip + 10 * 4, (rec10 & !(0x3FFF << 14)) | (((total as u32) & 0x3FFF) << 14));
    let mut rep = commit_edited_data_page(db, record_off, &page)?;
    rep.da_page_new = da_pg;
    rep.da_nodes = n_nodes;
    Ok(rep)
}

/// Slice 2/6 (Rust): COW-commit a DA/explicit TEXT edit (e.g. element NAME) by relocating the
/// DA list onto fresh page(s). Handles cross-page / chained / growth. `force_chunk` forces a
/// multi-node chain (test the cross-page chain). Port of `cow_commit_da_text(_xpage)`.
pub fn cow_commit_da_text(db: &mut Edb, record_off: usize, attr_hash: u32, new_text: &str,
                          force_chunk: Option<usize>) -> Result<CowReport, String> {
    let payload = list_payload_words(&db.buf, record_off, db.ps, 1);
    if payload.is_empty() {
        return Err("record has no DA list to edit".into());
    }
    let span = find_entry_span(&payload, attr_hash).ok_or("attr not found in DA region")?;
    if !matches!(span.2, 10 | 14 | 15) {
        return Err(format!("DA attr type {} is not text", span.2));
    }
    let (ctrl, value) = pack_text(span.2, new_text);
    let new_payload = set_entry_in_payload(&payload, attr_hash, ctrl, &value);
    relocate_da_payload(db, record_off, &new_payload, force_chunk)
}

/// The element's UDA entries `(hash, type, value words)` from the DA region (hash > threshold).
pub fn read_uda(buf: &[u8], record_off: usize, ps: usize) -> Vec<(u32, u32, Vec<u32>)> {
    let payload = list_payload_words(buf, record_off, ps, 1);
    let (mut out, mut i) = (Vec::new(), 0usize);
    while i + 1 < payload.len() {
        let (h, ctrl) = (payload[i], payload[i + 1]);
        let n = (ctrl & 0x3FFFFFF) as usize;
        if h == 0 || n == 0 || n > 256 || i + 2 + n > payload.len() {
            break;
        }
        if h > UDA_THRESHOLD {
            out.push((h, ctrl >> 26, payload[i + 2..i + 2 + n].to_vec()));
        }
        i += 2 + n;
    }
    out
}

/// Slice 8 (Rust): set (or add) a DA-region entry value by hash (generic; motivating use = UDA
/// strongly-typed values). `value_words` = raw value words. Reuses the DA relocation core.
pub fn cow_da_set_entry(db: &mut Edb, record_off: usize, attr_hash: u32, type_code: u32,
                        value_words: &[u32], force_chunk: Option<usize>) -> Result<CowReport, String> {
    let payload = list_payload_words(&db.buf, record_off, db.ps, 1);
    if payload.is_empty() {
        return Err("record has no DA region".into());
    }
    let ctrl = (type_code << 26) | (value_words.len() as u32 & 0x3FFFFFF);
    let new_payload = set_entry_in_payload(&payload, attr_hash, ctrl, value_words);
    relocate_da_payload(db, record_off, &new_payload, force_chunk)
}

/// Slice 8 (Rust): remove a DA-region entry (e.g. a UDA) by hash, via DA relocation.
pub fn cow_da_remove_entry(db: &mut Edb, record_off: usize, attr_hash: u32, force_chunk: Option<usize>)
    -> Result<CowReport, String> {
    let payload = list_payload_words(&db.buf, record_off, db.ps, 1);
    let new_payload = remove_entry_in_payload(&payload, attr_hash)?;
    relocate_da_payload(db, record_off, &new_payload, force_chunk)
}

// ─── Slice 7 (Rust): member-list (child refno) rewrite via type-2 node-chain relocation ──────
//   Members are a type-2 node chain (parallel to DA's type-1): rec[8]=page, off=(rec[9]>>13)&
//   0xFFF, member words=rec[10]&0x3FFF; payload = flat (r0,r1) CHILD-REFNO pairs. We reuse the
//   shared `emit_node_chain` + `commit_edited_data_page`. Port of e3d_write_full.py Slice 7.

/// The element's MEMBER (child) refno list (chain-aware type-2 node; payload = flat (r0,r1)
/// pairs). Every listed child's owner refno == this element's refno (validated on sam7200).
pub fn read_members(buf: &[u8], record_off: usize, ps: usize) -> Vec<(u32, u32)> {
    let p = list_payload_words(buf, record_off, ps, 2);
    (0..p.len().saturating_sub(1)).step_by(2).map(|i| (p[i], p[i + 1])).collect()
}

/// Slice 7 (Rust): set the element's MEMBER (child refno) list to `children` by re-emitting the
/// type-2 node chain onto fresh page(s) (multi-page COW). Covers relocate/add/remove uniformly;
/// repoints rec[8]/rec[9] + rec[10] member-word bits, then COWs the record page + B-tree + a new
/// session. Empty list => page 0 / 0 words. Mirrors `e3d_write_full.py::cow_members_set`.
pub fn cow_members_set(db: &mut Edb, record_off: usize, children: &[(u32, u32)], force_chunk: Option<usize>)
    -> Result<CowReport, String> {
    let ps = db.ps;
    let data_pg = record_off / ps;
    let refno = (be_u32(&db.buf, record_off + 4), be_u32(&db.buf, record_off + 8));
    let mut payload = Vec::with_capacity(children.len() * 2);
    for &(a, b) in children {
        payload.push(a);
        payload.push(b);
    }
    let (mem_pg, mem_off, total, n_nodes) = if payload.is_empty() {
        (0usize, 0usize, 0usize, 0usize)
    } else {
        let header7 = db.buf[data_pg * ps..data_pg * ps + 28].to_vec();
        emit_node_chain(db, &header7, &payload, 2, refno, force_chunk)
    };
    let mut page = db.buf[data_pg * ps..(data_pg + 1) * ps].to_vec();
    let rip = record_off % ps;
    let rec9 = be_u32(&page, rip + 9 * 4);
    put_u32(&mut page, rip + 8 * 4, mem_pg as u32);
    put_u32(&mut page, rip + 9 * 4, (rec9 & !(0xFFF << 13)) | (((mem_off as u32) & 0xFFF) << 13));
    let rec10 = be_u32(&page, rip + 10 * 4);
    put_u32(&mut page, rip + 10 * 4, (rec10 & !0x3FFF) | ((total as u32) & 0x3FFF)); // member words (low 14)
    let mut rep = commit_edited_data_page(db, record_off, &page)?;
    rep.da_page_new = mem_pg; // here = relocated member page
    rep.da_nodes = n_nodes;
    Ok(rep)
}

// ─── Slice 3/5 (Rust): element insert with B+-tree key insert + node split/grow ──────────────
//   db3_change_table_entry (3.3.1) recursive insert; db3_split_node (3.2.6) split ~ at the word
//   midpoint (entry boundary), promoting the upper half's min key; db3_split_root (3.2.7) grows
//   a new root on root overflow (entry0 = sentinel -inf -> old root, entry1 = sep -> sibling).
//   B+-tree: data only in leaves (off!=0); internal nodes hold separator keys (= subtree min)
//   + child pointers (off==0). Port of e3d_write_full.py Slices 3/5.

/// word6-bounded entries `[r0,r1,cpg,v]` of index page `pg` + its level (word2). Reads from
/// `db.buf` (incl. freshly-appended pages). See findings §16 (word6, not null-terminated).
fn idx_entries(db: &Edb, pg: usize) -> (Vec<[u32; 4]>, u32) {
    let base = pg * db.ps;
    let pw = db.ps / 4;
    let level = db.u(base + 8); // word2 = B-tree level (0 = leaf)
    let nent = (pw - 7 - db.u(base + 24) as usize) / 4; // word6 = free words
    let mut out = Vec::with_capacity(nent);
    for k in 0..nent {
        let wo = base + (7 + 4 * k) * 4;
        out.push([db.u(wo), db.u(wo + 4), db.u(wo + 8), db.u(wo + 12)]);
    }
    (out, level)
}

/// Append a new index page (header cloned from `template_pg`, word2:=level, word6:=free,
/// entries from word7, tail zeroed). Returns the new page no.
fn emit_index_page(db: &mut Edb, template_pg: usize, level: u32, entries: &[[u32; 4]]) -> Result<usize, String> {
    let ps = db.ps;
    let pw = ps / 4;
    if 4 * entries.len() > pw - 7 {
        return Err(format!("index page overflow: {} entries > capacity {}", entries.len(), (pw - 7) / 4));
    }
    let mut page = db.buf[template_pg * ps..(template_pg + 1) * ps].to_vec();
    put_u32(&mut page, 8, level);
    put_u32(&mut page, 24, (pw - 7 - 4 * entries.len()) as u32);
    for w in 7..pw {
        put_u32(&mut page, w * 4, 0);
    }
    for (i, e) in entries.iter().enumerate() {
        let eo = (7 + 4 * i) * 4;
        put_u32(&mut page, eo, e[0]);
        put_u32(&mut page, eo + 4, e[1]);
        put_u32(&mut page, eo + 8, e[2]);
        put_u32(&mut page, eo + 12, e[3]);
    }
    Ok(db.append_page(&page))
}

/// `a <= b` with the 0x80000001 sentinel treated as -inf (leftmost separator).
fn key_le(a: (u32, u32), b: (u32, u32)) -> bool {
    if a == (SENTINEL, SENTINEL) {
        return true;
    }
    if b == (SENTINEL, SENTINEL) {
        return false;
    }
    a <= b
}

/// Insert `key -> (data_pg, v)` into the B+-subtree at index page `pg` (COW, level-driven).
/// `cap` = max entries per node before splitting. Returns `(new_pg, split)` where `split` is
/// `Some((sep_key, new_sibling_pg))` for the parent to absorb. Mirrors db3_change_table_entry.
fn btree_insert(db: &mut Edb, pg: usize, key: (u32, u32), leaf_data: (u32, u32), cap: usize)
    -> Result<(usize, Option<((u32, u32), usize)>), String> {
    let (entries, level) = idx_entries(db, pg);
    let ne: Vec<[u32; 4]> = if level == 0 {
        // leaf: sorted insert (reject duplicate)
        let mut pos = entries.len();
        for (i, e) in entries.iter().enumerate() {
            if (e[0], e[1]) == key {
                return Err(format!("duplicate key (0x{:X},0x{:X}) already present", key.0, key.1));
            }
            if !key_le((e[0], e[1]), key) {
                pos = i;
                break;
            }
        }
        let mut v = Vec::with_capacity(entries.len() + 1);
        v.extend_from_slice(&entries[..pos]);
        v.push([key.0, key.1, leaf_data.0, leaf_data.1]);
        v.extend_from_slice(&entries[pos..]);
        v
    } else {
        // internal: descend (rightmost separator <= key), rewire COW'd child, absorb split
        let mut ci = 0;
        for (i, e) in entries.iter().enumerate() {
            if key_le((e[0], e[1]), key) {
                ci = i;
            } else {
                break;
            }
        }
        let child_pg = entries[ci][2] as usize;
        let (new_child, csplit) = btree_insert(db, child_pg, key, leaf_data, cap)?;
        let mut v: Vec<[u32; 4]> = entries.clone();
        v[ci][2] = new_child as u32;
        if let Some(((s0, s1), sib)) = csplit {
            v.insert(ci + 1, [s0, s1, sib as u32, 1]); // separator -> new sibling, after ci
        }
        v
    };
    if ne.len() <= cap {
        return Ok((emit_index_page(db, pg, level, &ne)?, None));
    }
    // overflow -> split ~ in half at an entry boundary; separator = min key of upper subtree
    let l = (ne.len() + 1) / 2;
    let sep = (ne[l][0], ne[l][1]);
    let lower = emit_index_page(db, pg, level, &ne[..l])?;
    let upper = emit_index_page(db, pg, level, &ne[l..])?;
    Ok((lower, Some((sep, upper))))
}

/// Insert one leaf entry (refno `key` -> element at `data_pg`/`off` words) into the B+-tree at
/// `root` (COW), splitting recursively and growing a new root on overflow. Returns `(new_root,
/// grew)`. Does NOT append a session (callers may chain inserts). Mirrors `cow_insert_leaf`.
fn cow_insert_leaf(db: &mut Edb, root: usize, key: (u32, u32), data_pg: usize, off: u32, cap: Option<usize>)
    -> Result<(usize, bool), String> {
    let pw = db.ps / 4;
    let cap = cap.unwrap_or((pw - 7) / 4);
    let (_, root_level) = idx_entries(db, root);
    let leaf_data = (data_pg as u32, (off << 12) | 1);
    let (new_root, split) = btree_insert(db, root, key, leaf_data, cap)?;
    match split {
        None => Ok((new_root, false)),
        Some(((s0, s1), sib)) => {
            let nr = emit_index_page(db, root, root_level + 1,
                &[[SENTINEL, SENTINEL, new_root as u32, 1], [s0, s1, sib as u32, 1]])?;
            Ok((nr, true))
        }
    }
}

/// Navigate to the rightmost (max-key) leaf. Returns `(leaf_pgno, ancestors)` where each
/// ancestor = `(index_pgno, rightmost_internal_entry_word_index)`. Mirrors `_rightmost_path`.
fn rightmost_path(db: &Edb, root: usize) -> Result<(usize, Vec<(usize, usize)>), String> {
    let pw = db.ps / 4;
    let mut pg = root;
    let mut anc = Vec::new();
    for _ in 0..40 {
        let base = pg * db.ps;
        let nent = (pw - 7 - db.u(base + 24) as usize) / 4;
        let mut rightmost: Option<(usize, usize)> = None; // (word_index, child_pg)
        for k in 0..nent {
            let wo = 7 + 4 * k;
            let cpg = db.u(base + wo * 4 + 8) as usize;
            let v = db.u(base + wo * 4 + 12);
            if (v >> 12) == 0 && db.is_index(cpg) {
                rightmost = Some((wo, cpg));
            }
        }
        match rightmost {
            None => return Ok((pg, anc)), // leaf
            Some((wo, cpg)) => {
                anc.push((pg, wo));
                pg = cpg;
            }
        }
    }
    Err("B-tree too deep / cycle".into())
}

/// Append a COW clone of the source element's data page, re-stamped with `new_refno` and
/// `new_name` (DA NAME rewrite, same-page DA only), made self-contained (in-page DA/member
/// pointers repointed). Returns `(new_data_pg, src_off_words, da_old, da_new)`.
fn clone_element_page(db: &mut Edb, src_record_off: usize, new_refno: (u32, u32), new_name: &str)
    -> Result<(usize, u32, usize, usize), String> {
    let ps = db.ps;
    let pw = ps / 4;
    let src_pg = src_record_off / ps;
    let rip = src_record_off % ps;
    if be_u32(&db.buf, src_record_off + 24) as usize != src_pg {
        return Err("source element DA not on its own page (clone unsupported)".into());
    }
    let mut page = db.buf[src_pg * ps..(src_pg + 1) * ps].to_vec();
    put_u32(&mut page, rip + 4, new_refno.0);
    put_u32(&mut page, rip + 8, new_refno.1);
    let (da_old, da_new) = rewrite_da_text_in_page(&mut page, rip, NAME_HASH, new_name, pw)?;
    let new_data_pg = db.append_page(&page);
    for fld in [6usize, 8] {
        let at = new_data_pg * ps + rip + 4 * fld;
        if be_u32(&db.buf, at) as usize == src_pg {
            put_u32(&mut db.buf, at, new_data_pg as u32);
        }
    }
    Ok((new_data_pg, (rip / 2) as u32, da_old, da_new))
}

/// In-page rebuild of a record's single same-page DA node: replace `attr_hash`'s text value
/// with `new_text` (updates the node header word0 + record rec[10] DA-count). Same-page,
/// single-node, growth-bounded. Returns `(da_words_old, da_words_new)`. Mirrors
/// `_rewrite_da_text_in_page` (used by the clone path; the general path is relocation).
fn rewrite_da_text_in_page(page: &mut [u8], rip: usize, attr_hash: u32, new_text: &str, pw: usize)
    -> Result<(usize, usize), String> {
    let (r6, r7, r8, r9, r10) = (
        be_u32(page, rip + 24), be_u32(page, rip + 28), be_u32(page, rip + 32),
        be_u32(page, rip + 36), be_u32(page, rip + 40),
    );
    let da_off = ((r7 >> 13) & 0xFFF) as usize;
    let da_words = ((r10 >> 14) & 0x3FFF) as usize;
    let mem_words = (r10 & 0x3FFF) as usize;
    let node_w0 = be_u32(page, da_off * 4);
    if ((node_w0 >> 16) & 0xF) != 1 {
        return Err("DA node type != 1".into());
    }
    if be_u32(page, (da_off + 3) * 4) != 0 {
        return Err("chained multi-node DA not supported".into());
    }
    let plen = (node_w0 & 0xFFFF) as usize - 5;
    let payload: Vec<u32> = (0..plen).map(|k| be_u32(page, (da_off + 5 + k) * 4)).collect();
    let (s, e, typ) = find_entry_span(&payload, attr_hash)
        .ok_or_else(|| format!("attr 0x{:X} not found in DA region", attr_hash))?;
    if !matches!(typ, 10 | 14 | 15) {
        return Err(format!("DA attr 0x{:X} is type {}, not text", attr_hash, typ));
    }
    let (new_ctrl, val) = pack_text(typ, new_text);
    let mut new_payload = Vec::new();
    new_payload.extend_from_slice(&payload[..s]);
    new_payload.push(attr_hash);
    new_payload.push(new_ctrl);
    new_payload.extend_from_slice(&val);
    new_payload.extend_from_slice(&payload[e..]);
    let new_da_words = new_payload.len();
    let end_word = da_off + 5 + new_da_words;
    let mut limit = pw;
    if mem_words > 0 && r8 == r6 {
        let mem_off = ((r9 >> 13) & 0xFFF) as usize;
        if mem_off > da_off {
            limit = limit.min(mem_off);
        }
    }
    if end_word > limit {
        return Err(format!("DA needs {} words, only {} before page/member bound (needs relocation)",
            end_word - da_off, limit - da_off));
    }
    put_u32(page, da_off * 4, (new_da_words as u32 + 5) | (1 << 16));
    for (k, &wv) in new_payload.iter().enumerate() {
        put_u32(page, (da_off + 5 + k) * 4, wv);
    }
    for wd in end_word..(da_off + 5 + da_words) {
        if wd < pw {
            put_u32(page, wd * 4, 0);
        }
    }
    put_u32(page, rip + 40, (r10 & !(0x3FFF << 14)) | (((new_da_words as u32) & 0x3FFF) << 14));
    Ok((da_words, new_da_words))
}

/// Slice 3 (Rust): create a NEW element by cloning `src_record_off` (new refno + NAME), appended
/// at the MAX key into the rightmost B-tree leaf via COW (no node split; requires room). Mirrors
/// `cow_insert_element`. (Slice 5 `cow_insert_element_split` generalises to any key + split.)
pub fn cow_insert_element(db: &mut Edb, src_record_off: usize, new_refno: (u32, u32), new_name: &str)
    -> Result<CowReport, String> {
    let ps = db.ps;
    let pw = ps / 4;
    let (new_data_pg, src_off_words, _da_old, _da_new) = clone_element_page(db, src_record_off, new_refno, new_name)?;
    let old_root = db.latest_root();
    let (leaf_pg, anc) = rightmost_path(db, old_root)?;
    let mut leaf = db.buf[leaf_pg * ps..(leaf_pg + 1) * ps].to_vec();
    let w6 = be_u32(&leaf, 24) as usize;
    if w6 < 4 {
        return Err("rightmost leaf full -> node split needed (use cow_insert_element_split)".into());
    }
    let nent = (pw - 7 - w6) / 4;
    let last = if nent > 0 {
        let eo = (7 + 4 * (nent - 1)) * 4;
        (be_u32(&leaf, eo), be_u32(&leaf, eo + 4))
    } else {
        (0, 0)
    };
    if new_refno <= last {
        return Err(format!("new refno (0x{:X},0x{:X}) not > current max (0x{:X},0x{:X})",
            new_refno.0, new_refno.1, last.0, last.1));
    }
    let eo = (7 + 4 * nent) * 4;
    put_u32(&mut leaf, eo, new_refno.0);
    put_u32(&mut leaf, eo + 4, new_refno.1);
    put_u32(&mut leaf, eo + 8, new_data_pg as u32);
    put_u32(&mut leaf, eo + 12, (src_off_words << 12) | 1);
    put_u32(&mut leaf, 24, (w6 - 4) as u32); // word6 -= one entry (4 words)
    let new_leaf_pg = db.append_page(&leaf);
    // COW the rightmost ancestor chain (rewire child pointer; keys unchanged)
    let (mut child_old, mut child_new) = (leaf_pg, new_leaf_pg);
    for &(idx_pg, wo) in anc.iter().rev() {
        let mut page = db.buf[idx_pg * ps..(idx_pg + 1) * ps].to_vec();
        let e = wo * 4;
        if be_u32(&page, e + 8) as usize != child_old {
            return Err(format!("path inconsistency at pg {}", idx_pg));
        }
        put_u32(&mut page, e + 8, child_new as u32);
        let np = db.append_page(&page);
        child_old = idx_pg;
        child_new = np;
    }
    let new_root = child_new;
    let (old_sesno, new_sesno, new_ses_pg) = append_session(db, new_root);
    Ok(CowReport {
        refno: new_refno, old_root, new_root, data_pg_old: src_record_off / ps, data_pg_new: new_data_pg,
        old_sesno, new_sesno, new_ses_pg, da_page_new: 0, da_nodes: 0, tree_grew: false,
    })
}

/// Slice 5 (Rust): create a NEW element (clone of `src_record_off` with `new_refno` + `new_name`)
/// and insert it at its SORTED key position via the general B+-tree insert (leaf split + recursive
/// parent split + new root as needed), then commit a new session. Generalises Slice 3. Mirrors
/// `cow_insert_element_split`.
pub fn cow_insert_element_split(db: &mut Edb, src_record_off: usize, new_refno: (u32, u32), new_name: &str,
                               cap: Option<usize>) -> Result<CowReport, String> {
    let ps = db.ps;
    let (new_data_pg, src_off, _da_old, _da_new) = clone_element_page(db, src_record_off, new_refno, new_name)?;
    let old_root = db.latest_root();
    let (new_root, grew) = cow_insert_leaf(db, old_root, new_refno, new_data_pg, src_off, cap)?;
    let (old_sesno, new_sesno, new_ses_pg) = append_session(db, new_root);
    Ok(CowReport {
        refno: new_refno, old_root, new_root, data_pg_old: src_record_off / ps, data_pg_new: new_data_pg,
        old_sesno, new_sesno, new_ses_pg, da_page_new: 0, da_nodes: 0, tree_grew: grew,
    })
}

/// Slice 4 (Rust): delete an element by removing its MAIN-record leaf entry from the B-tree
/// (compact the leaf left + word6 += 4), COW the path to root, append a new session. The data
/// page is intentionally left in place (old sessions still resolve it = multi-version). No node
/// merge on underflow (PDMS tolerates underfull nodes). Mirrors `e3d_write_full.py::cow_delete_element`.
pub fn cow_delete_element(db: &mut Edb, refno: (u32, u32)) -> Result<CowReport, String> {
    let ps = db.ps;
    let pw = ps / 4;
    let old_root = db.latest_root();
    let bo = record_off_via_root(db, old_root, refno)
        .ok_or_else(|| format!("main record for refno ({:#X},{:#X}) not found", refno.0, refno.1))?;
    let data_pg = bo / ps;
    let data_off = ((bo % ps) / 2) as u32;
    let path = find_leaf_path_by_loc(db, old_root, data_pg, data_off)
        .ok_or_else(|| format!("leaf entry for refno ({:#X},{:#X}) not found", refno.0, refno.1))?;
    let &(leaf_pg, entry_byte_off) = path.last().unwrap();
    let anc = &path[..path.len() - 1];

    let mut leaf = db.buf[leaf_pg * ps..(leaf_pg + 1) * ps].to_vec();
    let w6 = be_u32(&leaf, 24) as usize;
    let nent = (pw - 7 - w6) / 4;
    let entry_idx = ((entry_byte_off - leaf_pg * ps) / 4 - 7) / 4;
    if entry_idx >= nent {
        return Err(format!("entry index {} outside word6 bound ({} entries)", entry_idx, nent));
    }
    for k in entry_idx..nent.saturating_sub(1) {
        // compact: shift later entries left one slot (4 words = 16 bytes)
        let (d, s) = ((7 + 4 * k) * 4, (7 + 4 * (k + 1)) * 4);
        let chunk = leaf[s..s + 16].to_vec();
        leaf[d..d + 16].copy_from_slice(&chunk);
    }
    let last = (7 + 4 * (nent - 1)) * 4;
    for b in &mut leaf[last..last + 16] {
        *b = 0; // clear the vacated last slot
    }
    put_u32(&mut leaf, 24, (w6 + 4) as u32); // word6 += one entry (4 words)
    let new_leaf_pg = db.append_page(&leaf);

    let (mut child_old, mut child_new) = (leaf_pg, new_leaf_pg);
    for &(idx_pg, eoff) in anc.iter().rev() {
        let mut page = db.buf[idx_pg * ps..(idx_pg + 1) * ps].to_vec();
        let e = eoff - idx_pg * ps;
        if be_u32(&page, e + 8) as usize != child_old {
            return Err(format!("path inconsistency at pg {}", idx_pg));
        }
        put_u32(&mut page, e + 8, child_new as u32);
        let np = db.append_page(&page);
        child_old = idx_pg;
        child_new = np;
    }
    let new_root = child_new;
    let (old_sesno, new_sesno, new_ses_pg) = append_session(db, new_root);
    Ok(CowReport {
        refno, old_root, new_root, data_pg_old: data_pg, data_pg_new: data_pg, // data page left in place
        old_sesno, new_sesno, new_ses_pg, da_page_new: 0, da_nodes: 0, tree_grew: false,
    })
}

/// Decode one element at byte offset `record_off` (e.g. from `find_record_offset`). Public,
/// allocation-light wrapper over the internal full decoder (fresh typedef cache).
pub fn decode_at(db: &Edb, ss: &SchemaSet, record_off: usize) -> Element {
    decode_full(db, ss, record_off, &mut HashMap::new())
}

// ─── Ergonomic typed write API: `EdbWriter` ──────────────────────────────────────────────────
//   A name-oriented wrapper over the low-level `cow_*` commit functions: resolve a name to its
//   main-record offset and commit (COW + new session), surfacing a typed `E3dError`. The
//   low-level functions remain available for callers that already hold a `record_off`.

/// Typed error for the ergonomic write API.
#[derive(Debug)]
pub enum E3dError {
    /// No element with this NAME under the latest session.
    ElementNotFound(String),
    /// A low-level COW/commit failure (carries the underlying message).
    Write(String),
    /// I/O error opening/saving a db file.
    Io(std::io::Error),
}

impl std::fmt::Display for E3dError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            E3dError::ElementNotFound(n) => write!(f, "element not found: {n}"),
            E3dError::Write(m) => write!(f, "write failed: {m}"),
            E3dError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for E3dError {}

impl From<std::io::Error> for E3dError {
    fn from(e: std::io::Error) -> Self {
        E3dError::Io(e)
    }
}

/// Name-oriented offline E3D writer (owns a COW db buffer). All edits append a new session;
/// prior sessions stay intact (multi-version). Load a fresh writer per editing session; the
/// in-memory buffer is written out with [`EdbWriter::save`] / [`EdbWriter::into_bytes`] — the
/// source file is never touched until you save. Wraps the low-level `cow_*` functions.
pub struct EdbWriter<'a> {
    db: Edb,
    ss: &'a SchemaSet,
}

impl<'a> EdbWriter<'a> {
    /// Open a db file into an owned writer (the file itself is not modified).
    pub fn open(path: &str, ss: &'a SchemaSet) -> Result<Self, E3dError> {
        Ok(EdbWriter { db: Edb::open(path)?, ss })
    }
    /// Wrap an in-memory db copy.
    pub fn from_bytes(bytes: Vec<u8>, ss: &'a SchemaSet) -> Self {
        EdbWriter { db: Edb::from_bytes(bytes), ss }
    }
    /// Borrow the underlying db (for reads/decoding).
    pub fn db(&self) -> &Edb {
        &self.db
    }
    /// Current (possibly COW-extended) bytes.
    pub fn bytes(&self) -> &[u8] {
        self.db.bytes()
    }
    /// Consume and return the edited bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.db.into_bytes()
    }
    /// Write the edited db to `path`.
    pub fn save(&self, path: &str) -> Result<(), E3dError> {
        std::fs::write(path, self.db.bytes())?;
        Ok(())
    }
    /// Resolve a NAME to its main-record byte offset under the latest session.
    pub fn offset_of(&self, name: &str) -> Result<usize, E3dError> {
        find_record_offset(&self.db, self.ss, name).ok_or_else(|| E3dError::ElementNotFound(name.to_string()))
    }
    /// Decode the named element (latest session).
    pub fn element(&self, name: &str) -> Result<Element, E3dError> {
        Ok(decode_at(&self.db, self.ss, self.offset_of(name)?))
    }

    /// Set a fixed-size inline (implicit) value by attribute hash (real/int/ref, same count).
    pub fn set_inline(&mut self, name: &str, attr_hash: u32, val: &Val) -> Result<CowReport, E3dError> {
        let bo = self.offset_of(name)?;
        cow_commit_inline(&mut self.db, self.ss, bo, attr_hash, val).map_err(E3dError::Write)
    }
    /// Set the element's POS (a real triple).
    pub fn set_pos(&mut self, name: &str, xyz: [f64; 3]) -> Result<CowReport, E3dError> {
        self.set_inline(name, POS_HASH, &Val::Reals(xyz.to_vec()))
    }
    /// Rename the element (DA NAME text).
    pub fn rename(&mut self, name: &str, new_name: &str) -> Result<CowReport, E3dError> {
        let bo = self.offset_of(name)?;
        cow_commit_da_text(&mut self.db, bo, NAME_HASH, new_name, None).map_err(E3dError::Write)
    }
    /// Set the element's member (child refno) list.
    pub fn set_members(&mut self, name: &str, children: &[(u32, u32)]) -> Result<CowReport, E3dError> {
        let bo = self.offset_of(name)?;
        cow_members_set(&mut self.db, bo, children, None).map_err(E3dError::Write)
    }
    /// Delete the element (removes its main-record B-tree leaf entry).
    pub fn delete(&mut self, name: &str) -> Result<CowReport, E3dError> {
        let bo = self.offset_of(name)?;
        let refno = decode_at(&self.db, self.ss, bo).refno;
        cow_delete_element(&mut self.db, refno).map_err(E3dError::Write)
    }
    /// Clone an existing element to a fresh refno (max refseq+1 in its dbno) with `new_name`.
    /// Returns `(CowReport, new_refno)`.
    pub fn insert_clone(&mut self, src_name: &str, new_name: &str) -> Result<(CowReport, (u32, u32)), E3dError> {
        let bo = self.offset_of(src_name)?;
        let dbno = decode_at(&self.db, self.ss, bo).refno.0;
        let mut rm = HashMap::new();
        let elems = index_db(&self.db, self.ss, true, &mut rm);
        let maxseq = elems.iter().filter(|e| e.refno.0 == dbno).map(|e| e.refno.1).max().unwrap_or(0);
        let new_refno = (dbno, maxseq + 1);
        let rep = cow_insert_element_split(&mut self.db, bo, new_refno, new_name, None).map_err(E3dError::Write)?;
        Ok((rep, new_refno))
    }

    // ------------------------------------------------------------------
    // refno 导向薄变体(specs/004 决策 A,2026-06-11 用户批准的红线扩展):
    // 与上方 name 导向方法严格同构——仅把 NAME 解析换成 refno 解析,寻址后
    // 走完全相同的 cow_* 路径;使无名元素(真实库 ~88%)可进入 batch 单会话编辑。
    // 除此六变体 + 两个解析助手外,format/事务核心零改动。
    // ------------------------------------------------------------------

    /// Resolve a refno to its main-record byte offset under the latest session.
    pub fn offset_of_refno(&self, refno: (u32, u32)) -> Result<usize, E3dError> {
        record_off_via_root(&self.db, self.db.latest_root(), refno).ok_or_else(|| {
            E3dError::ElementNotFound(format!("refno ({:#x},{:#x})", refno.0, refno.1))
        })
    }
    /// Decode the element at `refno` (latest session).
    pub fn element_at(&self, refno: (u32, u32)) -> Result<Element, E3dError> {
        Ok(decode_at(&self.db, self.ss, self.offset_of_refno(refno)?))
    }
    /// [`Self::set_inline`] 的 refno 导向同构变体。
    pub fn set_inline_at(&mut self, refno: (u32, u32), attr_hash: u32, val: &Val) -> Result<CowReport, E3dError> {
        let bo = self.offset_of_refno(refno)?;
        cow_commit_inline(&mut self.db, self.ss, bo, attr_hash, val).map_err(E3dError::Write)
    }
    /// [`Self::set_pos`] 的 refno 导向同构变体。
    pub fn set_pos_at(&mut self, refno: (u32, u32), xyz: [f64; 3]) -> Result<CowReport, E3dError> {
        self.set_inline_at(refno, POS_HASH, &Val::Reals(xyz.to_vec()))
    }
    /// [`Self::rename`] 的 refno 导向同构变体。与 name 路径同语义:目标须**已有**
    /// NAME 条目(改写);无名元素首次命名属 DA 新增条目,走 [`cow_da_set_entry`]。
    pub fn rename_at(&mut self, refno: (u32, u32), new_name: &str) -> Result<CowReport, E3dError> {
        let bo = self.offset_of_refno(refno)?;
        cow_commit_da_text(&mut self.db, bo, NAME_HASH, new_name, None).map_err(E3dError::Write)
    }
    /// [`Self::set_members`] 的 refno 导向同构变体。
    pub fn set_members_at(&mut self, refno: (u32, u32), children: &[(u32, u32)]) -> Result<CowReport, E3dError> {
        let bo = self.offset_of_refno(refno)?;
        cow_members_set(&mut self.db, bo, children, None).map_err(E3dError::Write)
    }
    /// [`Self::delete`] 的 refno 导向同构变体(同语义:目标不存在 ⇒ `ElementNotFound`)。
    pub fn delete_at(&mut self, refno: (u32, u32)) -> Result<CowReport, E3dError> {
        self.offset_of_refno(refno)?;
        cow_delete_element(&mut self.db, refno).map_err(E3dError::Write)
    }
    /// [`Self::insert_clone`] 的 refno 导向同构变体(新 refno 同规则:该 dbno 最大 refseq+1)。
    pub fn insert_clone_at(&mut self, src_refno: (u32, u32), new_name: &str) -> Result<(CowReport, (u32, u32)), E3dError> {
        let bo = self.offset_of_refno(src_refno)?;
        let dbno = src_refno.0;
        let mut rm = HashMap::new();
        let elems = index_db(&self.db, self.ss, true, &mut rm);
        let maxseq = elems.iter().filter(|e| e.refno.0 == dbno).map(|e| e.refno.1).max().unwrap_or(0);
        let new_refno = (dbno, maxseq + 1);
        let rep = cow_insert_element_split(&mut self.db, bo, new_refno, new_name, None).map_err(E3dError::Write)?;
        Ok((rep, new_refno))
    }

    /// Apply several edits as ONE atomic "save" (`db5_save_work` batch semantics, FR-020 / SC-008).
    /// Each edit inside `edits` commits as usual (appending an intermediate session); on success
    /// they are collapsed into a single new session (`sesno` only +1) whose root carries every
    /// edit (multi-path COW merged onto one final root, since each edit COWs atop the previous
    /// one's latest root). On any edit error the whole batch rolls back: appended pages are
    /// discarded and page0 is restored. Returns the new `sesno`.
    pub fn batch(&mut self, edits: impl FnOnce(&mut Self) -> Result<(), E3dError>) -> Result<u32, E3dError> {
        let base_len = self.db.bytes().len();
        let base_ses = self.db.latest_ses();
        let base_sesno = self.db.sesno_of(base_ses);
        match edits(&mut *self) {
            Ok(()) => {
                let final_root = self.db.latest_root();
                Ok(self.db.collapse_session(base_ses, base_sesno, final_root))
            }
            Err(e) => {
                self.db.buf.truncate(base_len);
                put_u32(&mut self.db.buf, HDR_LATEST, base_ses as u32);
                Err(e)
            }
        }
    }

    /// Preview a batch WITHOUT mutating `self` or touching disk (FR-021 / SC-010): apply the edits
    /// on a throwaway copy and return the element-level [`Diff`] vs the current latest session.
    pub fn dry_run(&self, edits: impl FnOnce(&mut EdbWriter) -> Result<(), E3dError>) -> Result<Diff, E3dError> {
        let mut tmp = EdbWriter::from_bytes(self.db.bytes().to_vec(), self.ss);
        tmp.batch(edits)?;
        Ok(element_diff(&self.db, &tmp.db, self.ss))
    }
}

/// Length of a record's list node chain (which=1 DA via rec[6]/rec[7], 2 members via
/// rec[8]/rec[9]); follows node[3]/node[4], verifying node type==which. For tests.
#[cfg(test)]
fn node_chain_len(buf: &[u8], record_off: usize, ps: usize, which: u32) -> usize {
    let rec = |i: usize| be_u32(buf, record_off + 4 * i);
    let (mut page, mut off) = if which == 1 {
        (rec(6), (rec(7) >> 13) & 0xFFF)
    } else {
        (rec(8), (rec(9) >> 13) & 0xFFF)
    };
    let (mut n, mut guard) = (0, 0);
    while page != 0 && guard < 128 {
        guard += 1;
        let node = page as usize * ps + off as usize * 4;
        if node + 20 > buf.len() || ((be_u32(buf, node) >> 16) & 0xF) != which {
            break;
        }
        n += 1;
        page = be_u32(buf, node + 12);
        off = (be_u32(buf, node + 16) >> 13) & 0xFFF;
    }
    n
}

/// B+-tree binary-search descent: at each internal node pick the rightmost separator <= key
/// (sentinel = -inf), as db3 navigates. True iff the reached leaf contains `key`.
fn btree_descend(db: &Edb, root: usize, key: (u32, u32)) -> bool {
    let mut pg = root;
    for _ in 0..40 {
        let (ents, level) = idx_entries(db, pg);
        if level == 0 {
            return ents.iter().any(|e| (e[0], e[1]) == key);
        }
        let mut ci = 0;
        for (i, e) in ents.iter().enumerate() {
            if key_le((e[0], e[1]), key) {
                ci = i;
            } else {
                break;
            }
        }
        pg = ents[ci][2] as usize;
    }
    false
}

/// B+-tree validator report. `nav_ok` (every key reachable by binary-search descent)
/// — not "separator == subtree min" — is the real invariant: PDMS leaves loose separators after
/// deletes (findings §16). `balanced` = all leaves at one depth; `dups` = adjacent equal keys.
/// Promoted from test-only to a library API so [`verify_commit`] can reuse it (Phase 7 / FR-019).
pub struct BtreeReport {
    pub count: usize,
    pub dups: usize,
    pub leaf_pages: usize,
    pub index_pages: usize,
    pub height: usize,
    pub balanced: bool,
    pub sorted_ok: bool,
    pub nav_ok: bool,
    pub keyset: HashSet<(u32, u32)>,
}

struct BChk<'a> {
    db: &'a Edb,
    count: usize,
    dups: usize,
    leaf_pages: usize,
    index_pages: usize,
    heights: HashSet<usize>,
    prev: Option<(u32, u32)>,
    sorted_ok: bool,
    keyset: HashSet<(u32, u32)>,
}

impl BChk<'_> {
    fn rec(&mut self, pg: usize, depth: usize) {
        let (ents, level) = idx_entries(self.db, pg);
        self.index_pages += 1;
        if level == 0 {
            self.leaf_pages += 1;
            self.heights.insert(depth);
            for e in &ents {
                let k = (e[0], e[1]);
                self.count += 1;
                self.keyset.insert(k);
                if let Some(p) = self.prev {
                    if k < p {
                        self.sorted_ok = false;
                    } else if k == p {
                        self.dups += 1;
                    }
                }
                self.prev = Some(k);
            }
            return;
        }
        for e in &ents {
            self.rec(e[2] as usize, depth + 1);
        }
    }
}

/// Validate the B+-tree under `root` (Phase 7 / FR-019 ①). Library API (was test-only).
pub fn btree_check(db: &Edb, root: usize) -> BtreeReport {
    let mut c = BChk {
        db, count: 0, dups: 0, leaf_pages: 0, index_pages: 0,
        heights: HashSet::new(), prev: None, sorted_ok: true, keyset: HashSet::new(),
    };
    c.rec(root, 0);
    let nav_ok = c.keyset.iter().all(|&k| btree_descend(db, root, k));
    BtreeReport {
        count: c.count, dups: c.dups, leaf_pages: c.leaf_pages, index_pages: c.index_pages,
        height: c.heights.iter().cloned().max().unwrap_or(0),
        balanced: c.heights.len() == 1, sorted_ok: c.sorted_ok, nav_ok, keyset: c.keyset,
    }
}

/// A single expected post-commit element state, for [`verify_commit`] check ③ (read-back).
pub struct Expect {
    /// Element NAME to resolve under the edited db's latest session.
    pub name: String,
    /// Whether the element is expected to exist (false = expected absent, e.g. after delete).
    pub exists: bool,
    /// Expected implicit attribute read-backs `(attr_hash, value)` when `exists` (e.g. POS).
    pub attrs: Vec<(u32, Val)>,
}

/// Why a post-commit self-check ([`verify_commit`]) failed. An empty result `Vec` = a sound commit.
#[derive(Debug)]
pub enum VerifyIssue {
    /// A B-tree invariant broke under the latest session (`which` names it).
    BtreeBroken { which: &'static str },
    /// COW immutability broke: original byte offsets changed outside page0's session pointer.
    OriginalMutated { offsets: Vec<usize> },
    /// The edited file did not grow (COW must append, never overwrite/shrink in place).
    NotAppended,
    /// A target element read back differently than expected (or wrong existence).
    ReadbackMismatch { name: String, detail: String },
    /// An in-db reference (owner) points at a refno absent from the latest session.
    DanglingRef { from: (u32, u32), to: (u32, u32) },
}

/// Post-commit self-verification (Phase 7 / FR-019). Given the pristine `orig` bytes and the
/// `edited` db (after one or more COW commits), check: ① B-tree invariants (nav_ok / balanced /
/// sorted / no-dup) under the latest session; ② COW immutability (original bytes unchanged bar
/// page0's session pointer `0x28`) + append-only growth; ③ each [`Expect`] reads back as stated;
/// ④ owner-reference integrity (no in-db owner dangles). Returns `Ok(())` iff sound, else the
/// list of [`VerifyIssue`]s.
pub fn verify_commit(orig: &[u8], edited: &Edb, ss: &SchemaSet, expects: &[Expect]) -> Result<(), Vec<VerifyIssue>> {
    let mut issues = Vec::new();
    let eb = edited.bytes();

    // ② COW immutability + append-only: original-region bytes unchanged bar page0's session
    //    pointer (0x28..0x2C); the file must never shrink.
    if eb.len() < orig.len() {
        issues.push(VerifyIssue::NotAppended);
    }
    let n = orig.len().min(eb.len());
    let mut mutated = Vec::new();
    for i in 0..n {
        if (HDR_LATEST..HDR_LATEST + 4).contains(&i) {
            continue;
        }
        if orig[i] != eb[i] {
            mutated.push(i);
            if mutated.len() >= 64 {
                break;
            }
        }
    }
    if !mutated.is_empty() {
        issues.push(VerifyIssue::OriginalMutated { offsets: mutated });
    }

    // ① B-tree invariants under the latest session.
    let rep = btree_check(edited, edited.latest_root());
    if !rep.nav_ok {
        issues.push(VerifyIssue::BtreeBroken { which: "nav_ok" });
    }
    if !rep.balanced {
        issues.push(VerifyIssue::BtreeBroken { which: "balanced" });
    }
    if !rep.sorted_ok {
        issues.push(VerifyIssue::BtreeBroken { which: "sorted" });
    }
    if rep.dups != 0 {
        issues.push(VerifyIssue::BtreeBroken { which: "no_dups" });
    }

    // ③ read-back: each Expect resolves (or not) and its attrs match.
    for ex in expects {
        match (find_record_offset(edited, ss, &ex.name), ex.exists) {
            (None, true) => issues.push(VerifyIssue::ReadbackMismatch {
                name: ex.name.clone(),
                detail: "expected present, not found".into(),
            }),
            (Some(_), false) => issues.push(VerifyIssue::ReadbackMismatch {
                name: ex.name.clone(),
                detail: "expected absent, still present".into(),
            }),
            (Some(bo), true) => {
                let el = decode_at(edited, ss, bo);
                for (h, want) in &ex.attrs {
                    let got = el.implicit.iter().find(|a| a.hash == *h).map(|a| &a.val);
                    if got != Some(want) {
                        issues.push(VerifyIssue::ReadbackMismatch {
                            name: ex.name.clone(),
                            detail: format!("attr 0x{h:X}: got {got:?}, want {want:?}"),
                        });
                    }
                }
            }
            (None, false) => {}
        }
    }

    // ④ owner-reference integrity: no in-db owner points at a refno absent from the session.
    let mut refmap = HashMap::new();
    let elems = index_db(edited, ss, true, &mut refmap);
    let dbnos: HashSet<u32> = rep.keyset.iter().map(|k| k.0).collect();
    for e in &elems {
        let owner = e.owner;
        if owner == (0, 0) || owner.0 == 0x80000001 {
            continue;
        }
        if dbnos.contains(&owner.0) && !rep.keyset.contains(&owner) {
            issues.push(VerifyIssue::DanglingRef { from: e.refno, to: owner });
            if issues.len() > 256 {
                break;
            }
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

/// A single attribute change in an element-level [`Diff`] (old -> new; `None` = absent).
#[derive(Debug, PartialEq)]
pub struct AttrChange {
    pub hash: u32,
    pub name: String,
    pub old: Option<Val>,
    pub new: Option<Val>,
}

/// A modified element and its per-attribute changes, within an element-level [`Diff`].
#[derive(Debug, PartialEq)]
pub struct ElemChange {
    pub refno: (u32, u32),
    pub name: Option<String>,
    pub attrs: Vec<AttrChange>,
}

/// Element-level difference between two db states' latest sessions (FR-021).
#[derive(Debug, PartialEq, Default)]
pub struct Diff {
    pub added: Vec<((u32, u32), Option<String>)>,
    pub removed: Vec<((u32, u32), Option<String>)>,
    pub modified: Vec<ElemChange>,
}

fn snapshot_elems(db: &Edb, ss: &SchemaSet) -> HashMap<(u32, u32), Element> {
    let mut rm = HashMap::new();
    index_db(db, ss, true, &mut rm).into_iter().map(|e| (e.refno, e)).collect()
}

/// Element-level diff between two dbs' latest sessions: added / removed / modified (attr old->new).
/// NAME changes surface as an `AttrChange` with `hash == NAME_HASH`. Deterministically sorted.
pub fn element_diff(old_db: &Edb, new_db: &Edb, ss: &SchemaSet) -> Diff {
    let before = snapshot_elems(old_db, ss);
    let after = snapshot_elems(new_db, ss);
    let mut d = Diff::default();
    for (refno, el) in &after {
        if !before.contains_key(refno) {
            d.added.push((*refno, el.name.clone()));
        }
    }
    for (refno, el) in &before {
        let Some(nel) = after.get(refno) else {
            d.removed.push((*refno, el.name.clone()));
            continue;
        };
        let mut attrs = Vec::new();
        if el.name != nel.name {
            attrs.push(AttrChange {
                hash: NAME_HASH,
                name: "NAME".into(),
                old: el.name.clone().map(Val::Text),
                new: nel.name.clone().map(Val::Text),
            });
        }
        let mut hashes: Vec<u32> = el.implicit.iter().map(|a| a.hash).collect();
        for a in &nel.implicit {
            if !hashes.contains(&a.hash) {
                hashes.push(a.hash);
            }
        }
        for h in hashes {
            let ov = el.implicit.iter().find(|a| a.hash == h);
            let nv = nel.implicit.iter().find(|a| a.hash == h);
            let (oval, nval) = (ov.map(|a| &a.val), nv.map(|a| &a.val));
            if oval != nval {
                attrs.push(AttrChange {
                    hash: h,
                    name: ov.or(nv).map(|a| a.name.trim().to_string()).unwrap_or_default(),
                    old: oval.cloned(),
                    new: nval.cloned(),
                });
            }
        }
        if !attrs.is_empty() {
            attrs.sort_by_key(|a| a.hash);
            d.modified.push(ElemChange { refno: *refno, name: nel.name.clone(), attrs });
        }
    }
    d.added.sort();
    d.removed.sort();
    d.modified.sort_by_key(|c| c.refno);
    d
}

/// A safety guard that blocks a risky edit unless forced (T037 / FR-022).
#[derive(Debug, PartialEq)]
pub enum Guard {
    /// The element still has child elements (others' owner points at it) — deleting would orphan them.
    HasMembers { count: usize },
    /// The element is the target of other elements' reference attributes — deleting would dangle them.
    Referenced { by: Vec<(u32, u32)> },
}

/// Reasons it is unsafe to delete `refno` under the latest session (empty `Vec` = safe to delete):
/// it still has children (would orphan them) or is referenced by other elements' reference
/// attributes (would dangle them). Guards are advisory — callers may override with `--force`. FR-022.
pub fn delete_guards(db: &Edb, ss: &SchemaSet, refno: (u32, u32)) -> Vec<Guard> {
    let mut rm = HashMap::new();
    let elems = index_db(db, ss, true, &mut rm);
    let mut guards = Vec::new();
    let children = elems.iter().filter(|e| e.owner == refno).count();
    if children > 0 {
        guards.push(Guard::HasMembers { count: children });
    }
    let mut by: Vec<(u32, u32)> = elems
        .iter()
        .filter(|e| {
            e.refno != refno
                && e.implicit
                    .iter()
                    .chain(e.da.iter())
                    .any(|a| matches!(&a.val, Val::Refs(rs) if rs.contains(&refno)))
        })
        .map(|e| e.refno)
        .collect();
    by.sort();
    if !by.is_empty() {
        guards.push(Guard::Referenced { by });
    }
    guards
}

/// First clean main record with `lo..=hi` children -> (refno, record_off, children). For tests.
#[cfg(test)]
fn find_member_element(db: &Edb, lo: usize, hi: usize) -> Option<((u32, u32), usize, Vec<(u32, u32)>)> {
    let root = db.latest_root();
    let mut leaves = Vec::new();
    let mut seen_pg = HashSet::new();
    walk(db, root, &mut leaves, &mut seen_pg, 4_000_000);
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
        if (w0 >> 16) != 0 || !(8..=512).contains(&(w0 & 0xFFFF)) || !looks_like_noun(db.u(bo + 12)) {
            continue;
        }
        if !seen.insert((*r0, *r1)) {
            continue;
        }
        let nchild = (db.u(bo + 40) & 0x3FFF) as usize / 2;
        if (lo..=hi).contains(&nchild) {
            return Some(((*r0, *r1), bo, read_members(&db.buf, bo, db.ps)));
        }
    }
    None
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
        // word6-bounded + sentinel-descending walk (findings §16): the authoritative
        // sam7200 counts. The old null-terminated + sentinel-skipping walk reported
        // 6536/1144 (under-counted the leftmost B+-tree spine: WORLD/SITE/ZONE/...).
        assert_eq!(elems.len(), 10392);
        assert_eq!(elems.iter().filter(|e| e.name.is_some()).count(), 1209);
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

    #[test]
    fn cow_inline_commit_multiversion() {
        // Slice 1 (Rust): COW-commit POS twice and verify the multi-version session history —
        // newest session sees v2, the previous v1, the original the pristine value; the original
        // bytes are immutable except page0's session pointer (0x28). Mirrors the Python S1 demo.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let orig_bytes = std::fs::read(DBF).unwrap();
        let mut db = Edb::from_bytes(orig_bytes.clone());
        let bo0 = find_record_offset(&db, &ss, "/WB1").unwrap();
        let el0 = decode_full(&db, &ss, bo0, &mut HashMap::new());
        let refno = el0.refno;
        let orig_pos = el0.pos().unwrap().to_vec();
        let n_roots0 = session_roots(&db).len();

        let v1 = vec![1000.25, -2000.5, 3000.75];
        let v2 = vec![11.0, 22.0, 33.0];
        // commit #1 (re-resolve the record under the latest session each time)
        let bo1 = find_record_offset(&db, &ss, "/WB1").unwrap();
        cow_commit_inline(&mut db, &ss, bo1, POS_HASH, &Val::Reals(v1.clone())).unwrap();
        // commit #2 stacked on top
        let bo2 = find_record_offset(&db, &ss, "/WB1").unwrap();
        cow_commit_inline(&mut db, &ss, bo2, POS_HASH, &Val::Reals(v2.clone())).unwrap();

        let roots = session_roots(&db);
        let read_pos = |root: usize| -> Vec<f64> {
            let b = record_off_via_root(&db, root, refno).unwrap();
            decode_full(&db, &ss, b, &mut HashMap::new()).pos().unwrap().to_vec()
        };
        assert_eq!(read_pos(roots[0]), v2, "newest session must see v2");
        assert_eq!(read_pos(roots[1]), v1, "previous session must see v1");
        assert_eq!(read_pos(roots[2]), orig_pos, "original session must be pristine");
        assert_eq!(roots.len(), n_roots0 + 2, "two new sessions appended");

        // original byte range immutable except page0's session pointer (0x28..0x2C)
        let edited = db.bytes();
        let escaped: Vec<usize> = (0..orig_bytes.len())
            .filter(|&i| orig_bytes[i] != edited[i] && !(HDR_LATEST..HDR_LATEST + 4).contains(&i))
            .collect();
        assert!(escaped.is_empty(), "COW must not mutate original pages: {:?}", escaped);
        assert!(edited.len() > orig_bytes.len(), "new pages must be appended");
    }

    #[test]
    fn verify_commit_sound_and_bad() {
        // T031 (Phase 7 / FR-019, SC-009): a sound COW commit passes verify_commit; a tampered
        // original byte (broken COW immutability) and a wrong read-back expectation are caught.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let orig = std::fs::read(DBF).unwrap();

        // sound commit: set POS via the writer, verify passes
        let pos = [11.0, 22.0, 33.0];
        let mut w = EdbWriter::from_bytes(orig.clone(), &ss);
        w.set_pos("/WB1", pos).unwrap();
        let edited_bytes = w.into_bytes();
        let edited = Edb::from_bytes(edited_bytes.clone());
        let expects = vec![Expect {
            name: "/WB1".into(),
            exists: true,
            attrs: vec![(POS_HASH, Val::Reals(pos.to_vec()))],
        }];
        assert!(verify_commit(&orig, &edited, &ss, &expects).is_ok(), "sound commit must verify");

        // bad commit: mutate an original byte (page 1, not page0's session pointer)
        let mut tampered = edited_bytes.clone();
        tampered[2048 + 100] ^= 0xFF;
        let res = verify_commit(&orig, &Edb::from_bytes(tampered), &ss, &expects);
        let issues = res.expect_err("tampered original bytes must fail verify");
        assert!(
            issues.iter().any(|i| matches!(i, VerifyIssue::OriginalMutated { .. })),
            "expected OriginalMutated, got {issues:?}"
        );

        // wrong read-back expectation must be caught
        let wrong = vec![Expect {
            name: "/WB1".into(),
            exists: true,
            attrs: vec![(POS_HASH, Val::Reals(vec![0.0, 0.0, 0.0]))],
        }];
        let res2 = verify_commit(&orig, &edited, &ss, &wrong);
        let issues2 = res2.expect_err("wrong expected POS must fail verify");
        assert!(
            issues2.iter().any(|i| matches!(i, VerifyIssue::ReadbackMismatch { .. })),
            "expected ReadbackMismatch, got {issues2:?}"
        );

        // dangling owner (④): rewrite /WB1's owner (w4/w5 @ +16/+20) in its COW'd record — which
        // lives in the appended region, so this does NOT trip OriginalMutated — to a missing refseq.
        let bo = find_record_offset(&edited, &ss, "/WB1").unwrap();
        assert!(bo >= orig.len(), "COW'd record must live in the appended region");
        let dbno = decode_at(&edited, &ss, bo).refno.0;
        let mut tampered2 = edited_bytes.clone();
        tampered2[bo + 16..bo + 20].copy_from_slice(&dbno.to_be_bytes());
        tampered2[bo + 20..bo + 24].copy_from_slice(&99_999_999u32.to_be_bytes());
        let res3 = verify_commit(&orig, &Edb::from_bytes(tampered2), &ss, &[]);
        let issues3 = res3.expect_err("dangling owner must fail verify");
        assert!(
            issues3.iter().any(|i| matches!(i, VerifyIssue::DanglingRef { .. })),
            "expected DanglingRef, got {issues3:?}"
        );
    }

    #[test]
    fn batch_three_edits_single_session() {
        // T033 (FR-020, SC-008): set_pos + insert_clone + rename committed as ONE batch -> sesno
        // only +1, exactly one new session in the chain, all edits effective, the previous session
        // keeps the original value, and verify_commit passes.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let orig = std::fs::read(DBF).unwrap();
        let mut w = EdbWriter::from_bytes(orig.clone(), &ss);

        let wb1_refno = w.element("/WB1").unwrap().refno;
        let orig_pos = w.element("/WB1").unwrap().pos().unwrap().to_vec();
        let base_ses = w.db().latest_ses();
        let base_sesno = w.db().sesno_of(base_ses);
        let n0 = session_roots(w.db()).len();

        let pos = [123.0, 456.0, 789.0];
        let new_sesno = w
            .batch(|w| {
                w.set_pos("/WB1", pos)?;
                w.insert_clone("/WB1", "/WB_CLONE")?;
                w.rename("/WB1", "/WB1_R")?;
                Ok(())
            })
            .expect("batch must succeed");

        // single session: sesno only +1, exactly one new root in the session chain
        assert_eq!(new_sesno, base_sesno + 1, "batch must bump sesno by exactly 1");
        let roots = session_roots(w.db());
        assert_eq!(roots.len(), n0 + 1, "batch must add exactly one session to the chain");

        // all three edits effective under the latest session
        assert_eq!(w.element("/WB1_R").unwrap().pos().unwrap().to_vec(), pos, "/WB1_R keeps new POS");
        assert!(w.element("/WB_CLONE").is_ok(), "/WB_CLONE inserted");
        assert!(w.element("/WB1").is_err(), "/WB1 renamed away");

        // previous session unchanged (original POS, /WB1 still there under the old root)
        let prev_root = roots[1];
        let prev_off = record_off_via_root(w.db(), prev_root, wb1_refno).unwrap();
        assert_eq!(decode_at(w.db(), &ss, prev_off).pos().unwrap().to_vec(), orig_pos, "previous session pristine");

        // verify_commit passes for the whole batch
        let expects = vec![
            Expect { name: "/WB1_R".into(), exists: true, attrs: vec![(POS_HASH, Val::Reals(pos.to_vec()))] },
            Expect { name: "/WB_CLONE".into(), exists: true, attrs: vec![] },
            Expect { name: "/WB1".into(), exists: false, attrs: vec![] },
        ];
        verify_commit(&orig, w.db(), &ss, &expects).expect("batch commit must verify");
    }

    #[test]
    fn batch_rollback_on_error() {
        // Hardening (FR-020 atomicity): if any edit in a batch fails, the WHOLE batch rolls back —
        // no session is added, appended pages are discarded, and the latest read is unchanged.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let orig = std::fs::read(DBF).unwrap();
        let mut w = EdbWriter::from_bytes(orig.clone(), &ss);
        let n0 = session_roots(w.db()).len();
        let len0 = w.bytes().len();
        let pos0 = w.element("/WB1").unwrap().pos().unwrap().to_vec();

        let res = w.batch(|w| {
            w.set_pos("/WB1", [7.0, 8.0, 9.0])?; // would succeed on its own
            w.rename("/NOPE_NOT_THERE", "/X")?; // fails -> triggers a full-batch rollback
            Ok(())
        });

        assert!(res.is_err(), "a batch with a doomed edit must fail");
        assert_eq!(session_roots(w.db()).len(), n0, "no session may survive a rolled-back batch");
        assert_eq!(w.bytes().len(), len0, "appended pages must be discarded on rollback");
        assert_eq!(
            w.element("/WB1").unwrap().pos().unwrap().to_vec(),
            pos0,
            "/WB1 must be unchanged after a rolled-back batch"
        );
    }

    /// specs/004 决策 A:refno 导向薄变体让**无名元素**(sam7200 ~88%)可进入
    /// batch 单会话编辑——这是 name 导向 API 无法表达的能力。
    #[test]
    fn edbwriter_refno_oriented_unnamed() {
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let mut w = EdbWriter::open(DBF, &ss).unwrap();

        // 找一个无名且带 POS 的元素(写回管道的典型目标)。
        let mut rm = HashMap::new();
        let elems = index_db(w.db(), &ss, true, &mut rm);
        let target = elems
            .iter()
            .find(|e| e.name.is_none() && e.pos().is_some())
            .expect("sam7200 has unnamed elements with POS")
            .refno;
        let named = elems
            .iter()
            .find(|e| e.name.as_deref() == Some("/WB1"))
            .expect("/WB1 present")
            .refno;
        let sesno0 = w.db().u(w.db().latest_ses() * w.db().page_size() + SES_SESNO);

        // 混合两笔 refno 导向编辑,batch 单会话:无名元素改 POS(headline 能力)
        // + 具名元素经 refno 改名(rename_at 同构性)。
        let new_pos = [111.0, 222.0, 333.5];
        let sesno1 = w
            .batch(|w| {
                w.set_pos_at(target, new_pos)?;
                w.rename_at(named, "/WB1-T102A")?;
                Ok(())
            })
            .unwrap();
        assert_eq!(sesno1, sesno0 + 1, "batch must collapse to a single new session");

        // 读回:无名元素位置生效且仍无名、refno 不变;具名元素新名生效。
        let e = w.element_at(target).unwrap();
        assert_eq!(e.refno, target);
        assert_eq!(e.pos(), Some(&new_pos[..]));
        assert_eq!(e.name, None, "unnamed stays unnamed");
        assert_eq!(w.element_at(named).unwrap().name.as_deref(), Some("/WB1-T102A"));

        // 严格同构语义:rename_at 要求已有 NAME 条目——无名元素首次命名不在其面内。
        assert!(w.rename_at(target, "/NOPE").is_err());

        // 不存在的 refno ⇒ 类型化 ElementNotFound(与 name 路径同语义)。
        assert!(matches!(
            w.set_pos_at((0x5C20, 0xFFFF_FFF0), [0.0; 3]),
            Err(E3dError::ElementNotFound(_))
        ));
        // 错误编辑混入批 ⇒ 整批回滚(原子性延续到 refno 变体)。
        let len_before = w.bytes().len();
        let r = w.batch(|w| {
            w.set_pos_at(target, [1.0, 2.0, 3.0])?;
            w.delete_at((0x5C20, 0xFFFF_FFF0))?;
            Ok(())
        });
        assert!(r.is_err());
        assert_eq!(w.bytes().len(), len_before, "failed batch must roll back appended pages");
        let e2 = w.element_at(target).unwrap();
        assert_eq!(e2.pos(), Some(&new_pos[..]), "rolled-back edit must not stick");
    }

    /// 布局锚点(specs/005 T101 真实字节裁决的固化):
    /// ① members 链节点 = 5 词头 `[(which<<16)|total_words][refno0][refno1][next_pg][next_loc]`
    ///   + payload(成员 refno 对,自 node+20 起)——v1 解析器的 +12 载荷 = 把 w3/w4 链指针
    ///   也计入载荷(邻接单节点时为 (0,0));
    /// ② PDMS 原生(邻接)布局:rec[8]/[9] 所指节点恰好 == 隐式区+0/7 padding 之后的窗口
    ///   位置——v1 窗口邻接假设的字节级根据,也是 005 链式重组(F1-I1)的还原目标。
    #[test]
    fn members_node_layout_anchor() {
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let db = Edb::open(DBF).unwrap();
        let (refno, bo, children) = find_member_element(&db, 1, 8).expect("member element");
        let rec = |i: usize| db.u(bo + 4 * i);

        // 链视角节点位置
        let node = rec(8) as usize * db.page_size() + (((rec(9) >> 13) & 0xFFF) as usize) * 4;
        // 窗口邻接视角:隐式区 + 0/7 padding 之后
        let impl_words = (rec(0) & 0xFFFF) as usize;
        let mut membs_pos = bo + impl_words * 4;
        while membs_pos + 4 <= db.bytes().len() {
            let v = db.u(membs_pos);
            if v != 0 && v != 7 {
                break;
            }
            membs_pos += 4;
        }
        assert_eq!(node, membs_pos, "native layout: members node adjoins the record window");

        // 5 词节点头:which=2 | 总词数;w1/w2 = self refno;payload 自 +20。
        let hdr = db.u(node);
        assert_eq!((hdr >> 16) & 0xF, 2, "node type nibble = members");
        assert_eq!((db.u(node + 4), db.u(node + 8)), refno, "node w1/w2 = self refno");
        assert_eq!(
            (db.u(node + 20), db.u(node + 24)),
            children[0],
            "payload (member refno pairs) starts at node+20"
        );
    }

    #[test]
    fn dry_run_matches_real_commit() {
        // T036 (FR-021, SC-010): dry_run previews a batch WITHOUT mutating the writer or disk, and
        // its element-level Diff equals the diff computed from a real commit of the same edits.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let orig = std::fs::read(DBF).unwrap();
        let pos = [314.0, 271.0, 161.0];

        let w = EdbWriter::from_bytes(orig.clone(), &ss);
        let diff_dry = w
            .dry_run(|w| {
                w.set_pos("/WB1", pos)?;
                w.insert_clone("/WB1", "/WB_DRY")?;
                Ok(())
            })
            .expect("dry_run ok");
        assert!(w.element("/WB_DRY").is_err(), "dry_run must not mutate the writer");

        let mut w_real = EdbWriter::from_bytes(orig.clone(), &ss);
        w_real
            .batch(|w| {
                w.set_pos("/WB1", pos)?;
                w.insert_clone("/WB1", "/WB_DRY")?;
                Ok(())
            })
            .expect("real batch ok");
        let diff_real = element_diff(&Edb::from_bytes(orig.clone()), w_real.db(), &ss);

        assert_eq!(diff_dry, diff_real, "dry-run diff must equal real-commit diff");
        assert!(
            diff_dry.added.iter().any(|(_, n)| n.as_deref() == Some("/WB_DRY")),
            "added must include /WB_DRY: {:?}",
            diff_dry.added
        );
        assert!(
            diff_dry.modified.iter().any(|c| c
                .attrs
                .iter()
                .any(|a| a.hash == POS_HASH && a.new == Some(Val::Reals(pos.to_vec())))),
            "modified must include /WB1 POS->new: {:?}",
            diff_dry.modified
        );
    }

    #[test]
    fn delete_guards_block_parent() {
        // T037 (FR-022): deleting an element that still has children must be guarded; a childless,
        // unreferenced leaf is safe to delete.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let db = Edb::open(DBF).unwrap();
        let wb1 = find_record_offset(&db, &ss, "/WB1").unwrap();
        let parent = decode_at(&db, &ss, wb1).owner;
        let g = delete_guards(&db, &ss, parent);
        assert!(
            g.iter().any(|x| matches!(x, Guard::HasMembers { .. })),
            "deleting /WB1's parent (has children) must be guarded: {g:?}"
        );
    }

    #[test]
    fn cow_da_text_rename_multiversion() {
        // Slice 2/6 (Rust): COW-rename /WB1 via DA relocation. New session = new name (POS
        // unchanged), previous session = old name; the DA is now on a different page (cross-
        // page); original bytes immutable except page0's session pointer.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let orig = std::fs::read(DBF).unwrap();
        let mut db = Edb::from_bytes(orig.clone());
        let bo = find_record_offset(&db, &ss, "/WB1").unwrap();
        let refno = decode_full(&db, &ss, bo, &mut HashMap::new()).refno;
        let new_name = "/WB1-RS-RENAMED";
        cow_commit_da_text(&mut db, bo, NAME_HASH, new_name, None).unwrap();

        let roots = session_roots(&db);
        let read = |root: usize| -> (Option<String>, Option<Vec<f64>>) {
            let b = record_off_via_root(&db, root, refno).unwrap();
            let e = decode_full(&db, &ss, b, &mut HashMap::new());
            (e.name.clone(), e.pos().map(|p| p.to_vec()))
        };
        let (n_new, p_new) = read(roots[0]);
        let (n_old, _) = read(roots[1]);
        assert_eq!(n_new.as_deref(), Some(new_name), "new session sees new name");
        assert_eq!(n_old.as_deref(), Some("/WB1"), "old session keeps old name");
        assert_eq!(p_new, Some(vec![9630.0, 8072.0, 5282.5]), "POS unchanged");
        let b = record_off_via_root(&db, roots[0], refno).unwrap();
        assert_ne!(be_u32(db.bytes(), b + 24) as usize, b / db.page_size(), "DA now cross-page");
        let edited = db.bytes();
        let escaped: Vec<usize> = (0..orig.len())
            .filter(|&i| orig[i] != edited[i] && !(HDR_LATEST..HDR_LATEST + 4).contains(&i))
            .collect();
        assert!(escaped.is_empty(), "COW must not mutate original pages: {:?}", escaped);
    }

    #[test]
    fn cow_da_text_chained() {
        // Slice 6 (Rust): force a multi-node DA chain (tiny chunks) and verify the chained-aware
        // reader still reads the new name back -> writer chunked chain <-> reader chain agree.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let mut db = Edb::from_bytes(std::fs::read(DBF).unwrap());
        let bo = find_record_offset(&db, &ss, "/WB1").unwrap();
        let refno = decode_full(&db, &ss, bo, &mut HashMap::new()).refno;
        cow_commit_da_text(&mut db, bo, NAME_HASH, "/WB1-CHAIN", Some(4)).unwrap();
        let roots = session_roots(&db);
        let b = record_off_via_root(&db, roots[0], refno).unwrap();
        assert!(node_chain_len(db.bytes(), b, db.page_size(), 1) >= 2, "forced multi-node chain");
        let e = decode_full(&db, &ss, b, &mut HashMap::new());
        assert_eq!(e.name.as_deref(), Some("/WB1-CHAIN"));
    }

    #[test]
    fn cow_uda_add_remove_roundtrip() {
        // Slice 8 (Rust): add a synthetic UDA (hash>threshold, type-4 ref) then remove it.
        // Verify across sessions the UDA set goes original -> +new -> original.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let mut db = Edb::from_bytes(std::fs::read(DBF).unwrap());
        let bo = find_record_offset(&db, &ss, "/WB1").unwrap();
        let refno = decode_full(&db, &ss, bo, &mut HashMap::new()).refno;
        let (h, val) = (0x2C00FFFFu32, vec![0xAAAAu32, 0xBBBB]);
        assert!(read_uda(db.bytes(), bo, db.page_size()).iter().all(|u| u.0 != h));

        let bo_a = find_record_offset(&db, &ss, "/WB1").unwrap();
        cow_da_set_entry(&mut db, bo_a, h, 4, &val, None).unwrap();
        let b_add = record_off_via_root(&db, session_roots(&db)[0], refno).unwrap();
        let uda_add = read_uda(db.bytes(), b_add, db.page_size());
        assert!(uda_add.iter().any(|u| u.0 == h && u.2 == val), "UDA present after add");

        cow_da_remove_entry(&mut db, b_add, h, None).unwrap();
        let b_del = record_off_via_root(&db, session_roots(&db)[0], refno).unwrap();
        assert!(read_uda(db.bytes(), b_del, db.page_size()).iter().all(|u| u.0 != h), "UDA gone after remove");
    }

    #[test]
    fn btree_synthetic_split_grow() {
        // Slice 5 (synthetic, no data needed): build a B+-tree from scratch with a tiny cap so a
        // handful of inserts force recursive node splits + new-root growth, then assert every
        // structural invariant (balanced / strictly-sorted / nav_ok / no-dup / all keys present /
        // height grew). Exercises the recursive + root-split paths real sam7200 can't cheaply hit.
        let ps = 2048usize;
        let pw = ps / 4;
        let mut buf = vec![0u8; ps * 2]; // page0 (header) + page1 (empty root leaf)
        put_u32(&mut buf, 0x34, (ps / 4) as u32); // header word: page_size/4 -> Edb infers ps
        for (w, v) in [(0u32, 5u32), (1, INDEX_NOUN), (2, 0), (3, 2), (4, 2), (5, 0), (6, (pw - 7) as u32)] {
            put_u32(&mut buf, ps + 4 * w as usize, v); // type5 / noun / level0 / k2 / d2 / -- / free
        }
        let mut db = Edb::from_bytes(buf);
        assert_eq!(db.page_size(), ps);
        let (cap, n) = (3usize, 60u32);
        // a deterministic non-sorted permutation of 1..=n (37 coprime to 60) to stress placement
        let keys: Vec<(u32, u32)> = (0..n).map(|i| (0x5C20u32, ((i * 37) % n) + 1)).collect();
        let (mut root, mut grew) = (1usize, 0u32);
        for &k in &keys {
            let (r, g) = cow_insert_leaf(&mut db, root, k, 2, 10, Some(cap)).unwrap();
            root = r;
            grew += g as u32;
        }
        let rep = btree_check(&db, root);
        let expect: HashSet<(u32, u32)> = (1..=n).map(|s| (0x5C20u32, s)).collect();
        assert_eq!(rep.count, n as usize, "all keys present as leaf entries");
        assert_eq!(rep.dups, 0, "no duplicate keys");
        assert!(rep.sorted_ok, "leaf order non-decreasing");
        assert!(rep.balanced, "all leaves at one depth");
        assert!(rep.nav_ok, "every key reachable by binary-search descent");
        assert_eq!(rep.keyset, expect, "keyset complete");
        assert!(rep.height >= 2 && grew >= 1, "tree grew (height={}, grew={})", rep.height, grew);
        assert!(rep.index_pages >= rep.leaf_pages, "index pages include internal + leaf pages");
    }

    #[test]
    fn cow_insert_split_real() {
        // Slice 5 (real sam7200, leaf split): insert enough max-key leaf entries (aliasing /WB1's
        // data page) to overflow the rightmost leaf and force a real node split. Verify the new
        // session has orig+N keys (all kept + all new), a leaf actually split, the tree stays
        // balanced/sorted/nav_ok, the previous session is unchanged, and bytes are immutable.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let orig = std::fs::read(DBF).unwrap();
        let mut db = Edb::from_bytes(orig.clone());
        let bo = find_record_offset(&db, &ss, "/WB1").unwrap();
        let ps = db.page_size();
        let (data_pg, off) = (bo / ps, ((bo % ps) / 2) as u32);
        let root0 = db.latest_root();
        let chk0 = btree_check(&db, root0);
        let orig_keys = chk0.keyset.clone();
        let (leaf_pg, _) = rightmost_path(&db, root0).unwrap();
        let free = be_u32(db.bytes(), leaf_pg * ps + 24) as usize;
        let n = free / 4 + 6; // fill the rightmost leaf then overflow it
        let mx = *chk0.keyset.iter().max().unwrap();
        let new_keys: Vec<(u32, u32)> = (1..=n as u32).map(|i| (mx.0, mx.1 + i)).collect();
        let mut root = root0;
        for &k in &new_keys {
            let (r, _g) = cow_insert_leaf(&mut db, root, k, data_pg, off, None).unwrap();
            root = r;
        }
        append_session(&mut db, root);

        let roots = session_roots(&db);
        let chk_new = btree_check(&db, roots[0]);
        let chk_old = btree_check(&db, roots[1]);
        assert!(chk_new.leaf_pages > chk0.leaf_pages, "a real leaf split must occur");
        assert_eq!(chk_new.count, chk0.count + n, "orig + N entries");
        assert!(new_keys.iter().all(|k| chk_new.keyset.contains(k)), "all new keys present");
        assert!(orig_keys.iter().all(|k| chk_new.keyset.contains(k)), "originals kept");
        assert!(chk_new.balanced && chk_new.nav_ok && chk_new.sorted_ok && chk_new.dups == 0);
        assert_eq!(chk_old.keyset, orig_keys, "previous session unchanged");
        let edited = db.bytes();
        let escaped: Vec<usize> = (0..orig.len())
            .filter(|&i| orig[i] != edited[i] && !(HDR_LATEST..HDR_LATEST + 4).contains(&i))
            .collect();
        assert!(escaped.is_empty(), "COW must not mutate original pages: {:?}", escaped);
    }

    #[test]
    fn cow_insert_mid_real() {
        // Slice 5 (real sam7200, arbitrary key): clone /WB1 to a FREE refno in the MIDDLE of the
        // key range and insert via the general B+-tree insert. Verify the new element decodes
        // (noun/owner/name) in the new session, is absent from the previous one, the tree stays
        // balanced/sorted/nav_ok, and the original bytes are immutable.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let orig = std::fs::read(DBF).unwrap();
        let mut db = Edb::from_bytes(orig.clone());
        let bo = find_record_offset(&db, &ss, "/WB1").unwrap();
        let src = decode_full(&db, &ss, bo, &mut HashMap::new());
        let root0 = db.latest_root();
        let chk0 = btree_check(&db, root0);
        let dbno = src.refno.0;
        let maxk = *chk0.keyset.iter().max().unwrap();
        let mut seqs: Vec<u32> = chk0.keyset.iter().filter(|k| k.0 == dbno).map(|k| k.1).collect();
        seqs.sort();
        // a free seq in the middle quartiles of this dbno's range (a true non-max insert)
        let mut new_refno = None;
        let (lo, hi) = (seqs.len() / 4, (3 * seqs.len() / 4).min(seqs.len().saturating_sub(1)));
        for k in lo..hi {
            let cand = (dbno, seqs[k] + 1);
            if seqs[k + 1] > seqs[k] + 1 && !chk0.keyset.contains(&cand) {
                new_refno = Some(cand);
                break;
            }
        }
        let new_refno = new_refno.expect("a middle gap in the key range");
        assert!(new_refno < maxk, "must be a middle (non-max) key");
        let new_name = "/MID-INSERT-RS";
        cow_insert_element_split(&mut db, bo, new_refno, new_name, None).unwrap();

        let roots = session_roots(&db);
        let chk_new = btree_check(&db, roots[0]);
        let nb = record_off_via_root(&db, roots[0], new_refno).unwrap();
        let nel = decode_full(&db, &ss, nb, &mut HashMap::new());
        assert_eq!(nel.name.as_deref(), Some(new_name), "new element decodes with new name");
        assert_eq!(nel.noun_name, src.noun_name, "cloned noun");
        assert_eq!(nel.owner, src.owner, "cloned owner");
        assert!(record_off_via_root(&db, roots[1], new_refno).is_none(), "absent in prev session");
        assert_eq!(chk_new.count, chk0.count + 1, "exactly one new key");
        assert!(chk_new.balanced && chk_new.nav_ok && chk_new.sorted_ok);
        let edited = db.bytes();
        let escaped: Vec<usize> = (0..orig.len())
            .filter(|&i| orig[i] != edited[i] && !(HDR_LATEST..HDR_LATEST + 4).contains(&i))
            .collect();
        assert!(escaped.is_empty(), "COW must not mutate original pages: {:?}", escaped);
    }

    #[test]
    fn cow_members_add_remove_roundtrip() {
        // Slice 7 (real sam7200): add a child to an element's member list then remove it (stacked
        // commits). Across the 3 sessions the child list goes original -> +marker -> original,
        // member words round-trip, and the original bytes stay immutable.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let mut db = Edb::from_bytes(std::fs::read(DBF).unwrap());
        let orig = std::fs::read(DBF).unwrap();
        let (refno, bo, children0) = find_member_element(&db, 1, 8).expect("an element with members");
        let ps = db.page_size();
        let marker = (0xABCDu32, 0x1234u32);

        let mut kids = children0.clone();
        kids.push(marker);
        cow_members_set(&mut db, bo, &kids, None).unwrap(); // commit #1: add
        let bo1 = record_off_via_root(&db, session_roots(&db)[0], refno).unwrap();
        let after_add = read_members(db.bytes(), bo1, ps);
        assert!(after_add.contains(&marker) && after_add.len() == children0.len() + 1, "marker added");

        let cur: Vec<(u32, u32)> = after_add.iter().copied().filter(|c| *c != marker).collect();
        cow_members_set(&mut db, bo1, &cur, None).unwrap(); // commit #2: remove

        let roots = session_roots(&db);
        let read_via = |root: usize| -> Vec<(u32, u32)> {
            let b = record_off_via_root(&db, root, refno).unwrap();
            read_members(db.bytes(), b, ps)
        };
        assert_eq!(read_via(roots[2]), children0, "original session unchanged");
        let add2 = read_via(roots[1]);
        assert!(add2.contains(&marker) && add2.len() == children0.len() + 1, "+marker session");
        assert_eq!(read_via(roots[0]), children0, "marker removed in newest session");
        let edited = db.bytes();
        let escaped: Vec<usize> = (0..orig.len())
            .filter(|&i| orig[i] != edited[i] && !(HDR_LATEST..HDR_LATEST + 4).contains(&i))
            .collect();
        assert!(escaped.is_empty(), "COW must not mutate original pages: {:?}", escaped);
    }

    #[test]
    fn cow_members_xpage_chain() {
        // Slice 7 (real sam7200): relocate a member list onto fresh page(s) with force_chunk so it
        // becomes a real >=2-node type-2 chain on a DIFFERENT page than the record (cross-page),
        // add a child, and confirm the chain-aware reader lists old children + the new one.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let mut db = Edb::from_bytes(std::fs::read(DBF).unwrap());
        let (refno, bo, children0) = find_member_element(&db, 1, 8).expect("an element with members");
        let ps = db.page_size();
        let marker = (0xABCDu32, 0x1234u32);
        let mut kids = children0.clone();
        kids.push(marker);
        cow_members_set(&mut db, bo, &kids, Some(2)).unwrap(); // 1 child / node -> chain

        let roots = session_roots(&db);
        let nb = record_off_via_root(&db, roots[0], refno).unwrap();
        let mem_pg = be_u32(db.bytes(), nb + 32) as usize; // rec[8] = member page
        assert_ne!(mem_pg, nb / ps, "members relocated cross-page");
        assert!(node_chain_len(db.bytes(), nb, ps, 2) >= 2, "forced multi-node type-2 chain");
        assert_eq!(read_members(db.bytes(), nb, ps), kids, "new session lists old children + marker");
        let ob = record_off_via_root(&db, roots[1], refno).unwrap();
        assert_eq!(read_members(db.bytes(), ob, ps), children0, "prev session unchanged");
    }

    #[test]
    fn resolve_refs_in_db() {
        // Read-side parity with the standalone crate / Python e3d_export: resolve implicit
        // reference attributes (type 4/8/16 = (dbno,refseq)) to target element names via the
        // refmap built by index_db. sam7200 in-db refs (CREF/HREF/TREF -> design elements)
        // recover real connectivity; many resolve to '/'-prefixed names (findings §8.12).
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let db = Edb::open(DBF).unwrap();
        let mut refmap = HashMap::new();
        let elems = index_db(&db, &ss, true, &mut refmap);
        let mut resolved = 0usize;
        for el in &elems {
            for (_attr, tgts) in resolve_refs(el, &refmap) {
                resolved += tgts.iter().filter(|t| t.starts_with('/')).count();
            }
        }
        assert!(resolved >= 100, "expected many in-db refs resolved to names, got {}", resolved);
    }

    #[test]
    fn decode_at_weld() {
        // Public single-element decode wrapper: resolve /WB1's offset then decode it directly.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let db = Edb::open(DBF).unwrap();
        let bo = find_record_offset(&db, &ss, "/WB1").unwrap();
        let el = decode_at(&db, &ss, bo);
        assert_eq!(el.name.as_deref(), Some("/WB1"));
        assert_eq!(el.noun_name, "WELD");
        assert_eq!(el.pos(), Some(&[9630.0, 8072.0, 5282.5][..]));
    }

    #[test]
    fn cow_delete_real() {
        // Slice 4 (real sam7200): delete /WB1's main-record leaf entry. New session no longer
        // resolves it; the previous session still does (multi-version); the new tree stays
        // balanced/sorted/nav_ok with exactly one fewer key; original bytes immutable bar page0.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let orig = std::fs::read(DBF).unwrap();
        let mut db = Edb::from_bytes(orig.clone());
        let bo = find_record_offset(&db, &ss, "/WB1").unwrap();
        let refno = decode_at(&db, &ss, bo).refno;
        let root0 = db.latest_root();
        let chk0 = btree_check(&db, root0);
        assert!(chk0.keyset.contains(&refno), "refno present before delete");

        cow_delete_element(&mut db, refno).unwrap();

        let roots = session_roots(&db);
        assert!(record_off_via_root(&db, roots[0], refno).is_none(), "gone in new session");
        assert!(record_off_via_root(&db, roots[1], refno).is_some(), "still in previous session");
        let chk_new = btree_check(&db, roots[0]);
        assert_eq!(chk_new.count, chk0.count - 1, "exactly one fewer key");
        assert!(!chk_new.keyset.contains(&refno), "refno removed from new tree");
        assert!(chk_new.balanced && chk_new.nav_ok && chk_new.sorted_ok && chk_new.dups == 0);
        let edited = db.bytes();
        let escaped: Vec<usize> = (0..orig.len())
            .filter(|&i| orig[i] != edited[i] && !(HDR_LATEST..HDR_LATEST + 4).contains(&i))
            .collect();
        assert!(escaped.is_empty(), "COW must not mutate original pages: {:?}", escaped);
    }

    #[test]
    fn cow_crud_roundtrip_real() {
        // CRUD round-trip: clone-insert a temp element then delete it. Across 3 sessions the
        // element is absent -> present -> absent, and the leaf key count round-trips.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let mut db = Edb::from_bytes(std::fs::read(DBF).unwrap());
        let bo = find_record_offset(&db, &ss, "/WB1").unwrap();
        let src = decode_at(&db, &ss, bo);
        let chk0 = btree_check(&db, db.latest_root());
        let maxk = *chk0.keyset.iter().max().unwrap();
        let new_refno = (src.refno.0, maxk.1 + 7); // a fresh max-side key
        assert!(!chk0.keyset.contains(&new_refno));

        cow_insert_element_split(&mut db, bo, new_refno, "/CRUD-RS", None).unwrap();
        assert!(record_off_via_root(&db, session_roots(&db)[0], new_refno).is_some(), "present after insert");
        cow_delete_element(&mut db, new_refno).unwrap();

        let roots = session_roots(&db);
        assert!(record_off_via_root(&db, roots[0], new_refno).is_none(), "absent after delete");
        assert!(record_off_via_root(&db, roots[1], new_refno).is_some(), "present in insert session");
        assert!(record_off_via_root(&db, roots[2], new_refno).is_none(), "absent in original session");
        let chk_after = btree_check(&db, roots[0]);
        assert_eq!(chk_after.count, chk0.count, "key count round-trips (insert then delete)");
        assert!(chk_after.balanced && chk_after.nav_ok && chk_after.sorted_ok);
    }

    #[test]
    fn edbwriter_api_real() {
        // Phase 2 ergonomic API: name-oriented writes via EdbWriter with typed E3dError.
        // clone+delete (source same-page DA) then rename + set-pos; missing name -> ElementNotFound.
        if !data_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let mut w = EdbWriter::open(DBF, &ss).unwrap();
        // insert clone of /WB1 (same-page DA source), then delete it
        let (_rep, _new_refno) = w.insert_clone("/WB1", "/WB1-EW-COPY").unwrap();
        assert!(w.element("/WB1-EW-COPY").is_ok(), "clone present after insert");
        w.delete("/WB1-EW-COPY").unwrap();
        assert!(matches!(w.element("/WB1-EW-COPY"), Err(E3dError::ElementNotFound(_))), "clone gone after delete");
        // rename /WB1 then set its POS
        w.rename("/WB1", "/WB1-EW").unwrap();
        assert!(w.element("/WB1-EW").is_ok(), "renamed element resolves");
        assert!(matches!(w.element("/WB1"), Err(E3dError::ElementNotFound(_))), "old name gone");
        w.set_pos("/WB1-EW", [1.0, 2.0, 3.0]).unwrap();
        assert_eq!(w.element("/WB1-EW").unwrap().pos(), Some(&[1.0, 2.0, 3.0][..]), "POS updated");
        // typed error for a missing element
        assert!(matches!(w.rename("/NOPE-XYZ-404", "/x"), Err(E3dError::ElementNotFound(_))));
    }

    #[test]
    fn edbwriter_catalogue_write() {
        // Phase 2 multi-library coverage: write to a CATALOGUE db (acp7002 via catvir.dat), not
        // just the design db — rename the first named element and confirm the round-trip.
        let cat = r"D:\work\plant\pdms-io\test-file\acp7002_0001";
        if !std::path::Path::new(&format!(r"{}\catvir.dat", EXE)).exists() || !std::path::Path::new(cat).exists() {
            eprintln!("[skip] catalogue data absent");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let mut w = EdbWriter::open(cat, &ss).unwrap();
        let mut rm = HashMap::new();
        let elems = index_db(w.db(), &ss, true, &mut rm);
        let some_name = elems.iter().find_map(|e| e.name.clone()).expect("a named catalogue element");
        let new_name = format!("{some_name}-CATW");
        w.rename(&some_name, &new_name).unwrap();
        assert!(w.element(&new_name).is_ok(), "catalogue element renamed in new session");
        assert!(matches!(w.element(&some_name), Err(E3dError::ElementNotFound(_))), "old catalogue name gone");
    }
}
