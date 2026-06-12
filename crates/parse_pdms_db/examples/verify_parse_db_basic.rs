use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use aios_core::RefU64;
use anyhow::{Context, Result, bail};
use clap::Parser;
use dashmap::DashMap;
use parse_pdms_db::parse::{gen_ref_type_pos_table_scan, parse_db_basic_data, parse_ele_membs};
use rayon::prelude::*;
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(
    about = "Run parse_db_basic_data (index-backed) and diff children against the legacy scan path"
)]
struct Args {
    /// PDMS db file path.
    #[arg(long)]
    db_file: PathBuf,

    /// Optional refno whose children are printed, e.g. 24381/145018.
    #[arg(long)]
    refno: Option<String>,
}

#[derive(Debug, Serialize)]
struct VerifyReport {
    db_file: String,
    index_refnos: usize,
    scan_refnos: usize,
    index_basic_ms: u128,
    scan_children_ms: u128,
    common_refnos: usize,
    children_equal: usize,
    children_diff: usize,
    children_diff_samples: Vec<String>,
    target_refno: Option<String>,
    target_children: Option<Vec<String>>,
}

fn parse_refno(input: &str) -> Result<RefU64> {
    let normalized = input.trim().trim_start_matches('=').replace('_', "/");
    let Some((db, ele)) = normalized.split_once('/') else {
        bail!("invalid refno '{input}', expected db/element or db_element");
    };
    Ok(RefU64::from_two_nums(db.parse()?, ele.parse()?))
}

fn main() -> Result<()> {
    let args = Args::parse();
    let bytes = fs::read(&args.db_file)
        .with_context(|| format!("read db file {}", args.db_file.display()))?;

    // 主链路（索引优先）：与 aios-database 按需解析调用的入口一致。
    let index_start = Instant::now();
    let basic = parse_db_basic_data(bytes.clone(), "verify", "verify")?;
    let index_basic_ms = index_start.elapsed().as_millis();

    // 旧扫描路径重建 children，作为对照组。
    let scan_start = Instant::now();
    let (scan_table, _) = gen_ref_type_pos_table_scan(&bytes);
    let scan_children: DashMap<RefU64, Vec<RefU64>> = DashMap::with_capacity(scan_table.len());
    scan_table.par_iter().for_each(|entry| {
        let refno = *entry.key();
        let pos = entry.value().pos;
        let membs = parse_ele_membs(&bytes[pos - 4..]);
        let children: Vec<RefU64> = membs
            .iter()
            .filter(|x| scan_table.contains_key(*x))
            .cloned()
            .collect();
        scan_children.insert(refno, children);
    });
    let scan_children_ms = scan_start.elapsed().as_millis();

    let mut common_refnos = 0usize;
    let mut children_equal = 0usize;
    let mut children_diff = 0usize;
    let mut children_diff_samples = Vec::new();
    for (refno, index_children) in &basic.children_map {
        let Some(scan_entry) = scan_children.get(refno) else {
            continue;
        };
        common_refnos += 1;
        // 幽灵元素（已删除元素历史记录）只存在于 scan 表,会让 scan children 多出成员;
        // 比较时把双方 children 都过滤到两表共同成员,聚焦结构一致性。
        let scan_filtered: Vec<RefU64> = scan_entry
            .iter()
            .filter(|x| basic.refno_table_map.contains_key(*x))
            .cloned()
            .collect();
        let index_filtered: Vec<RefU64> = index_children
            .iter()
            .filter(|x| scan_table.contains_key(*x))
            .cloned()
            .collect();
        if scan_filtered == index_filtered {
            children_equal += 1;
        } else {
            children_diff += 1;
            if children_diff_samples.len() < 10 {
                children_diff_samples.push(format!(
                    "{}: index_children={} scan_children={}",
                    refno.to_e3d_id(),
                    index_filtered.len(),
                    scan_filtered.len()
                ));
            }
        }
    }

    let target = args.refno.as_deref().map(parse_refno).transpose()?;
    let target_children = target.and_then(|refno| {
        basic.children_map.get(&refno).map(|children| {
            children
                .iter()
                .take(20)
                .map(|c| c.to_e3d_id())
                .collect::<Vec<_>>()
        })
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&VerifyReport {
            db_file: args.db_file.display().to_string(),
            index_refnos: basic.refno_table_map.len(),
            scan_refnos: scan_table.len(),
            index_basic_ms,
            scan_children_ms,
            common_refnos,
            children_equal,
            children_diff,
            children_diff_samples,
            target_refno: target.map(|r| r.to_e3d_id()),
            target_children,
        })?
    );

    Ok(())
}
