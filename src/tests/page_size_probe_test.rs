use crate::defines::{PdmsHeader, PageType, PAGE_SIZE_2K, PAGE_SIZE_512};
use crate::io::PdmsIO;
use std::fs;
use std::mem::size_of;

#[test]
fn test_open_page_size_probe_prefers_session_page_type() -> anyhow::Result<()> {
    // 构造一个“头部 page_size 字段不可信”的最小文件：
    // - header.page_size = 512
    // - header.session_page_no = 4
    // - 在 offset = 4 * 2048 的位置写入 Session page_type=3
    // 期望：PdmsIO::open 通过探测读到 page_type=3，从而选择 2K 页面。

    let mut header = PdmsHeader::default();
    header.db_num = 1;
    header.session_page_no = 4;
    header.latest_ses_pgno = 4;
    header.page_size = PAGE_SIZE_512 as u32; // 故意写“错”

    let session_pgno = header.session_page_no;
    let header_bytes: Vec<u8> = header.try_into().unwrap();
    assert_eq!(header_bytes.len(), size_of::<PdmsHeader>());

    let dir = tempfile::tempdir()?;
    let path = dir.path().join("page_size_probe.bin");

    // 预分配文件内容，确保 4 * 2048 位置可写。
    let mut data = vec![0u8; PAGE_SIZE_2K * 8];
    data[..header_bytes.len()].copy_from_slice(&header_bytes);

    let session_off = session_pgno as usize * PAGE_SIZE_2K;
    data[session_off..session_off + 4].copy_from_slice(&(PageType::Session as i32).to_be_bytes());

    fs::write(&path, data)?;

    let mut io = PdmsIO::new("test", &path, false);
    io.open()?;
    assert_eq!(io.page_size, PAGE_SIZE_2K);
    Ok(())
}
