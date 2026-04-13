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

fn build_small_record(refno: RefNo) -> Vec<u8> {
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

fn build_large_block_record(refno: RefNo) -> Vec<u8> {
    let owner = RefNo::from_parts(0x1234, 0x5678);
    let mut record = build_header(refno, 10, owner);
    let block_len_words = 180u16;
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

fn extract_refno(record: &[u8]) -> RefNo {
    let hi = u32::from_be_bytes(record[4..8].try_into().unwrap());
    let lo = u32::from_be_bytes(record[8..12].try_into().unwrap());
    RefNo::from_parts(hi, lo)
}

#[test]
fn write_record_single_page_v2() -> Result<()> {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("engine_v2_single_page.db");
    let _ = std::fs::remove_file(&file_path);
    create_minimal_db(&file_path, 512)?;

    let handle = EngineV2::open_write(&file_path, EngineOptions::default())?;
    let refno = RefNo::from_parts(0x2222, 0x1111);
    let record = build_small_record(refno);
    let written = handle.write_record(1, &record)?;

    assert_eq!(written.start.page_no, 1);
    assert_eq!(written.pages_used, 1);

    let read_back = handle.read_record(written.start)?;
    assert_eq!(read_back, record);
    Ok(())
}

#[test]
fn write_record_cross_page_v2() -> Result<()> {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("engine_v2_cross_page.db");
    let _ = std::fs::remove_file(&file_path);
    create_minimal_db(&file_path, 512)?;

    let handle = EngineV2::open_write(&file_path, EngineOptions::default())?;
    let refno = RefNo::from_parts(0x3333, 0x4444);
    let record = build_large_block_record(refno);
    let written = handle.write_record(1, &record)?;

    assert!(written.end_page.page_no > written.start.page_no);
    assert!(written.pages_used >= 2);

    let read_back = handle.read_record(written.start)?;
    assert!(read_back.len() >= record.len());
    assert_eq!(extract_refno(&read_back), refno);
    assert_eq!(&read_back[..24], &record[..24]);
    assert!(read_back.windows(4).any(|w| w == [0, 0, 0, 7]));
    Ok(())
}
