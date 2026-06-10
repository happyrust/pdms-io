//! Standalone CLI: whole-db read + JSON export for E3D / PDMS element dbs, built on the shared
//! `e3d_io` crate (the single source of truth for the offline decoder). This binary previously
//! carried a full *duplicate* of the decoder; it now depends on `e3d_io` and keeps only the
//! CLI / JSON-export logic. Prints stats + a sample, and optionally exports JSON with cross-(db)
//! reference resolution.  (Element read/write is in `crates/e3d_io`; its `e3d-io` bin adds writes.)
//!
//! Usage:  e3d_decode_rs [exe_dir] [db_file] [--json out.json] [--cat <db>]...

use std::collections::{HashMap, HashSet};
use std::fs;

use e3d_io::{index_db, resolve_refs, Attr, Edb, SchemaSet, Val};

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

/// JSON-serialize a decoded attribute value (free fn over `e3d_io::Val`).
fn val_json(v: &Val) -> String {
    match v {
        Val::Reals(v) => format!("[{}]", v.iter().map(|x| fmt_f64(*x)).collect::<Vec<_>>().join(",")),
        Val::Ints(v) => format!("[{}]", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")),
        Val::Refs(v) => format!("[{}]", v.iter().map(|(a, b)| format!("[{},{}]", a, b)).collect::<Vec<_>>().join(",")),
        Val::Bool(b) => b.to_string(),
        Val::Text(s) => json_str(s),
        Val::Null => "null".into(),
    }
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
        parts.push(format!("{}:{}", json_str(&key), val_json(&a.val)));
    }
    format!("{{{}}}", parts.join(","))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
    let exe = pos.first().cloned().unwrap_or_else(|| r"D:\AVEVA\Everything3D2.10".into());
    let dbf = pos.get(1).cloned().unwrap_or_else(|| r"pdms-test-data\sam7200_0001".into());

    let ss = SchemaSet::load(&exe);
    println!("schemas: {}  noun types: {}", ss.schema_count(), ss.noun_count());

    let db = Edb::open(&dbf).expect("open db");
    println!("db page_size={}", db.page_size());

    let mut refmap: HashMap<(u32, u32), (String, String)> = HashMap::new();
    let elems = index_db(&db, &ss, true, &mut refmap);

    // merge catalogue refmaps for cross-db reference resolution
    for cat in &cats {
        if let Ok(cdb) = Edb::open(cat) {
            index_db(&cdb, &ss, false, &mut refmap);
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
        let pos = e.implicit.iter().find(|a| a.hash == pos_hash).map(|a| val_json(&a.val)).unwrap_or_else(|| "-".into());
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

    println!("\n(Python ref: sam7200 -> 10392 elements / 1209 named / 145 noun types)");
}
