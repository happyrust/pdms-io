use anyhow::Result;
use pdmsdb_engine_v2::compare::legacy_oracle::LegacyOracle;
use pdmsdb_engine_v2::db4::{ElementRecordView, parse_explicit_blocks, parse_member_refs};
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
fn element_record_view_parse() -> Result<()> {
    let db_path = match resolve_db() {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试");
            return Ok(());
        }
    };

    let handle = EngineV2::open_read(&db_path, EngineOptions::default())?;
    let refno = sample_refno();
    let hit = handle
        .find_refno(refno, None)?
        .ok_or_else(|| anyhow::anyhow!("refno 未找到"))?;

    let raw = handle.read_record(hit.loc)?;
    let view = ElementRecordView::from_raw(&raw)?;

    assert_eq!(view.refno, refno);
    assert!(view.noun_hash != 0, "noun_hash 应不为零");
    assert!(view.implicit_data.len() >= 24, "隐式数据应至少包含元素头");

    println!("refno: {:?}", view.refno);
    println!("noun_hash: 0x{:08X}", view.noun_hash);
    println!("owner: {:?}", view.owner);
    println!("implicit_data.len: {}", view.implicit_data.len());
    println!("members_data.len: {}", view.members_data.len());
    println!("explicit_data.len: {}", view.explicit_data.len());

    let children = parse_member_refs(&view.members_data);
    println!("children.len: {}", children.len());

    if !view.explicit_data.is_empty() {
        let blocks = parse_explicit_blocks(&view.explicit_data)?;
        println!("explicit_blocks.count: {}", blocks.len());
        for (i, block) in blocks.iter().enumerate().take(5) {
            println!(
                "  block[{}]: flag=0x{:02X}, hash=0x{:08X}, payload.len={}",
                i,
                block.flag,
                block.hash,
                block.payload.len()
            );
        }
    }

    Ok(())
}

#[test]
fn element_record_view_unit() {
    let mut record = vec![0u8; 64];
    record[0..4].copy_from_slice(&6i32.to_be_bytes());
    record[4..8].copy_from_slice(&100u32.to_be_bytes());
    record[8..12].copy_from_slice(&200u32.to_be_bytes());
    record[12..16].copy_from_slice(&0xDEADBEEFu32.to_be_bytes());
    record[16..20].copy_from_slice(&50u32.to_be_bytes());
    record[20..24].copy_from_slice(&60u32.to_be_bytes());

    let view = ElementRecordView::from_raw(&record).unwrap();
    assert_eq!(view.refno, RefNo::from_parts(100, 200));
    assert_eq!(view.noun_hash, 0xDEADBEEF);
    assert_eq!(view.owner, RefNo::from_parts(50, 60));
    assert_eq!(view.impl_len_words, 6);
}
