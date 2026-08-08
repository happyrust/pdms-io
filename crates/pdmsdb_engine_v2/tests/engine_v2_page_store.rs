use std::io::Write;

use pdmsdb_engine_v2::db1::PageStore;
use pdmsdb_engine_v2::PageId;

fn create_test_file(page_size: usize, page_count: u32) -> tempfile::NamedTempFile {
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    for pgno in 0..page_count {
        let mut page = vec![0u8; page_size];
        page[0..4].copy_from_slice(&pgno.to_be_bytes());
        tmp.write_all(&page).unwrap();
    }
    tmp.flush().unwrap();
    tmp
}

#[test]
fn read_page_returns_correct_data() {
    let page_size = 64;
    let tmp = create_test_file(page_size, 8);
    let mut file = std::fs::File::open(tmp.path()).unwrap();
    let mut store = PageStore::new(page_size, 0);

    let data = store
        .read_page(&mut file, PageId { ext_no: 1, page_no: 3 })
        .unwrap();
    assert_eq!(data.len(), page_size);
    let stored_pgno = u32::from_be_bytes(data[0..4].try_into().unwrap());
    assert_eq!(stored_pgno, 3);
}

#[test]
fn read_page_cache_hit() {
    let page_size = 64;
    let tmp = create_test_file(page_size, 4);
    let mut file = std::fs::File::open(tmp.path()).unwrap();
    let mut store = PageStore::new(page_size, 0);

    let pid = PageId { ext_no: 1, page_no: 2 };
    let d1 = store.read_page(&mut file, pid).unwrap();
    let d2 = store.read_page(&mut file, pid).unwrap();
    assert_eq!(d1, d2);

    let stats = store.read_stats();
    assert_eq!(stats.physical_pages_read, 1);
    assert_eq!(stats.cache_misses, 1);
    assert_eq!(stats.cache_hits, 1);
    assert_eq!(stats.bytes_read, page_size as u64);
}

#[test]
fn prefetch_is_included_in_physical_read_stats() {
    let page_size = 64;
    let tmp = create_test_file(page_size, 4);
    let mut file = std::fs::File::open(tmp.path()).unwrap();
    let mut store = PageStore::new(page_size, 1);

    store
        .read_page(&mut file, PageId { ext_no: 1, page_no: 1 })
        .unwrap();

    let stats = store.read_stats();
    assert_eq!(stats.physical_pages_read, 2);
    assert_eq!(stats.prefetched_pages, 1);
    assert_eq!(stats.bytes_read, (page_size * 2) as u64);
}

#[test]
fn lru_eviction_when_cache_full() {
    let page_size = 64;
    let page_count = 20u32;
    let tmp = create_test_file(page_size, page_count);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(tmp.path())
        .unwrap();

    let mut store = PageStore::new_with_cache_size(page_size, 0, 4);

    for pgno in 0..page_count {
        let data = store
            .read_page(&mut file, PageId { ext_no: 1, page_no: pgno })
            .unwrap();
        let stored = u32::from_be_bytes(data[0..4].try_into().unwrap());
        assert_eq!(stored, pgno, "page {} should contain its own page number", pgno);
    }
}

#[test]
fn locked_pages_survive_eviction() {
    let page_size = 64;
    let page_count = 10u32;
    let tmp = create_test_file(page_size, page_count);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(tmp.path())
        .unwrap();

    let mut store = PageStore::new_with_cache_size(page_size, 0, 4);

    let locked_pid = PageId { ext_no: 1, page_no: 0 };
    store.read_page(&mut file, locked_pid).unwrap();
    store.lock_page(locked_pid);

    for pgno in 1..page_count {
        store
            .read_page(&mut file, PageId { ext_no: 1, page_no: pgno })
            .unwrap();
    }

    let still_cached = store.read_page(&mut file, locked_pid).unwrap();
    let stored = u32::from_be_bytes(still_cached[0..4].try_into().unwrap());
    assert_eq!(stored, 0, "locked page should remain in cache");

    store.unlock_page(locked_pid);
}

