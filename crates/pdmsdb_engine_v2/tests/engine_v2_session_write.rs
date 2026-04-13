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
fn commit_session_reopen_v2() -> Result<()> {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("engine_v2_commit_reopen.db");
    let _ = std::fs::remove_file(&file_path);
    create_minimal_db(&file_path, 512)?;

    let writer = EngineV2::open_write(&file_path, EngineOptions::default())?;
    writer.begin_write_session(1, "V2-PC", "first save")?;
    let root = writer.allocate_page(1)?;
    writer.write_empty_index_root(root)?;

    let refno = RefNo::from_parts(0x5555, 0x6666);
    let record = build_record(refno);
    let write = writer.write_record(1, &record)?;
    let root = writer.upsert_refno(root, refno, write.start)?;

    let session = writer.commit_current_session()?;

    assert_eq!(session.sesno, 1);
    assert_eq!(session.index_root.page_no, root.page_no);
    assert_eq!(session.end_page.page_no, write.end_page.page_no);

    let reader = EngineV2::open_read(&file_path, EngineOptions::default())?;
    let latest = reader.latest_session()?;
    assert_eq!(latest.sesno, 1);
    assert_eq!(latest.index_root.page_no, root.page_no);
    assert_eq!(latest.end_page.page_no, write.end_page.page_no);

    let hit = reader
        .find_refno(refno, None)?
        .expect("reopen 后应能命中新写入 refno");
    assert_eq!(hit.loc.page_no, write.start.page_no);

    let read_back = reader.read_record(hit.loc)?;
    assert_eq!(read_back, record);
    Ok(())
}
