use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use aios_core::RefU64;
use anyhow::{Context, Result, bail};
use clap::Parser;
use parse_pdms_db::parse::gen_ref_type_pos_table_scan;
use parse_pdms_db::refno_index::{find_refno_entry, gen_ref_type_pos_table_from_index};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(about = "Compare B-tree refno lookup with the legacy byte-scan table")]
struct Args {
    /// PDMS db file path.
    #[arg(long)]
    db_file: PathBuf,

    /// Optional target refno. Accepts 13246_243899, 13246/243899, or =13246/243899.
    #[arg(long)]
    refno: Option<String>,

    /// Diff the whole index-built table against the legacy byte-scan table.
    #[arg(long)]
    full: bool,

    /// Verify N refnos sampled from the scan table through single-refno B-tree lookup.
    #[arg(long)]
    sample: Option<usize>,
}

#[derive(Debug, Serialize)]
struct EntryView {
    pos: usize,
    noun_hash: i32,
}

#[derive(Debug, Serialize)]
struct SingleCompare {
    refno: String,
    index_entry: Option<EntryView>,
    scan_entry: Option<EntryView>,
    matches: bool,
}

#[derive(Debug, Serialize)]
struct FullCompare {
    scan_count: usize,
    index_count: usize,
    scan_ms: u128,
    index_ms: u128,
    matched: usize,
    mismatched: usize,
    scan_only: usize,
    index_only: usize,
    mismatch_samples: Vec<String>,
    scan_only_samples: Vec<String>,
    index_only_samples: Vec<String>,
    scan_world: String,
    index_world: String,
}