#[test]
fn write_page_and_flush() {
    let page_size = 64;
    let tmp = create_test_file(page_size, 4);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(tmp.path())
        .unwrap();
    let mut store = PageStore::new(page_size, 0);

    let pid = PageId { ext_no: 1, page_no: 1 };
    let mut new_data = vec![0xFFu8; page_size];
    new_data[0..4].copy_from_slice(&0xDEADBEEFu32.to_be_bytes());
    store.write_page(&mut file, pid, &new_data).unwrap();

    let cached = store.read_page(&mut file, pid).unwrap();
    assert_eq!(cached[0..4], 0xDEADBEEFu32.to_be_bytes());

    let flushed = store.flush_dirty(&mut file).unwrap();
    assert_eq!(flushed, 1);

    drop(store);
    let mut store2 = PageStore::new(page_size, 0);
    let from_disk = store2.read_page(&mut file, pid).unwrap();
    assert_eq!(from_disk[0..4], 0xDEADBEEFu32.to_be_bytes());
}

#[test]
fn cow_snapshot_and_rollback() {
    let page_size = 64;
    let tmp = create_test_file(page_size, 4);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(tmp.path())
        .unwrap();
    let mut store = PageStore::new(page_size, 0);

    let pid = PageId { ext_no: 1, page_no: 2 };
    store.read_page(&mut file, pid).unwrap();

    let mut modified = vec![0xAAu8; page_size];
    modified[0..4].copy_from_slice(&0x12345678u32.to_be_bytes());
    store.write_page(&mut file, pid, &modified).unwrap();

    store.snapshot_cow();

    let mut modified2 = vec![0xBBu8; page_size];
    modified2[0..4].copy_from_slice(&0x87654321u32.to_be_bytes());
    store.write_page(&mut file, pid, &modified2).unwrap();

    let before_rollback = store.read_page(&mut file, pid).unwrap();
    assert_eq!(before_rollback[0..4], 0x87654321u32.to_be_bytes());

    store.rollback_cow();

    let after_rollback = store.read_page(&mut file, pid).unwrap();
    assert_eq!(
        after_rollback[0..4],
        0x12345678u32.to_be_bytes(),
        "rollback should restore to data at snapshot time"
    );
}

#[test]
fn allocate_page_extends_beyond_existing() {
    let page_size = 64;
    let tmp = create_test_file(page_size, 2);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(tmp.path())
        .unwrap();
    let mut store = PageStore::new(page_size, 0);

    let new_page = store.allocate_page(&mut file, 1).unwrap();
    assert!(new_page.page_no >= 2, "new page should be beyond existing pages");

    let mut data = vec![0u8; page_size];
    data[0..4].copy_from_slice(&0xCAFEu32.to_be_bytes());
    store.write_page(&mut file, new_page, &data).unwrap();
    store.flush_dirty(&mut file).unwrap();

    let readback = store.read_page(&mut file, new_page).unwrap();
    assert_eq!(readback[0..4], 0xCAFEu32.to_be_bytes());
}

#[test]
fn discard_cow_keeps_dirty_data() {
    let page_size = 64;
    let tmp = create_test_file(page_size, 4);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(tmp.path())
        .unwrap();
    let mut store = PageStore::new(page_size, 0);

    let pid = PageId { ext_no: 1, page_no: 1 };
    let mut modified = vec![0xCCu8; page_size];
    modified[0..4].copy_from_slice(&0xFACEFEEDu32.to_be_bytes());
    store.write_page(&mut file, pid, &modified).unwrap();

    store.snapshot_cow();
    store.discard_cow();

    let data = store.read_page(&mut file, pid).unwrap();
    assert_eq!(
        data[0..4],
        0xFACEFEEDu32.to_be_bytes(),
        "discard_cow should keep current data intact"
    );
}
