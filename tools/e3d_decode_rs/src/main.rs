//! Standalone offline reader/decoder/exporter for E3D/PDMS DABACON databases (std-only).
//!
//! Rust port of the Python toolchain (`e3d_db_reader_v2.py` + `e3d_attr_decoder.py`
//! + `e3d_export.py`): header -> session chain -> B-tree index walk -> per-element
//! decode of noun / NAME / refno / owner / all inline (implicit) named attributes /
//! DA (explicit) attributes, plus cross-(db) reference resolution and JSON export.
//! Fully offline (no runtime / no IDA). See `docs/e3d 数据库分析/`.
//!
//! Usage:  e3d_decode_rs [exe_dir] [db_file] [--json out.json] [--cat <db>]...
//!   exe_dir  = schema dir with *vir.dat   (default D:\AVEVA\Everything3D2.10)
//!   db_file  = element db                 (default pdms-test-data\sam7200_0001)

use std::collections::{HashMap, HashSet};
use std::fs;

const PAGE: usize = 2048; // schema page size
const DATA_WORDS: usize = 511;
const BASE27: u32 = 0x81BF1;
const UDA_THRESHOLD: u32 = 0x171FAD39;
const NAME_HASH: u32 = 0x9C18E;
const INDEX_NOUN: u32 = 0xCC47DF;

