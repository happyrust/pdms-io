//! CLI for the fully-offline E3D / PDMS read + write decoder (`e3d_io`).
//!
//! Read:  `show <name>` · `refs <name>`  (decode an element / resolve its references)
//! Write: `rename` · `set-pos` · `delete` · `insert`  — COW + new-session commit, written to a
//!        COPY (`<db>.e3dout`, or `--out FILE`) unless `--inplace` is given. Prior sessions stay
//!        intact (multi-version); see docs/e3d 数据库分析/ §12.6.
//!
//! Usage:
//!   e3d-io <exe_dir> <db_file> <command> [args...] [--out FILE] [--cat DB]... [--inplace]
//!
//! Examples:
//!   e3d-io "D:\AVEVA\Everything3D2.10" sam7200_0001 show /WB1 --cat acp7002_0001
//!   e3d-io <exe> sam7200_0001 rename /WB1 /WB1-NEW
//!   e3d-io <exe> sam7200_0001 set-pos /WB1 100 200 300.5 --out edited.bin
//!   e3d-io <exe> sam7200_0001 insert /WB1 /WB1-COPY
//!   e3d-io <exe> sam7200_0001 delete /WB1

use std::collections::HashMap;

use e3d_io::*;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

const USAGE: &str = "usage: e3d-io <exe_dir> <db_file> <command> [args] [--out FILE] [--cat DB]... [--inplace] [--force] [--yes]\n\
\n\
read commands:\n\
  show <name>                 decode an element (implicit + DA attrs + resolved refs)\n\
  refs <name>                 list the element's resolved reference attributes\n\
write commands (COW; writes to <db>.e3dout unless --inplace):\n\
  rename <name> <new_name>    rename an element\n\
  set-pos <name> <x> <y> <z>  set the element's POS\n\
  delete <name>               delete an element\n\
  insert <clone_name> <new>   clone an element to a fresh refno with name <new>\n\
batch commands (one atomic new session + post-commit verify):\n\
  plan <edits-file>           preview the batch as an element-level diff (no write)\n\
  apply <edits-file>          apply the batch (COW + verify), write copy unless --inplace\n\
\n\
edits-file: one op per line (blank / #comment skipped):\n\
  set-pos <name> <x> <y> <z> | rename <name> <new> | insert <src> <new> | delete <name>\n";

fn fmt_val(v: &Val) -> String {
    match v {
        Val::Reals(r) => format!("{r:?}"),
        Val::Ints(i) => format!("{i:?}"),
        Val::Refs(r) => r.iter().map(|(a, b)| format!("=({a:#X},{b})")).collect::<Vec<_>>().join(" "),
        Val::Bool(b) => b.to_string(),
        Val::Text(s) => format!("{s:?}"),
        Val::Null => "-".into(),
    }
}

/// Build a global refmap from the main db + any catalogue dbs (for cross-(db) ref resolution).
fn build_refmap(ss: &SchemaSet, main: &Edb, cats: &[String]) -> HashMap<(u32, u32), (String, String)> {
    let mut m = HashMap::new();
    index_db(main, ss, false, &mut m);
    for c in cats {
        match Edb::open(c) {
            Ok(cdb) => {
                index_db(&cdb, ss, false, &mut m);
            }
            Err(e) => eprintln!("warn: --cat {c}: {e}"),
        }
    }
    m
}

fn write_out(bytes: &[u8], dbf: &str, out: &Option<String>, inplace: bool, rep: &CowReport) -> Result<(), String> {
    let dst = if inplace {
        dbf.to_string()
    } else {
        out.clone().unwrap_or_else(|| format!("{dbf}.e3dout"))
    };
    std::fs::write(&dst, bytes).map_err(|e| format!("write {dst}: {e}"))?;
    println!(
        "OK  sesno {} -> {}  index_root {} -> {}  wrote {}",
        rep.old_sesno, rep.new_sesno, rep.old_root, rep.new_root, dst
    );
    Ok(())
}

/// One edit op parsed from an edits-file (T035 batch `plan`/`apply`).
#[derive(Debug, PartialEq)]
enum Edit {
    SetPos(String, [f64; 3]),
    Rename(String, String),
    Insert(String, String),
    Delete(String),
}

/// Parse an edits-file body: one op per line; blank lines and `#`-comments are skipped.
fn parse_edits(text: &str) -> Result<Vec<Edit>, String> {
    let mut edits = Vec::new();
    for (lno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let p: Vec<&str> = line.split_whitespace().collect();
        let err = |m: &str| format!("line {}: {m}: {line:?}", lno + 1);
        match p[0] {
            "set-pos" => {
                if p.len() != 5 {
                    return Err(err("set-pos needs <name> <x> <y> <z>"));
                }
                let mut xyz = [0.0f64; 3];
                for k in 0..3 {
                    xyz[k] = p[2 + k].parse().map_err(|_| err("bad number"))?;
                }
                edits.push(Edit::SetPos(p[1].into(), xyz));
            }
            "rename" => {
                if p.len() != 3 {
                    return Err(err("rename needs <name> <new>"));
                }
                edits.push(Edit::Rename(p[1].into(), p[2].into()));
            }
            "insert" => {
                if p.len() != 3 {
                    return Err(err("insert needs <src> <new>"));
                }
                edits.push(Edit::Insert(p[1].into(), p[2].into()));
            }
            "delete" => {
                if p.len() != 2 {
                    return Err(err("delete needs <name>"));
                }
                edits.push(Edit::Delete(p[1].into()));
            }
            other => return Err(err(&format!("unknown op {other:?}"))),
        }
    }
    Ok(edits)
}

