use crate::engine_v2::io_layer::FileHandle;
use crate::engine_v2::types::*;

/// 创建新索引表 (空 B-树根节点)
pub fn create_new_table(handle: &mut FileHandle, page_size: usize) -> DbResult<u32> {
    let page_no = handle.total_pages();
    let mut data = vec![0u8; page_size];

    let page_type: u32 = 1;
    data[0..4].copy_from_slice(&page_type.to_be_bytes());
    data[4..8].copy_from_slice(&INDEX_PAGE_NOUN.to_be_bytes());
    let level: u32 = 0;
    data[8..12].copy_from_slice(&level.to_be_bytes());
    let count: u32 = 0;
    data[16..20].copy_from_slice(&count.to_be_bytes());

    handle.write_page(page_no, &data)?;
    handle.refresh_len()?;

    Ok(page_no)
}