fn be_u32(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn rd_words(b: &[u8], off: usize, n: usize) -> Vec<u32> {
    let n = n.min((b.len().saturating_sub(off)) / 4);
    (0..n).map(|i| be_u32(b, off + 4 * i)).collect()
}

/// Chained read for schema dbs (511 data + 1 link word per 512-word page; page P @ (P-1)*2048).
fn chain(b: &[u8], start_page: u32, total: usize) -> Vec<u32> {
    let off = |p: u32| (p as usize - 1) * PAGE;
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

fn db1_dehash(mut h: u32) -> String {
    if h > UDA_THRESHOLD {
        // UDA short-code: ':' + lossy base-64 (core.dll DEHASH 0x1065B930). NOT the real
        // UDA name (which lives in the UDA dictionary db); kept stable & matching Python.
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

/// Decoded attribute value (mirrors the Python decoder's value variants).
#[derive(Clone)]
enum Val {
    Reals(Vec<f64>),
    Ints(Vec<u32>),
    Refs(Vec<(u32, u32)>),
    Bool(bool),
    Text(String),
    Null,
}

impl Val {
    fn is_null(&self) -> bool {
        matches!(self, Val::Null)
    }
    fn to_json(&self) -> String {
        match self {
            Val::Reals(v) => {
                let parts: Vec<String> = v.iter().map(|x| fmt_f64(*x)).collect();
                format!("[{}]", parts.join(","))
            }
            Val::Ints(v) => {
                let parts: Vec<String> = v.iter().map(|x| x.to_string()).collect();
                format!("[{}]", parts.join(","))
            }
            Val::Refs(v) => {
                let parts: Vec<String> = v.iter().map(|(a, b)| format!("[{},{}]", a, b)).collect();
                format!("[{}]", parts.join(","))
            }
            Val::Bool(b) => b.to_string(),
            Val::Text(s) => json_str(s),
            Val::Null => "null".into(),
        }
    }
}

fn fmt_f64(x: f64) -> String {
    if x == x.trunc() && x.abs() < 1e15 {
        format!("{:.1}", x)
    } else {
        format!("{}", x)
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
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

struct Attr {
    hash: u32,
    name: String,
    typ: u32,
    val: Val,
}

/// Port of _decode_one: one inline (implicit) attribute from an element record.
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

/// Port of _parse_attr_words: a flat [hash][ctrl:type<<26|wc][value...] entry stream.
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

/// Port of decode_da_list (which=1, DA/explicit): chain-following node walk.
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
        let avail = ((buf.len().saturating_sub(node + 20)) / 4) as i64;
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

struct Schema {
    buf: Vec<u8>,
    index: HashMap<u32, (u32, u32)>,
}

impl Schema {
    fn open(path: &str) -> std::io::Result<Schema> {
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

struct SchemaSet {
    schemas: Vec<Schema>,
    noun2: HashMap<u32, usize>,
}

impl SchemaSet {
    fn load(dir: &str) -> SchemaSet {
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
            if let Ok(s) = Schema::open(p.to_str().unwrap()) {
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

    fn typedef(&self, noun: u32) -> Option<HashMap<u32, Desc>> {
        self.noun2.get(&noun).and_then(|&i| self.schemas[i].typedef(noun))
    }
}

struct Edb {
    buf: Vec<u8>,
    ps: usize,
}

impl Edb {
    fn open(path: &str) -> std::io::Result<Edb> {
        let buf = fs::read(path)?;
        let mut ps = (be_u32(&buf, 0x34) as usize) * 4;
        if ![512usize, 2048, 4096].contains(&ps) {
            ps = 2048;
        }
        Ok(Edb { buf, ps })
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

/// Full element decode: noun/name/refno/owner + implicit + DA attrs (mirrors decode_full_element).
struct Element {
    refno: (u32, u32),
    noun_name: String,
    owner: (u32, u32),
    name: Option<String>,
    implicit: Vec<Attr>,
    da: Vec<Attr>,
}

fn decode_full(db: &Edb, ss: &SchemaSet, bo: usize, td_cache: &mut HashMap<u32, Option<HashMap<u32, Desc>>>) -> Element {
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
    let name = da
        .iter()
        .find(|a| a.hash == NAME_HASH)
        .and_then(|a| match &a.val {
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

fn index_db(
    db: &Edb,
    ss: &SchemaSet,
    collect: bool,
    refmap: &mut HashMap<(u32, u32), (String, String)>,
    td_cache: &mut HashMap<u32, Option<HashMap<u32, Desc>>>,
) -> Vec<Element> {
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
        let el = decode_full(db, ss, bo, td_cache);
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

/// resolve type 4/8/16 implicit ref attrs -> target names (this db + catalogue refmap).
fn resolve_refs(el: &Element, refmap: &HashMap<(u32, u32), (String, String)>) -> Vec<(String, Vec<String>)> {
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

fn attrs_json(attrs: &[Attr]) -> String {
    let mut parts = Vec::new();
    let mut seen = HashSet::new();
    for a in attrs {
        if a.val.is_null() {
            continue;
        }
        let key = if a.name.is_empty() { format!("0x{:X}", a.hash) } else { a.name.clone() };
        if !seen.insert(key.clone()) {
            continue;
        }
        parts.push(format!("{}:{}", json_str(&key), a.val.to_json()));
    }
    format!("{{{}}}", parts.join(","))
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut json_out: Option<String> = None;
    let mut cats: Vec<String> = Vec::new();
    let mut pos: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                json_out = args.get(i + 1).cloned();
                i += 2;
            }
            "--cat" => {
                if let Some(c) = args.get(i + 1).cloned() {
                    cats.push(c);
                }
                i += 2;
            }
            _ => {
                pos.push(args[i].clone());
                i += 1;
            }
        }
    }
    args.clear();
    let exe = pos.get(0).cloned().unwrap_or_else(|| r"D:\AVEVA\Everything3D2.10".into());
    let dbf = pos.get(1).cloned().unwrap_or_else(|| r"pdms-test-data\sam7200_0001".into());

    let ss = SchemaSet::load(&exe);
    println!("schemas: {}  noun types: {}", ss.schemas.len(), ss.noun2.len());

    let db = Edb::open(&dbf).expect("open db");
    println!("db page_size={}", db.ps);

    let mut refmap: HashMap<(u32, u32), (String, String)> = HashMap::new();
    let mut td_cache: HashMap<u32, Option<HashMap<u32, Desc>>> = HashMap::new();
    let elems = index_db(&db, &ss, true, &mut refmap, &mut td_cache);

    // merge catalogue refmaps for cross-db reference resolution
    for cat in &cats {
        if let Ok(cdb) = Edb::open(cat) {
            let mut cm = HashMap::new();
            index_db(&cdb, &ss, false, &mut cm, &mut td_cache);
            refmap.extend(cm);
        }
    }

    let mut hist: HashMap<String, usize> = HashMap::new();
    let mut named = 0usize;
    for e in &elems {
        *hist.entry(e.noun_name.clone()).or_insert(0) += 1;
        if e.name.is_some() {
            named += 1;
        }
    }
    println!("\nelements={}  named={}  noun_types={}  refmap={}", elems.len(), named, hist.len(), refmap.len());
    let mut top: Vec<_> = hist.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    print!("top nouns:");
    for (n, c) in top.iter().take(12) {
        print!(" {}={}", n, c);
    }
    println!();

    println!("\nsample named elements (noun name POS / refs):");
    let pos_hash = 0x853B1u32;
    let mut shown = 0;
    for e in &elems {
        if shown >= 6 || e.name.is_none() {
            continue;
        }
        let pos = e.implicit.iter().find(|a| a.hash == pos_hash).map(|a| a.val.to_json()).unwrap_or_else(|| "-".into());
        let refs = resolve_refs(e, &refmap);
        let refstr: Vec<String> = refs.iter().take(2).map(|(k, v)| format!("{}:{}", k, v.join(","))).collect();
        println!("  {:<6} {:<24} POS={} {}", e.noun_name, e.name.as_deref().unwrap_or(""), pos, refstr.join(" "));
        shown += 1;
    }

    if let Some(out) = json_out {
        let mut s = String::new();
        s.push_str(&format!(
            "{{\n \"file\": {},\n \"element_count\": {},\n \"named\": {},\n \"noun_types\": {},\n \"refmap_size\": {},\n \"elements\": [\n",
            json_str(&dbf), elems.len(), named, hist.len(), refmap.len()
        ));
        for (idx, e) in elems.iter().enumerate() {
            let refs = resolve_refs(e, &refmap);
            let refs_json = if refs.is_empty() {
                String::new()
            } else {
                let parts: Vec<String> = refs
                    .iter()
                    .map(|(k, v)| {
                        let vs: Vec<String> = v.iter().map(|x| json_str(x)).collect();
                        format!("{}:[{}]", json_str(k), vs.join(","))
                    })
                    .collect();
                format!(",\"refs\":{{{}}}", parts.join(","))
            };
            s.push_str(&format!(
                "  {{\"refno\":\"{}/{}\",\"noun\":{},\"name\":{},\"owner\":\"{}/{}\",\"implicit\":{},\"explicit\":{}{}}}",
                e.refno.0, e.refno.1, json_str(&e.noun_name),
                e.name.as_ref().map(|n| json_str(n)).unwrap_or_else(|| "null".into()),
                e.owner.0, e.owner.1, attrs_json(&e.implicit), attrs_json(&e.da), refs_json
            ));
            s.push_str(if idx + 1 < elems.len() { ",\n" } else { "\n" });
        }
        s.push_str(" ]\n}\n");
        fs::write(&out, s).expect("write json");
        println!("\nexported JSON -> {}", out);
    }

    println!("\n(Python ref: sam7200 -> 6536 elements / 1144 named / 140 noun types)");
}

#[cfg(test)]
mod tests {
    use super::*;

    // Self-verifying regression: requires the local AVEVA schema dir + sam7200 sample.
    // Skips gracefully (passes) when the data is absent, so it never breaks a bare checkout.
    const EXE: &str = r"D:\AVEVA\Everything3D2.10";
    const DBF: &str = r"D:\work\plant\pdms-io\pdms-test-data\sam7200_0001";

    fn data_present() -> bool {
        std::path::Path::new(&format!(r"{}\desvir.dat", EXE)).exists()
            && std::path::Path::new(DBF).exists()
    }

    #[test]
    fn dehash_roundtrip() {
        assert_eq!(db1_dehash(0x9C18E).trim(), "NAME");
        assert_eq!(db1_dehash(0x853B1).trim(), "POS");
        assert_eq!(db1_dehash(0x97247).trim(), "WELD");
        assert_eq!(db1_dehash(0x9CAF3).trim(), "PIPE");
    }

    #[test]
    fn sam7200_counts_and_weld_pos_match_python() {
        if !data_present() {
            eprintln!("[skip] AVEVA schema / sam7200 sample not present");
            return;
        }
        let ss = SchemaSet::load(EXE);
        let db = Edb::open(DBF).unwrap();
        let mut refmap = HashMap::new();
        let mut td = HashMap::new();
        let elems = index_db(&db, &ss, true, &mut refmap, &mut td);

        // counts must match the Python toolchain exactly
        assert_eq!(elems.len(), 6536, "element count");
        let named = elems.iter().filter(|e| e.name.is_some()).count();
        assert_eq!(named, 1144, "named count");
        let noun_types: HashSet<&str> = elems.iter().map(|e| e.noun_name.as_str()).collect();
        assert_eq!(noun_types.len(), 140, "noun type count");

        // canonical value: WELD /WB1 POS = (9630, 8072, 5282.5)
        let wb1 = elems
            .iter()
            .find(|e| e.name.as_deref() == Some("/WB1"))
            .expect("WELD /WB1 element");
        let pos = wb1.implicit.iter().find(|a| a.hash == 0x853B1).expect("POS attr");
        match &pos.val {
            Val::Reals(v) => assert_eq!(v, &vec![9630.0, 8072.0, 5282.5], "WELD /WB1 POS"),
            _ => panic!("POS not real"),
        }
    }
}
