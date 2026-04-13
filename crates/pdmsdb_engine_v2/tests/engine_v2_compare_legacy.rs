#![cfg(feature = "legacy-oracle")]

use aios_core::RefU64;
use anyhow::Result;
use pdms_io_legacy::PdmsIO;
use pdmsdb_engine_v2::compare::legacy_oracle::LegacyOracle;
use pdmsdb_engine_v2::{EngineOptions, EngineV2, RefNo};

fn resolve_db() -> Option<std::path::PathBuf> {
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    LegacyOracle::resolve_repo_test_db_path(repo_root, "ams1112_0001")
}

fn sample_refno() -> RefNo {
    RefNo::from_parts(17496, 171138)
}

fn open_legacy(path: &std::path::Path) -> Result<PdmsIO> {
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let db_option_file = repo_root.join("DbOption.toml");
    if db_option_file.exists() {
        unsafe {
            std::env::set_var("DB_OPTION_FILE", &db_option_file);
        }
    }

    let mut io = PdmsIO::new("legacy-oracle", path, true);
    io.open()?;
    io.init_ses_range_map()?;
    Ok(io)
}

#[test]
fn compare_search_hit_with_legacy() -> Result<()> {
    let db_path = match resolve_db() {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return Ok(());
        }
    };

    let handle = EngineV2::open_read(&db_path, EngineOptions::default())?;
    let v2 = handle
        .find_refno(sample_refno(), None)?
        .expect("v2 应命中样本 refno");

    let mut legacy = open_legacy(&db_path)?;
    let legacy_refno = RefU64::from_two_nums(sample_refno().hi(), sample_refno().lo());
    let (legacy_sesno, legacy_offset) = legacy
        .search_latest_refno(legacy_refno, None)
        .expect("legacy 应命中样本 refno");

    assert_eq!(v2.sesno, legacy_sesno);
    assert_eq!(
        v2.loc.page_no as u64 * handle.page_size() as u64 + v2.loc.byte_offset as u64,
        legacy_offset
    );
    Ok(())
}

#[test]
fn compare_record_bytes_with_legacy() -> Result<()> {
    let db_path = match resolve_db() {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return Ok(());
        }
    };

    let handle = EngineV2::open_read(&db_path, EngineOptions::default())?;
    let v2 = handle
        .find_refno(sample_refno(), None)?
        .expect("v2 应命中样本 refno");
    let v2_record = handle.read_record(v2.loc)?;

    let mut legacy = open_legacy(&db_path)?;
    let legacy_refno = RefU64::from_two_nums(sample_refno().hi(), sample_refno().lo());
    let (_, legacy_offset) = legacy
        .search_latest_refno(legacy_refno, None)
        .expect("legacy 应命中样本 refno");
    let legacy_record = legacy.read_element_record_cached(legacy_offset)?;

    assert_eq!(v2_record, legacy_record);
    Ok(())
}

#[test]
fn compare_parsed_refno_and_owner_with_legacy() -> Result<()> {
    let db_path = match resolve_db() {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return Ok(());
        }
    };

    let handle = EngineV2::open_read(&db_path, EngineOptions::default())?;
    let v2 = handle
        .find_refno(sample_refno(), None)?
        .expect("v2 应命中样本 refno");

    let mut legacy = open_legacy(&db_path)?;
    let legacy_refno = RefU64::from_two_nums(sample_refno().hi(), sample_refno().lo());
    let (_, legacy_offset) = legacy
        .search_latest_refno(legacy_refno, None)
        .expect("legacy 应命中样本 refno");
    let legacy_ele = legacy.parse_raw_element(legacy_offset)?;

    let v2_record = handle.read_record(v2.loc)?;
    let mut prefix = 0usize;
    while prefix + 4 <= v2_record.len() {
        let w = &v2_record[prefix..prefix + 4];
        if w == [0, 0, 0, 0] || w == [0, 0, 0, 7] {
            prefix += 4;
        } else {
            break;
        }
    }

    let ref_hi = u32::from_be_bytes(v2_record[prefix + 4..prefix + 8].try_into().unwrap());
    let ref_lo = u32::from_be_bytes(v2_record[prefix + 8..prefix + 12].try_into().unwrap());
    let owner_hi = u32::from_be_bytes(v2_record[prefix + 16..prefix + 20].try_into().unwrap());
    let owner_lo = u32::from_be_bytes(v2_record[prefix + 20..prefix + 24].try_into().unwrap());

    assert_eq!(RefNo::from_parts(ref_hi, ref_lo), sample_refno());
    assert_eq!(legacy_ele.refno.get_0(), ref_hi);
    assert_eq!(legacy_ele.refno.get_1(), ref_lo);
    assert_eq!(legacy_ele.owner.get_0(), owner_hi);
    assert_eq!(legacy_ele.owner.get_1(), owner_lo);
    Ok(())
}
