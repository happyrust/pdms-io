pub mod compact;
pub mod mark;
pub mod refresh;

use std::collections::HashMap;
use std::io::Seek;
use std::path::Path;

use crate::core::{
    CommitSessionRequest, DbHandle, EngineError, EngineOptions, PageId, SessionSnapshot,
};
use crate::db1::PageStore;
use crate::db2::{HeaderUpdaterV2, HeaderView, SessionBuilderV2, SessionChain, SessionPageView};

pub fn open_read_db(path: &Path, options: EngineOptions) -> Result<DbHandle, EngineError> {
    let file = std::fs::OpenOptions::new().read(true).open(path)?;
    let mut file = file;
    build_handle(path, options, &mut file)
}

pub fn open_write_db(path: &Path, options: EngineOptions) -> Result<DbHandle, EngineError> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    let mut file = file;
    build_handle(path, options, &mut file)
}

fn build_handle(
    path: &Path,
    options: EngineOptions,
    file: &mut std::fs::File,
) -> Result<DbHandle, EngineError> {
    let header = HeaderView::read_from(&mut *file)?;
    let page_size =
        PageStore::validate_page_size_from_header(&mut *file, &header, options.page_size_hint)?;
    let mut page_store = PageStore::new(page_size, options.prefetch_pages.max(1));
    let sessions = SessionChain::walk_latest_backwards(&mut *file, &mut page_store, &header)?;
    let session_ranges = SessionChain::build_session_ranges(&sessions);

    let extent_files = scan_extent_files(path)?;

    Ok(DbHandle {
        path: path.to_path_buf(),
        file: std::cell::RefCell::new((*file).try_clone()?),
        extent_files: std::cell::RefCell::new(extent_files),
        page_store: std::cell::RefCell::new(page_store),
        header,
        sessions,
        session_ranges,
        write_context: std::cell::RefCell::new(None),
        latest_session_cache: std::cell::RefCell::new(None),
        latest_session_range: std::cell::RefCell::new(None),
        current_index_root: std::cell::RefCell::new(None),
        transaction_manager: std::cell::RefCell::new(crate::db5::mark::TransactionManager::new()),
    })
}

fn scan_extent_files(primary_path: &Path) -> Result<HashMap<u32, std::fs::File>, EngineError> {
    let mut extent_files = HashMap::new();

    let stem = primary_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let parent = primary_path.parent().unwrap_or(Path::new("."));

    if stem.len() < 5 {
        return Ok(extent_files);
    }

    let base = &stem[..stem.len() - 4];
    let suffix_str = &stem[stem.len() - 4..];
    if suffix_str.chars().all(|c| c.is_ascii_digit()) {
        for ext_no in 2..=999u32 {
            let ext_name = format!("{}{:04}", base, ext_no);
            let ext_path = parent.join(&ext_name);
            if ext_path.exists() {
                let file = std::fs::OpenOptions::new().read(true).open(&ext_path)?;
                extent_files.insert(ext_no, file);
            } else {
                break;
            }
        }
    }

    Ok(extent_files)
}

pub fn commit_session(
    file: &mut std::fs::File,
    store: &mut PageStore,
    page_size: usize,
    request: CommitSessionRequest,
) -> Result<SessionSnapshot, EngineError> {
    store.flush_dirty(file)?;

    let session_page = store.allocate_page(file, 1)?;
    let session_bytes = SessionBuilderV2::new(
        request.sesno,
        request.last_session.map(|page| page.page_no).unwrap_or(0),
    )
    .end_page(request.end_page)
    .index_root(request.index_root)
    .claim_root(request.claim_root)
    .computer_name(request.computer_name)
    .comments(request.comments)
    .build(page_size);

    store.write_page(file, session_page, &session_bytes)?;
    store.flush_dirty(file)?;

    let file_size = file.seek(std::io::SeekFrom::End(0))?;
    let page_count = (file_size / page_size as u64) as u32;
    HeaderUpdaterV2::update_header(file, session_page.page_no, page_count)?;

    let page = store.read_page(file, session_page)?;
    let view = SessionPageView::from_page(&page)?;
    Ok(SessionSnapshot {
        sesno: view.sesno,
        page: session_page,
        last_session: (view.last_ses_pageno > 0).then_some(PageId {
            ext_no: view.last_ses_extno.max(1),
            page_no: view.last_ses_pageno as u32,
        }),
        end_page: PageId {
            ext_no: view.end_extno.max(1),
            page_no: view.end_pgno,
        },
        index_root: PageId {
            ext_no: view.index_root_extno.max(1),
            page_no: view.index_root_pageno,
        },
        claim_root: (view.claim_pageno != 0).then_some(PageId {
            ext_no: view.claim_extno.max(1),
            page_no: view.claim_pageno,
        }),
    })
}
