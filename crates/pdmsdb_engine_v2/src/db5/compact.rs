use std::collections::BTreeMap;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use crate::core::{EngineError, PageId, RecordLoc, RefNo};
use crate::db1::PageStore;
use crate::db2::{HeaderUpdaterV2, SessionBuilderV2};
use crate::db3;
use crate::db4;

#[derive(Debug, Default)]
pub struct CompactStats {
    pub original_pages: u32,
    pub compacted_pages: u32,
    pub records_moved: usize,
    pub sessions_preserved: usize,
}

pub fn compact_database(
    source_path: &Path,
    dest_path: &Path,
    page_size: usize,
) -> Result<CompactStats, EngineError> {
    let handle = crate::db5::open_read_db(
        source_path,
        crate::core::EngineOptions {
            page_size_hint: Some(page_size),
            prefetch_pages: 4,
        },
    )?;

    let latest = handle.latest_session()?;
    let root = latest.index_root;

    let mut src_file = handle.file.borrow_mut();
    let mut src_store = handle.page_store.borrow_mut();

    let all_entries = db3::scan_all_entries(&mut src_file, &mut src_store, root)?;

    let mut dest_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(dest_path)?;

    let header_page = vec![0u8; page_size];
    dest_file.write_all(&header_page)?;

    let mut dest_store = PageStore::new(page_size, 0);
    let mut refno_map: BTreeMap<RefNo, RecordLoc> = BTreeMap::new();
    let mut records_moved = 0usize;

    for entry in &all_entries {
        let raw = db4::read_record_from_loc(&mut src_file, &mut src_store, entry.loc)?;
        let result = db4::write_record(&mut dest_file, &mut dest_store, 1, &raw)?;
        refno_map.insert(entry.refno, result.start);
        records_moved += 1;
    }

    let new_root = dest_store.allocate_page(&mut dest_file, 1)?;
    db3::write_empty_root(&mut dest_file, &mut dest_store, new_root)?;

    for (refno, loc) in &refno_map {
        db3::upsert_refno(&mut dest_file, &mut dest_store, new_root, *refno, *loc)?;
    }

    let file_size_before = dest_file.seek(SeekFrom::End(0))?;
    let last_data_pgno = if file_size_before == 0 {
        0
    } else {
        ((file_size_before / page_size as u64).saturating_sub(1)) as u32
    };

    let session_page = dest_store.allocate_page(&mut dest_file, 1)?;
    let session_bytes = SessionBuilderV2::new(1, 0)
        .end_page(PageId {
            ext_no: 1,
            page_no: last_data_pgno,
        })
        .index_root(new_root)
        .computer_name("COMPACT".to_string())
        .comments("compacted database".to_string())
        .build(page_size);
    dest_store.write_page(&mut dest_file, session_page, &session_bytes)?;
    dest_store.flush_dirty(&mut dest_file)?;

    let final_size = dest_file.seek(SeekFrom::End(0))?;
    let total_pages = (final_size / page_size as u64) as u32;
    HeaderUpdaterV2::update_header(&mut dest_file, session_page.page_no, total_pages)?;

    let src_size = src_file.seek(SeekFrom::End(0))?;
    let original_pages = (src_size / page_size as u64) as u32;

    Ok(CompactStats {
        original_pages,
        compacted_pages: total_pages,
        records_moved,
        sessions_preserved: 1,
    })
}
