//! specs/005 T202——门面增量路径(链式重组流)与 e3d_io 解码对齐(契约 F1-I4/F2-A1)。
//!
//! ① 对齐抽样:sam7200 具名 150 + 无名 60 元素,门面 `auto_get_raw_element`(经链式流)
//!   解出的 refno/name/children 与 e3d_io 真相(index_db/read_members)逐项一致;
//! ② R5 门面级终证:writeback 改名(DA 重定位到远页)后,门面解析读出**新名**——
//!   002 时代的窗口流在此必然读旧名/丢 NAME(已由 e3d_io 侧 R5 双断言证明盲区)。

use std::collections::HashMap;

use pdms_io::io::PdmsIO;
use pdms_io::writeback_core::{EditOp, WriteMode, apply_writeback_file};

const EXE: &str = r"D:\AVEVA\Everything3D2.10";
const SAM: &str = r"D:\work\plant\pdms-io\pdms-test-data\sam7200_0001";

fn data_present() -> bool {
    std::path::Path::new(&format!(r"{}\desvir.dat", EXE)).exists()
        && std::path::Path::new(SAM).exists()
}

#[test]
fn chained_facade_aligns_with_e3d_io_truth() -> anyhow::Result<()> {
    if !data_present() {
        eprintln!("[skip] data absent");
        return Ok(());
    }
    let ss = e3d_io::SchemaSet::load(EXE);
    let db = e3d_io::Edb::open(SAM)?;
    let mut rm = HashMap::new();
    let elems = e3d_io::index_db(&db, &ss, true, &mut rm);
    let root = db.latest_root();

    let mut io = PdmsIO::new("sam", SAM, false);
    io.open()?;

    let named = elems.iter().filter(|e| e.name.is_some()).take(150);
    let unnamed = elems.iter().filter(|e| e.name.is_none()).take(60);
    let mut checked = 0usize;
    for e in named.chain(unnamed) {
        let refno = aios_core::RefU64::from_two_nums(e.refno.0, e.refno.1);
        // 个别 noun 可能缺 attr-info 映射(既有行为:解析报错)——跳过,不计入对齐样本。
        let Ok(ed) = io.auto_get_raw_element(refno) else {
            continue;
        };
        assert_eq!((ed.refno.get_0(), ed.refno.get_1()), e.refno, "refno roundtrip");
        if let Some(n) = &e.name {
            assert_eq!(&ed.name, n, "name mismatch for {:?}", e.refno);
        }
        if let Some(bo) = e3d_io::record_off_via_root(&db, root, e.refno) {
            let truth: Vec<(u32, u32)> = e3d_io::read_members(db.bytes(), bo, db.page_size())
                .into_iter()
                .filter(|p| *p != (0, 0))
                .collect();
            let got: Vec<(u32, u32)> = ed
                .children
                .0
                .iter()
                .map(|r| (r.get_0(), r.get_1()))
                .filter(|p| *p != (0, 0))
                .collect();
            assert_eq!(got, truth, "children mismatch for {:?}", e.refno);
        }
        checked += 1;
    }
    assert!(checked >= 120, "expected >=120 aligned samples, got {checked}");
    println!("[T202] aligned samples: {checked}");
    Ok(())
}

#[test]
fn chained_facade_sees_writeback_rename() -> anyhow::Result<()> {
    if !data_present() {
        eprintln!("[skip] data absent");
        return Ok(());
    }
    let ss = e3d_io::SchemaSet::load(EXE);
    let db = e3d_io::Edb::open(SAM)?;
    let mut rm = HashMap::new();
    let elems = e3d_io::index_db(&db, &ss, true, &mut rm);
    let target = elems.iter().find(|e| e.name.as_deref() == Some("/WB1")).unwrap().refno;

    let tmp = std::env::temp_dir().join(format!("t202_{}_sam7200", std::process::id()));
    std::fs::copy(SAM, &tmp)?;
    let (out_path, _rep) = apply_writeback_file(
        &tmp,
        &ss,
        &[EditOp::Rename { refno: target, new_name: "/WB1-T202".to_string() }],
        WriteMode::Copy,
    )?;

    let mut io = PdmsIO::new("sam", &out_path, false);
    io.open()?;
    let ed = io.auto_get_raw_element(aios_core::RefU64::from_two_nums(target.0, target.1))?;
    assert_eq!(ed.name, "/WB1-T202", "R5 fixed at facade level: chained stream sees far-page DA");

    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(&out_path);
    Ok(())
}
