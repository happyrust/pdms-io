use std::fs::OpenOptions;
use std::io::Write;

use anyhow::Result;
use pdmsdb_engine_v2::{EngineOptions, EngineV2, RecordLoc, RefNo};

fn create_minimal_db(path: &std::path::Path, page_size: u32) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    let mut header = vec![0u8; page_size as usize];
    header[0x04..0x08].copy_from_slice(&2u32.to_be_bytes());
    header[0x08..0x0C].copy_from_slice(&1u32.to_be_bytes());
    header[0x2C..0x30].copy_from_slice(&1u32.to_be_bytes());
    header[0x34..0x38].copy_from_slice(&page_size.to_be_bytes());
    file.write_all(&header)?;
    file.flush()?;
    Ok(())
}

#[test]
fn index_upsert_orders_leaf_entries_v2() -> Result<()> {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("engine_v2_index_leaf.db");
    let _ = std::fs::remove_file(&file_path);
    create_minimal_db(&file_path, 512)?;

    let handle = EngineV2::open_write(&file_path, EngineOptions::default())?;
    let root = handle.allocate_page(1)?;
    handle.write_empty_index_root(root)?;

    for (refno, page_no) in [(300u64, 30u32), (100u64, 10u32), (200u64, 20u32)] {
        handle.upsert_refno(
            root,
            RefNo::new(refno),
            RecordLoc {
                ext_no: 1,
                page_no,
                byte_offset: 32,
            },
        )?;
    }

    let hit = handle
        .find_refno_from_root(root, RefNo::new(200))?
        .expect("应命中 200");
    assert_eq!(hit.page_no, 20);
    Ok(())
}

#[test]
fn index_upsert_splits_root_leaf_v2() -> Result<()> {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("engine_v2_index_split.db");
    let _ = std::fs::remove_file(&file_path);
    create_minimal_db(&file_path, 128)?;

    let handle = EngineV2::open_write(
        &file_path,
        EngineOptions {
            page_size_hint: Some(128),
            prefetch_pages: 0,
        },
    )?;
    let root = handle.allocate_page(1)?;
    handle.write_empty_index_root(root)?;

    for i in 1..=24u64 {
        handle.upsert_refno(
            root,
            RefNo::new(i),
            RecordLoc {
                ext_no: 1,
                page_no: (i + 100) as u32,
                byte_offset: 48,
            },
        )?;
    }

    let hit = handle
        .find_refno_from_root(root, RefNo::new(24))?
        .expect("分裂后仍应命中 24");
    assert_eq!(hit.page_no, 124);
    Ok(())
}
