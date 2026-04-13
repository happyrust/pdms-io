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
fn engine_v2_index_write_multilevel() -> Result<()> {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("engine_v2_index_multilevel.db");
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

    for i in 1..=64u64 {
        handle.upsert_refno(
            root,
            RefNo::new(i),
            RecordLoc {
                ext_no: 1,
                page_no: (i + 200) as u32,
                byte_offset: 48,
            },
        )?;
    }

    let hit = handle
        .find_refno_from_root(root, RefNo::new(64))?
        .expect("多层 split 后仍应命中 64");
    assert_eq!(hit.page_no, 264);
    Ok(())
}

#[test]
fn engine_v2_index_update_existing() -> Result<()> {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("engine_v2_index_update_existing.db");
    let _ = std::fs::remove_file(&file_path);
    create_minimal_db(&file_path, 256)?;

    let handle = EngineV2::open_write(
        &file_path,
        EngineOptions {
            page_size_hint: Some(256),
            prefetch_pages: 0,
        },
    )?;
    let root = handle.allocate_page(1)?;
    handle.write_empty_index_root(root)?;

    let refno = RefNo::new(900);
    handle.upsert_refno(
        root,
        refno,
        RecordLoc {
            ext_no: 1,
            page_no: 300,
            byte_offset: 48,
        },
    )?;
    handle.upsert_refno(
        root,
        refno,
        RecordLoc {
            ext_no: 1,
            page_no: 301,
            byte_offset: 64,
        },
    )?;

    let hit = handle
        .find_refno_from_root(root, refno)?
        .expect("更新后应命中 refno");
    assert_eq!(hit.page_no, 301);
    assert_eq!(hit.byte_offset, 64);
    Ok(())
}