/// Apply parsed edits to a writer (used inside `batch` / `dry_run`).
fn apply_edits(w: &mut EdbWriter, edits: &[Edit], ss: &SchemaSet, force: bool) -> Result<(), E3dError> {
    for e in edits {
        match e {
            Edit::SetPos(n, xyz) => {
                w.set_pos(n, *xyz)?;
            }
            Edit::Rename(n, nn) => {
                w.rename(n, nn)?;
            }
            Edit::Insert(s, nn) => {
                w.insert_clone(s, nn)?;
            }
            Edit::Delete(n) => {
                if !force {
                    let refno = w.element(n)?.refno;
                    let g = delete_guards(w.db(), ss, refno);
                    if !g.is_empty() {
                        return Err(E3dError::Write(format!("guarded delete of {n}: {g:?} (use --force)")));
                    }
                }
                w.delete(n)?;
            }
        }
    }
    Ok(())
}

/// Print an element-level diff (for `plan` / `apply`).
fn print_diff(d: &Diff) {
    println!("-- added ({}) --", d.added.len());
    for (r, n) in &d.added {
        println!("  +({:#X},{}) {}", r.0, r.1, n.as_deref().unwrap_or("-"));
    }
    println!("-- removed ({}) --", d.removed.len());
    for (r, n) in &d.removed {
        println!("  -({:#X},{}) {}", r.0, r.1, n.as_deref().unwrap_or("-"));
    }
    println!("-- modified ({}) --", d.modified.len());
    for c in &d.modified {
        println!("  ~({:#X},{}) {}", c.refno.0, c.refno.1, c.name.as_deref().unwrap_or("-"));
        for a in &c.attrs {
            let o = a.old.as_ref().map(fmt_val).unwrap_or_else(|| "-".into());
            let nv = a.new.as_ref().map(fmt_val).unwrap_or_else(|| "-".into());
            println!("      {:<8} {} -> {}", a.name, o, nv);
        }
    }
}

fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let (mut pos, mut out, mut cats, mut inplace) = (Vec::new(), None, Vec::new(), false);
    let (mut force, mut yes) = (false, false);
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--out" => {
                i += 1;
                out = Some(raw.get(i).ok_or("--out needs a path")?.clone());
            }
            "--cat" => {
                i += 1;
                cats.push(raw.get(i).ok_or("--cat needs a db path")?.clone());
            }
            "--inplace" => inplace = true,
            "--force" => force = true,
            "--yes" => yes = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            s => pos.push(s.to_string()),
        }
        i += 1;
    }
    if pos.len() < 3 {
        return Err(format!("too few arguments\n{USAGE}"));
    }
    let (exe, dbf, cmd) = (pos[0].clone(), pos[1].clone(), pos[2].clone());
    let rest = &pos[3..];
    let ss = SchemaSet::load(&exe);
    if ss.schema_count() == 0 {
        eprintln!("warn: no *vir.dat schemas loaded from {exe} (attribute decode will be empty)");
    }

    let need = |n: usize| -> Result<(), String> {
        if rest.len() < n {
            Err(format!("`{cmd}` needs {n} argument(s)\n{USAGE}"))
        } else {
            Ok(())
        }
    };

    match cmd.as_str() {
        "show" => {
            need(1)?;
            let db = Edb::open(&dbf).map_err(|e| format!("open {dbf}: {e}"))?;
            let bo = find_record_offset(&db, &ss, &rest[0]).ok_or_else(|| format!("element {} not found", rest[0]))?;
            let el = decode_at(&db, &ss, bo);
            println!(
                "{} {}  refno=({:#X},{:#X}) owner=({:#X},{:#X})",
                el.noun_name,
                el.name.as_deref().unwrap_or("-"),
                el.refno.0, el.refno.1, el.owner.0, el.owner.1
            );
            println!("-- implicit ({}) --", el.implicit.len());
            for a in &el.implicit {
                println!("  {:<8} {}", a.name.trim(), fmt_val(&a.val));
            }
            println!("-- DA ({}) --", el.da.len());
            for a in &el.da {
                println!("  {:<8} {}", a.name.trim(), fmt_val(&a.val));
            }
            let refmap = build_refmap(&ss, &db, &cats);
            let refs = resolve_refs(&el, &refmap);
            if !refs.is_empty() {
                println!("-- refs --");
                for (n, ts) in refs {
                    println!("  {:<8} {}", n.trim(), ts.join(", "));
                }
            }
        }
        "refs" => {
            need(1)?;
            let db = Edb::open(&dbf).map_err(|e| format!("open {dbf}: {e}"))?;
            let bo = find_record_offset(&db, &ss, &rest[0]).ok_or_else(|| format!("element {} not found", rest[0]))?;
            let el = decode_at(&db, &ss, bo);
            let refmap = build_refmap(&ss, &db, &cats);
            for (n, ts) in resolve_refs(&el, &refmap) {
                println!("{:<8} -> {}", n.trim(), ts.join(", "));
            }
        }
        "rename" => {
            need(2)?;
            let mut w = EdbWriter::open(&dbf, &ss).map_err(|e| e.to_string())?;
            let rep = w.rename(&rest[0], &rest[1]).map_err(|e| e.to_string())?;
            write_out(w.bytes(), &dbf, &out, inplace, &rep)?;
        }
        "set-pos" => {
            need(4)?;
            let mut xyz = [0.0f64; 3];
            for (k, s) in rest[1..4].iter().enumerate() {
                xyz[k] = s.parse::<f64>().map_err(|_| format!("bad number {s:?}"))?;
            }
            let mut w = EdbWriter::open(&dbf, &ss).map_err(|e| e.to_string())?;
            let rep = w.set_pos(&rest[0], xyz).map_err(|e| e.to_string())?;
            write_out(w.bytes(), &dbf, &out, inplace, &rep)?;
        }
        "delete" => {
            need(1)?;
            let mut w = EdbWriter::open(&dbf, &ss).map_err(|e| e.to_string())?;
            let refno = w.element(&rest[0]).map_err(|e| e.to_string())?.refno;
            if !force {
                let g = delete_guards(w.db(), &ss, refno);
                if !g.is_empty() {
                    return Err(format!("refusing to delete {} (use --force): {g:?}", rest[0]));
                }
            }
            let rep = w.delete(&rest[0]).map_err(|e| e.to_string())?;
            write_out(w.bytes(), &dbf, &out, inplace, &rep)?;
        }
        "insert" => {
            need(2)?;
            let mut w = EdbWriter::open(&dbf, &ss).map_err(|e| e.to_string())?;
            let (rep, new_refno) = w.insert_clone(&rest[0], &rest[1]).map_err(|e| e.to_string())?;
            println!("new refno = ({:#X},{:#X})", new_refno.0, new_refno.1);
            write_out(w.bytes(), &dbf, &out, inplace, &rep)?;
        }
        "plan" => {
            need(1)?;
            let body = std::fs::read_to_string(&rest[0]).map_err(|e| format!("read {}: {e}", rest[0]))?;
            let edits = parse_edits(&body)?;
            let w = EdbWriter::open(&dbf, &ss).map_err(|e| e.to_string())?;
            let diff = w.dry_run(|w| apply_edits(w, &edits, &ss, true)).map_err(|e| e.to_string())?;
            println!("plan: {} edit(s) from {}", edits.len(), rest[0]);
            print_diff(&diff);
        }
        "apply" => {
            need(1)?;
            let body = std::fs::read_to_string(&rest[0]).map_err(|e| format!("read {}: {e}", rest[0]))?;
            let edits = parse_edits(&body)?;
            let orig = std::fs::read(&dbf).map_err(|e| format!("open {dbf}: {e}"))?;
            if inplace && !yes {
                return Err("--inplace overwrites the source file; pass --yes to confirm".into());
            }
            let mut w = EdbWriter::from_bytes(orig.clone(), &ss);
            let new_sesno = w.batch(|w| apply_edits(w, &edits, &ss, force)).map_err(|e| e.to_string())?;
            if let Err(issues) = verify_commit(&orig, w.db(), &ss, &[]) {
                return Err(format!("post-commit verify failed: {issues:?}"));
            }
            let dst = if inplace { dbf.clone() } else { out.clone().unwrap_or_else(|| format!("{dbf}.e3dout")) };
            std::fs::write(&dst, w.bytes()).map_err(|e| format!("write {dst}: {e}"))?;
            println!("OK  batch {} edit(s)  sesno -> {}  verify OK  wrote {}", edits.len(), new_sesno, dst);
        }
        other => return Err(format!("unknown command {other:?}\n{USAGE}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_edits_ok_and_errors() {
        let text = "# comment\nset-pos /A 1 2 3.5\n\nrename /A /B\ninsert /A /C\ndelete /D\n";
        let edits = parse_edits(text).unwrap();
        assert_eq!(edits.len(), 4);
        assert_eq!(edits[0], Edit::SetPos("/A".into(), [1.0, 2.0, 3.5]));
        assert_eq!(edits[1], Edit::Rename("/A".into(), "/B".into()));
        assert_eq!(edits[2], Edit::Insert("/A".into(), "/C".into()));
        assert_eq!(edits[3], Edit::Delete("/D".into()));
        assert!(parse_edits("set-pos /A 1 2").is_err(), "too few args");
        assert!(parse_edits("frobnicate /A").is_err(), "unknown op");
        assert!(parse_edits("set-pos /A 1 2 x").is_err(), "bad number");
    }
}
