use crate::defines::{PAGE_SIZE_2K, PAGE_SIZE_4K};
use crate::io::{IndexMap, PdmsIO};
use aios_core::RefU64;
use std::fs;

#[test]
fn test_index_map_cache_roundtrip_pim1() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let cache_path = dir.path().join("index_map_cache.bin");

    let io = PdmsIO::new("test", dir.path().join("dummy.db"), false);

    let mut index_map: IndexMap = IndexMap::new();
    index_map.insert(RefU64::from_two_nums(1, 2), vec![30, 10, 10]);
    index_map.insert(RefU64::from_two_nums(3, 4), vec![5]);

    io.cache_index_map(&cache_path, &index_map)?;
    let loaded = io.load_cached_index_map(&cache_path)?;

    let mut expected: IndexMap = IndexMap::new();
    expected.insert(RefU64::from_two_nums(1, 2), vec![10, 30]);
    expected.insert(RefU64::from_two_nums(3, 4), vec![5]);

    assert_eq!(loaded, expected);
    Ok(())
}

#[test]
fn test_index_map_cache_page_size_mismatch_rejected() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let cache_path = dir.path().join("index_map_cache.bin");

    let mut io_write = PdmsIO::new("test", dir.path().join("dummy.db"), false);
    io_write.page_size = PAGE_SIZE_2K;

    let mut index_map: IndexMap = IndexMap::new();
    index_map.insert(RefU64::from_two_nums(1, 2), vec![10]);
    io_write.cache_index_map(&cache_path, &index_map)?;

    let mut io_read = PdmsIO::new("test", dir.path().join("dummy.db"), false);
    io_read.page_size = PAGE_SIZE_4K;

    let err = io_read
        .load_cached_index_map(&cache_path)
        .expect_err("expected page_size mismatch error");
    let msg = format!("{:#}", err);
    assert!(msg.contains("page_size"), "err={}", msg);
    assert!(msg.contains("不匹配"), "err={}", msg);
    Ok(())
}

#[test]
fn test_index_map_cache_legacy_format_supported() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let cache_path = dir.path().join("index_map_cache_legacy.bin");

    // 旧格式：count(u32) + repeated {ref0(u32) ref1(u32) loc_count(u32) offsets(u64)*}
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(2u32).to_le_bytes());

    // ref=(1,2), offsets=[9, 1] (unsorted)
    bytes.extend_from_slice(&(1u32).to_le_bytes());
    bytes.extend_from_slice(&(2u32).to_le_bytes());
    bytes.extend_from_slice(&(2u32).to_le_bytes());
    bytes.extend_from_slice(&(9u64).to_le_bytes());
    bytes.extend_from_slice(&(1u64).to_le_bytes());

    // ref=(3,4), offsets=[7]
    bytes.extend_from_slice(&(3u32).to_le_bytes());
    bytes.extend_from_slice(&(4u32).to_le_bytes());
    bytes.extend_from_slice(&(1u32).to_le_bytes());
    bytes.extend_from_slice(&(7u64).to_le_bytes());

    fs::write(&cache_path, bytes)?;

    let io = PdmsIO::new("test", dir.path().join("dummy.db"), false);
    let loaded = io.load_cached_index_map(&cache_path)?;

    let mut expected: IndexMap = IndexMap::new();
    expected.insert(RefU64::from_two_nums(1, 2), vec![1, 9]);
    expected.insert(RefU64::from_two_nums(3, 4), vec![7]);

    assert_eq!(loaded, expected);
    Ok(())
}

#[test]
fn test_index_map_cache_rejects_unknown_version() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let cache_path = dir.path().join("index_map_cache_bad_ver.bin");

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"PIM1");
    bytes.extend_from_slice(&(2u32).to_le_bytes()); // unsupported version
    bytes.extend_from_slice(&(PAGE_SIZE_2K as u32).to_le_bytes());
    bytes.extend_from_slice(&(0u32).to_le_bytes()); // count
    fs::write(&cache_path, bytes)?;

    let mut io = PdmsIO::new("test", dir.path().join("dummy.db"), false);
    io.page_size = PAGE_SIZE_2K;

    let err = io
        .load_cached_index_map(&cache_path)
        .expect_err("expected version error");
    let msg = format!("{:#}", err);
    assert!(msg.contains("不支持"), "err={}", msg);
    Ok(())
}
