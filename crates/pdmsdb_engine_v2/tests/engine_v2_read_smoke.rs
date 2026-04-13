use anyhow::Result;
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

#[test]
fn open_read_smoke_v2() -> Result<()> {
    let db_path = match resolve_db() {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return Ok(());
        }
    };

    let handle = EngineV2::open_read(&db_path, EngineOptions::default())?;
    assert_eq!(handle.page_size(), 2048);
    let latest = handle.latest_session()?;
    assert_eq!(latest.page.page_no, handle.header().latest_ses_pgno);
    Ok(())
}

#[test]
fn session_chain_walk_v2() -> Result<()> {
    let db_path = match resolve_db() {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return Ok(());
        }
    };

    let handle = EngineV2::open_read(&db_path, EngineOptions::default())?;
    assert!(!handle.sessions().is_empty(), "session 链不应为空");
    let latest = handle.latest_session()?;
    assert!(latest.sesno > 0);
    Ok(())
}

#[test]
fn search_refno_hit_v2() -> Result<()> {
    let db_path = match resolve_db() {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return Ok(());
        }
    };

    let handle = EngineV2::open_read(&db_path, EngineOptions::default())?;
    let actual = handle
        .find_refno(sample_refno(), None)?
        .expect("v2 应命中样本 refno");

    assert!(actual.sesno > 0);
    assert!(actual.loc.page_no > 0);
    assert_eq!(actual.loc.ext_no, 1);
    Ok(())
}

#[test]
fn read_record_roundtrip_v2() -> Result<()> {
    let db_path = match resolve_db() {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return Ok(());
        }
    };

    let handle = EngineV2::open_read(&db_path, EngineOptions::default())?;
    let actual = handle
        .find_refno(sample_refno(), None)?
        .expect("v2 应命中样本 refno");
    let actual_record = handle.read_record(actual.loc)?;

    assert!(actual_record.len() >= 24);

    let mut prefix = 0usize;
    while prefix + 4 <= actual_record.len() {
        let w = &actual_record[prefix..prefix + 4];
        if w == [0, 0, 0, 0] || w == [0, 0, 0, 7] {
            prefix += 4;
        } else {
            break;
        }
    }

    let hi = u32::from_be_bytes(actual_record[prefix + 4..prefix + 8].try_into().unwrap());
    let lo = u32::from_be_bytes(actual_record[prefix + 8..prefix + 12].try_into().unwrap());
    assert_eq!(RefNo::from_parts(hi, lo), sample_refno());
    Ok(())
}
