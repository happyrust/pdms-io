use std::fs::File;

use crate::core::{EngineError, PageId, RefNo};
use crate::db1::PageStore;

use super::index::{IndexPageView, serialize_index_page};

pub fn delete_refno(
    file: &mut File,
    store: &mut PageStore,
    root: PageId,
    target: RefNo,
) -> Result<bool, EngineError> {
    let deleted = delete_recursive(file, store, root, target, true)?;
    if deleted {
        store.flush_dirty(file)?;
    }
    Ok(deleted)
}

fn delete_recursive(
    file: &mut File,
    store: &mut PageStore,
    page_id: PageId,
    target: RefNo,
    is_root: bool,
) -> Result<bool, EngineError> {
    let page_data = store.read_page(file, page_id)?;
    let parsed = IndexPageView::from_page(&page_data)?;

    if parsed.level == 0 {
        return delete_from_leaf(file, store, page_id, parsed, target);
    }

    let normals: Vec<(usize, super::index::IndexEntry)> = parsed
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.is_start_marker())
        .map(|(i, e)| (i, *e))
        .collect();

    if normals.is_empty() {
        return Ok(false);
    }

    let child_page_no = find_child_for_target(&parsed.entries, target);
    let child_page = PageId {
        ext_no: page_id.ext_no,
        page_no: child_page_no,
    };

    let deleted = delete_recursive(file, store, child_page, target, false)?;

    if deleted && !is_root {
        let child_data = store.read_page(file, child_page)?;
        let child_parsed = IndexPageView::from_page(&child_data)?;
        if child_parsed.entries.iter().all(|e| e.is_start_marker()) || child_parsed.entries.is_empty()
        {
            let mut entries = parsed.entries.clone();
            entries.retain(|e| e.page_no != child_page_no || e.is_start_marker());
            let updated = IndexPageView {
                entries,
                ..parsed
            };
            let bytes = serialize_index_page(store.page_size(), &updated)?;
            store.write_page(file, page_id, &bytes)?;
        }
    }

    Ok(deleted)
}

fn delete_from_leaf(
    file: &mut File,
    store: &mut PageStore,
    page_id: PageId,
    parsed: IndexPageView,
    target: RefNo,
) -> Result<bool, EngineError> {
    let original_len = parsed.entries.len();
    let entries: Vec<_> = parsed
        .entries
        .into_iter()
        .filter(|e| e.refno != target)
        .collect();

    if entries.len() == original_len {
        return Ok(false);
    }

    let updated = IndexPageView {
        entries,
        ..parsed
    };
    let bytes = serialize_index_page(store.page_size(), &updated)?;
    store.write_page(file, page_id, &bytes)?;
    Ok(true)
}

fn find_child_for_target(
    entries: &[super::index::IndexEntry],
    target: RefNo,
) -> u32 {
    use std::cmp::Ordering;
    let start_marker = entries.iter().find(|e| e.is_start_marker());
    let normals: Vec<_> = entries.iter().filter(|e| !e.is_start_marker()).collect();

    if normals.is_empty() {
        return start_marker.map(|e| e.page_no).unwrap_or(0);
    }

    if (target.hi(), target.lo()) < (normals[0].refno.hi(), normals[0].refno.lo()) {
        return start_marker
            .map(|e| e.page_no)
            .unwrap_or(normals[0].page_no);
    }

    let mut prev = normals[0];
    for entry in &normals {
        match (target.hi(), target.lo()).cmp(&(entry.refno.hi(), entry.refno.lo())) {
            Ordering::Less => return prev.page_no,
            Ordering::Equal => return entry.page_no,
            Ordering::Greater => prev = entry,
        }
    }

    prev.page_no
}
