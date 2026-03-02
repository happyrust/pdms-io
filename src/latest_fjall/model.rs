use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const WHITELIST_ATTRS: [&str; 5] = ["NAME", "TYPE", "OWNER", "ZONE", "SITE"];
pub const META_BUILD_INFO_KEY: &str = "build_info";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestElementRecord {
    pub refno: String,
    pub sesno: u32,
    pub entity_type: String,
    pub name: String,
    pub owner: String,
    pub attrs: BTreeMap<String, String>,
    pub children: Vec<String>,
    pub source_pgno: u32,
    pub source_offset: u64,
}

impl LatestElementRecord {
    pub fn whitelist_index_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::with_capacity(WHITELIST_ATTRS.len());
        for attr in WHITELIST_ATTRS {
            if let Some(value) = self.attr_value(attr) {
                if !value.is_empty() {
                    pairs.push((normalize_attr_name(attr), normalize_attr_value(value)));
                }
            }
        }
        pairs
    }

    fn attr_value(&self, attr: &str) -> Option<&str> {
        match attr {
            "NAME" => Some(self.name.as_str()),
            "TYPE" => Some(self.entity_type.as_str()),
            "OWNER" => Some(self.owner.as_str()),
            _ => self.attrs.get(attr).map(String::as_str),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildMeta {
    pub db_file: String,
    pub built_at: String,
    pub page_size: usize,
    pub latest_sesno: u32,
    pub root_index_pgno: u32,
    pub total_index_nodes: usize,
    pub total_leaf_nodes: usize,
    pub total_leaf_entries: usize,
    pub valid_leaf_entries: usize,
    pub total_latest_refnos: usize,
    pub parsed_ok: usize,
    pub parsed_failed: usize,
    pub batch_size: usize,
}

#[derive(Debug, Clone, Default)]
pub struct BuildSummary {
    pub db_file: String,
    pub out_dir: String,
    pub latest_sesno: u32,
    pub total_latest_refnos: usize,
    pub parsed_ok: usize,
    pub parsed_failed: usize,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, Default)]
pub struct ParseStats {
    pub total_candidates: usize,
    pub parsed_ok: usize,
    pub parsed_failed: usize,
    pub flush_batches: usize,
    pub flushed_records: usize,
}

pub fn normalize_attr_name(attr: &str) -> String {
    attr.trim().to_ascii_uppercase()
}

pub fn normalize_attr_value(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}
