use std::path::Path;

use crate::core::{DbHandle, EngineError, EngineOptions, SessionSnapshot};
use crate::db2::SessionChain;

pub fn refresh_sessions(handle: &DbHandle) -> Result<Vec<SessionSnapshot>, EngineError> {
    let mut file = handle.file.borrow_mut();
    let mut store = handle.page_store.borrow_mut();
    SessionChain::walk_latest_backwards(&mut file, &mut store, &handle.header)
}

pub fn reopen_refreshed(
    path: &Path,
    options: EngineOptions,
) -> Result<DbHandle, EngineError> {
    crate::db5::open_read_db(path, options)
}
