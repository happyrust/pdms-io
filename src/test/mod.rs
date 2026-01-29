pub mod test_data;
pub mod test_data_with_members;

use std::path::{Path, PathBuf};
use crate::config::Config;

pub fn resolve_test_db_path(file_name: &str) -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let local = manifest_dir.join("test-file").join(file_name);
    if local.exists() {
        return Some(local);
    }

    let fallback = Config::get_database_path(file_name);
    if Path::new(&fallback).exists() {
        return Some(PathBuf::from(fallback));
    }

    None
}


// 其余模块仅用于单元/集成测试；避免影响库本身的编译与发布。
#[cfg(test)]
pub mod test_parse;
#[cfg(test)]
pub mod test_max_att_version;
#[cfg(test)]
pub mod test_parse_ele;
#[cfg(test)]
pub mod test_ses_data;
#[cfg(test)]
pub mod test_history_data;
#[cfg(test)]
pub mod test_refno_status;
#[cfg(test)]
pub mod test_collect_latest_eles;
#[cfg(test)]
pub mod test_write_integration;
#[cfg(test)]
pub mod test_ses_validate;
#[cfg(test)]
pub mod expression_test_utils;
#[cfg(test)]
pub mod test_case_loader;
#[cfg(test)]
pub mod test_desp_attribute;
