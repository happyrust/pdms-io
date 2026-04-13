use std::path::{Path, PathBuf};

pub struct LegacyOracle;

impl LegacyOracle {
    pub fn resolve_repo_test_db_path(
        repo_root: impl AsRef<Path>,
        file_name: &str,
    ) -> Option<PathBuf> {
        let local = repo_root.as_ref().join("test-file").join(file_name);
        local.exists().then_some(local)
    }
}
