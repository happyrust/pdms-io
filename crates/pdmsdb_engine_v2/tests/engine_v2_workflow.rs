use std::fs::OpenOptions;
use std::io::Write;

use anyhow::Result;
use pdmsdb_engine_v2::{EngineOptions, EngineV2, RefNo};

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

fn build_header(refno: RefNo, type_hash: i32, owner: RefNo) -> Vec<u8> {
    let mut buf = Vec::with_capacity(24);
    buf.extend_from_slice(&6u32.to_be_bytes());
    buf.extend_from_slice(&refno.raw().to_be_bytes());
    buf.extend_from_slice(&type_hash.to_be_bytes());
    buf.extend_from_slice(&owner.raw().to_be_bytes());
    buf
}

fn build_record(refno: RefNo) -> Vec<u8> {
    let owner = RefNo::from_parts(0x1000, 0x2000);
    let mut record = build_header(refno, 10, owner);
    let block_len_words = 8u16;
    let block_len_bytes = block_len_words as usize * 4;
    let mut block = vec![0u8; block_len_bytes];
    block[0..2].copy_from_slice(&1u16.to_be_bytes());
    block[2..4].copy_from_slice(&block_len_words.to_be_bytes());
    block[4..12].copy_from_slice(&refno.raw().to_be_bytes());
    for (idx, byte) in block[12..].iter_mut().enumerate() {
        *byte = (idx % 251) as u8;
    }
    record.extend_from_slice(&block);
    record.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 7]);
    record
}

#[test]
fn engine_v2_save_work_workflow() -> Result<()> {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("engine_v2_workflow.db");
    let _ = std::fs::remove_file(&file_path);
    create_minimal_db(&file_path, 512)?;

    let handle = EngineV2::open_write(&file_path, EngineOptions::default())?;
    handle.begin_write_session(11, "WF-PC", "workflow save")?;
    let root = handle.create_empty_database_layout()?;
    let refno = RefNo::from_parts(0x7000, 0x0001);
    let record = build_record(refno);
    let write = handle.insert_record(refno, &record)?;
    let session = handle.save_work()?;

    assert_eq!(session.sesno, 11);
    assert_eq!(session.index_root.page_no, root.page_no);
    assert_eq!(session.end_page.page_no, write.end_page.page_no);
    Ok(())
}

#[test]
fn engine_v2_reopen_after_save_work() -> Result<()> {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("engine_v2_workflow_reopen.db");
    let _ = std::fs::remove_file(&file_path);
    create_minimal_db(&file_path, 512)?;

    let handle = EngineV2::open_write(&file_path, EngineOptions::default())?;
    handle.begin_write_session(12, "WF-PC", "workflow reopen")?;
    handle.create_empty_database_layout()?;
    let refno = RefNo::from_parts(0x7000, 0x0002);
    let record = build_record(refno);
    handle.insert_record(refno, &record)?;
    handle.save_work()?;
    drop(handle);

    let reader = EngineV2::open_read(&file_path, EngineOptions::default())?;
    let hit = reader
        .find_refno(refno, None)?
        .expect("save_work 后 reopen 应命中 refno");
    let read_back = reader.read_record(hit.loc)?;
    assert_eq!(read_back, record);
    Ok(())
}

#[test]
fn engine_v2_session_cache_updates_in_place() -> Result<()> {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("engine_v2_session_cache.db");
    let _ = std::fs::remove_file(&file_path);
    create_minimal_db(&file_path, 512)?;

    let handle = EngineV2::open_write(&file_path, EngineOptions::default())?;
    handle.begin_write_session(13, "WF-PC", "cache update")?;
    handle.create_empty_database_layout()?;
    let refno = RefNo::from_parts(0x7000, 0x0003);
    let record = build_record(refno);
    handle.insert_record(refno, &record)?;
    let session = handle.save_work()?;

    let latest = handle.latest_session()?;
    assert_eq!(latest.sesno, session.sesno);
    assert_eq!(latest.page.page_no, session.page.page_no);

    let hit = handle
        .find_refno(refno, None)?
        .expect("当前 handle 无需 reopen 也应命中 refno");
    let read_back = handle.read_record(hit.loc)?;
    assert_eq!(read_back, record);
    Ok(())
}
