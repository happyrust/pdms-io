use crate::io::PdmsIO;
use aios_core::RefU64;
use anyhow::Result;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct LatestRefLoc {
    pub sesno: u32,
    pub pgno: u32,
    pub offset: u64,
}

#[derive(Debug, Clone, Default)]
pub struct LatestIndexScanStats {
    pub total_nodes: usize,
    pub internal_nodes: usize,
    pub leaf_nodes: usize,
    pub total_leaf_entries: usize,
    pub valid_leaf_entries: usize,
    pub latest_refnos: usize,
    pub skipped_marker_entries: usize,
    pub skipped_zero_refno_entries: usize,
    pub skipped_invalid_flag_entries: usize,
    pub skipped_invalid_offset_entries: usize,
    pub skipped_no_session_entries: usize,
    pub read_page_errors: usize,
}

#[derive(Debug, Clone)]
pub struct LatestIndexScanResult {
    pub root_pgno: u32,
    pub latest_locs: HashMap<RefU64, LatestRefLoc>,
    pub stats: LatestIndexScanStats,
}

pub fn scan_latest_refno_offsets(io: &mut PdmsIO) -> Result<LatestIndexScanResult> {
    let basic = io.get_page_basic_info()?;
    let root_pgno = basic.latest_ses_data.index_root_pageno;

    let mut queue = VecDeque::new();
    queue.push_back(root_pgno);

    let mut visited = HashSet::new();
    let mut latest_locs: HashMap<RefU64, LatestRefLoc> = HashMap::new();
    let mut stats = LatestIndexScanStats::default();

    while let Some(page_no) = queue.pop_front() {
        if !visited.insert(page_no) {
            continue;
        }

        let index_data = match io.read_index_data(page_no) {
            Ok(v) => v,
            Err(_) => {
                stats.read_page_errors += 1;
                continue;
            }
        };

        stats.total_nodes += 1;

        if index_data.level == 0 {
            stats.leaf_nodes += 1;

            for loc in index_data.refno_locs {
                stats.total_leaf_entries += 1;

                if loc.refno_0 == 0 && loc.refno_1 == 0 {
                    stats.skipped_zero_refno_entries += 1;
                    continue;
                }
                if loc.refno_0 == 0x80000001 && loc.refno_1 == 0x80000001 {
                    stats.skipped_marker_entries += 1;
                    continue;
                }
                if loc.flag != 1 {
                    stats.skipped_invalid_flag_entries += 1;
                    continue;
                }
                if loc.pgno == 0 || loc.offset == 0 {
                    stats.skipped_invalid_offset_entries += 1;
                    continue;
                }

                let Some(sesno) = io.get_sesno(loc.pgno) else {
                    stats.skipped_no_session_entries += 1;
                    continue;
                };

                let offset = loc.get_att_offset_with_page_size(io.page_size);
                let refno = RefU64::from_two_nums(loc.refno_0, loc.refno_1);

                let candidate = LatestRefLoc {
                    sesno,
                    pgno: loc.pgno,
                    offset,
                };
                stats.valid_leaf_entries += 1;

                match latest_locs.get_mut(&refno) {
                    Some(existing) => {
                        if (candidate.sesno, candidate.offset) > (existing.sesno, existing.offset) {
                            *existing = candidate;
                        }
                    }
                    None => {
                        latest_locs.insert(refno, candidate);
                    }
                }
            }
        } else {
            stats.internal_nodes += 1;
            for loc in index_data.refno_locs {
                if loc.pgno > 0 {
                    queue.push_back(loc.pgno);
                }
            }
        }
    }

    stats.latest_refnos = latest_locs.len();
    Ok(LatestIndexScanResult {
        root_pgno,
        latest_locs,
        stats,
    })
}
