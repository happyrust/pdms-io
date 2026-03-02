pub mod collector;
pub mod index_scan;
pub mod model;
pub mod store;

use crate::io::PdmsIO;
use anyhow::{anyhow, Result};
use collector::collect_latest_records_in_batches;
use index_scan::scan_latest_refno_offsets;
use model::{BuildMeta, BuildSummary};
use std::path::Path;
use std::time::Instant;
use store::LatestFjallStore;

pub use model::{LatestElementRecord, WHITELIST_ATTRS};

fn infer_project_name(db_file: &Path) -> String {
    db_file
        .file_name()
        .and_then(|s| s.to_str())
        .map(|name| name.chars().take(3).collect::<String>())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "ams".to_string())
}

pub async fn build_latest_fjall_from_db(
    db_file: &Path,
    out_dir: &Path,
    batch_size: usize,
) -> Result<BuildSummary> {
    if !db_file.exists() {
        return Err(anyhow!("db 文件不存在: {}", db_file.display()));
    }

    let started = Instant::now();
    let project_name = infer_project_name(db_file);
    let mut io = PdmsIO::new(project_name, db_file, false);
    io.open()?;

    let latest_sesno = io.get_latest_sesno()?;
    let scan = scan_latest_refno_offsets(&mut io)?;

    let store = LatestFjallStore::open(out_dir)?;
    store.reset()?;

    let parse_stats = collect_latest_records_in_batches(
        &mut io,
        &scan.latest_locs,
        batch_size.max(1),
        |records| store.write_records_batch(records),
    )
    .await?;

    let meta = BuildMeta {
        db_file: db_file.display().to_string(),
        built_at: chrono::Utc::now().to_rfc3339(),
        page_size: io.page_size,
        latest_sesno,
        root_index_pgno: scan.root_pgno,
        total_index_nodes: scan.stats.total_nodes,
        total_leaf_nodes: scan.stats.leaf_nodes,
        total_leaf_entries: scan.stats.total_leaf_entries,
        valid_leaf_entries: scan.stats.valid_leaf_entries,
        total_latest_refnos: scan.stats.latest_refnos,
        parsed_ok: parse_stats.parsed_ok,
        parsed_failed: parse_stats.parsed_failed,
        batch_size: batch_size.max(1),
    };

    store.write_meta(&meta)?;
    store.persist()?;

    Ok(BuildSummary {
        db_file: db_file.display().to_string(),
        out_dir: out_dir.display().to_string(),
        latest_sesno,
        total_latest_refnos: scan.stats.latest_refnos,
        parsed_ok: parse_stats.parsed_ok,
        parsed_failed: parse_stats.parsed_failed,
        elapsed_ms: started.elapsed().as_millis(),
    })
}