#[derive(Debug, Serialize)]
struct SampleCompare {
    sampled: usize,
    matched: usize,
    deleted_skipped: usize,
    lookup_ms: u128,
    failures: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CompareReport {
    db_file: String,
    file_len: usize,
    single: Option<SingleCompare>,
    full: Option<FullCompare>,
    sample: Option<SampleCompare>,
}

fn parse_refno(input: &str) -> Result<RefU64> {
    let normalized = input.trim().trim_start_matches('=').replace('_', "/");
    let Some((db, ele)) = normalized.split_once('/') else {
        bail!("invalid refno '{input}', expected db/element or db_element");
    };

    Ok(RefU64::from_two_nums(
        db.parse()
            .with_context(|| format!("invalid refno db part in '{input}'"))?,
        ele.parse()
            .with_context(|| format!("invalid refno element part in '{input}'"))?,
    ))
}

fn main() -> Result<()> {
    let args = Args::parse();
    let bytes = fs::read(&args.db_file)
        .with_context(|| format!("read db file {}", args.db_file.display()))?;

    // Default behaviour with no mode flags: full diff plus a 20-refno sample.
    let run_full = args.full || (args.refno.is_none() && args.sample.is_none());
    let sample_count = args
        .sample
        .or_else(|| (args.refno.is_none() && !args.full).then_some(20));

    let scan_start = Instant::now();
    let (scan_table, scan_world) = gen_ref_type_pos_table_scan(&bytes);
    let scan_ms = scan_start.elapsed().as_millis();

    let single = args
        .refno
        .as_deref()
        .map(parse_refno)
        .transpose()?
        .map(|target| {
            let index_entry = find_refno_entry(&bytes, target).map(|entry| EntryView {
                pos: entry.pos,
                noun_hash: entry.noun_hash,
            });
            let scan_entry = scan_table.get(&target).map(|entry| EntryView {
                pos: entry.pos,
                noun_hash: entry.noun_hash,
            });
            let matches = match (&index_entry, &scan_entry) {
                (Some(index), Some(scan)) => {
                    index.pos == scan.pos && index.noun_hash == scan.noun_hash
                }
                (None, None) => true,
                _ => false,
            };
            SingleCompare {
                refno: target.to_e3d_id(),
                index_entry,
                scan_entry,
                matches,
            }
        });

    let full = if run_full {
        let index_start = Instant::now();
        let Some((index_table, index_world)) = gen_ref_type_pos_table_from_index(&bytes) else {
            bail!("index table build failed; the file may need the byte-scan fallback");
        };
        let index_ms = index_start.elapsed().as_millis();

        let mut matched = 0usize;
        let mut mismatched = 0usize;
        let mut scan_only = 0usize;
        let mut mismatch_samples = Vec::new();
        let mut scan_only_samples = Vec::new();
        for kv in scan_table.iter() {
            let refno = *kv.key();
            let scan_entry = kv.value();
            match index_table.get(&refno) {
                Some(index_entry)
                    if index_entry.pos == scan_entry.pos
                        && index_entry.noun_hash == scan_entry.noun_hash =>
                {
                    matched += 1;
                }
                Some(index_entry) => {
                    mismatched += 1;
                    if mismatch_samples.len() < 10 {
                        mismatch_samples.push(format!(
                            "{}: index=({},{:#X}) scan=({},{:#X})",
                            refno.to_e3d_id(),
                            index_entry.pos,
                            index_entry.noun_hash,
                            scan_entry.pos,
                            scan_entry.noun_hash
                        ));
                    }
                }
                None => {
                    scan_only += 1;
                    if scan_only_samples.len() < 10 {
                        scan_only_samples.push(refno.to_e3d_id());
                    }
                }
            }
        }

        let mut index_only = 0usize;
        let mut index_only_samples = Vec::new();
        for kv in index_table.iter() {
            if !scan_table.contains_key(kv.key()) {
                index_only += 1;
                if index_only_samples.len() < 10 {
                    index_only_samples.push(kv.key().to_e3d_id());
                }
            }
        }

        Some(FullCompare {
            scan_count: scan_table.len(),
            index_count: index_table.len(),
            scan_ms,
            index_ms,
            matched,
            mismatched,
            scan_only,
            index_only,
            mismatch_samples,
            scan_only_samples,
            index_only_samples,
            scan_world: scan_world.to_e3d_id(),
            index_world: index_world.to_e3d_id(),
        })
    } else {
        None
    };

    let sample = sample_count.map(|count| {
        // 以 BFS 索引表为基准抽样验证单点查找：scan 表中可能含已删除元素的历史记录,
        // 它们本就不在最新会话索引里,不应计为单点查找失败。
        let bfs_table = gen_ref_type_pos_table_from_index(&bytes).map(|(table, _)| table);
        let mut sampled = 0usize;
        let mut matched = 0usize;
        let mut deleted_skipped = 0usize;
        let mut failures = Vec::new();
        let lookup_start = Instant::now();
        for kv in scan_table.iter().take(count) {
            let refno = *kv.key();
            let scan_entry = kv.value();
            let in_bfs = bfs_table
                .as_ref()
                .map(|table| table.contains_key(&refno))
                .unwrap_or(true);
            if !in_bfs {
                deleted_skipped += 1;
                continue;
            }
            sampled += 1;
            match find_refno_entry(&bytes, refno) {
                Some(index_entry) if index_entry.noun_hash == scan_entry.noun_hash => {
                    matched += 1;
                }
                Some(index_entry) => {
                    if failures.len() < 10 {
                        failures.push(format!(
                            "{}: index=({},{:#X}) scan=({},{:#X})",
                            refno.to_e3d_id(),
                            index_entry.pos,
                            index_entry.noun_hash,
                            scan_entry.pos,
                            scan_entry.noun_hash
                        ));
                    }
                }
                None => {
                    if failures.len() < 10 {
                        failures.push(format!(
                            "{}: index lookup miss (present in BFS table)",
                            refno.to_e3d_id()
                        ));
                    }
                }
            }
        }
        SampleCompare {
            sampled,
            matched,
            deleted_skipped,
            lookup_ms: lookup_start.elapsed().as_millis(),
            failures,
        }
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&CompareReport {
            db_file: args.db_file.display().to_string(),
            file_len: bytes.len(),
            single,
            full,
            sample,
        })?
    );

    Ok(())
}
