//! spec 007 T007 诊断：判定 compare_refno_index 的 scan-only refno 属于哪个 session。
//!
//! 沿 session 链（ses page word[1] -> 上一 session）逐版本枚举各自的 B-tree 索引，
//! 报告目标 refno 在哪些 session 可达。若 refno 只出现在历史 session 而不在最新
//! session，则说明旧 byte-scan 捕获的是已删除/历史记录，索引枚举无漏读。
//!
//! 用法：
//!   cargo run --release --example probe_scan_only_sessions -- \
//!     --db-file <path> --refnos 2013286704/1323,2013286704/2065

use std::collections::{HashSet, VecDeque};

const INDEX_PAGE_NOUN: i32 = 0x00CC_47DF;
const INDEX_PAGE_HEADER_SIZE: usize = 0x1C;
const INDEX_ENTRY_SIZE: usize = 16;
const START_MARKER: u32 = 0x8000_0001;

fn read_u32(input: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(input.get(offset..offset + 4)?.try_into().ok()?))
}

fn read_i32(input: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_be_bytes(input.get(offset..offset + 4)?.try_into().ok()?))
}

fn detect_page_size(input: &[u8], latest_ses: u32) -> usize {
    for page_size in [2048usize, 4096, 512, 1024] {
        let offset = latest_ses as usize * page_size;
        if offset + 4 <= input.len() && read_i32(input, offset) == Some(3) {
            return page_size;
        }
    }
    2048
}

/// 返回 (session_pgno, index_root_pgno) 链，从最新到最旧。
fn session_chain(input: &[u8], page_size: usize, latest_ses: u32) -> Vec<(u32, u32)> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut cur = latest_ses;
    while cur != 0 && seen.insert(cur) {
        let offset = cur as usize * page_size;
        if read_i32(input, offset) != Some(3) {
            break;
        }
        let prev = read_u32(input, offset + 4).unwrap_or(0);
        let root = read_u32(input, offset + 0x1C).unwrap_or(0);
        chain.push((cur, root));
        if prev == cur {
            break;
        }
        cur = prev;
    }
    chain
}

/// BFS 枚举一个 index root 下全部 leaf refno（含 start marker 子树，剔除 marker 本身）。
fn enumerate_root(input: &[u8], page_size: usize, root: u32) -> HashSet<(u32, u32)> {
    let mut refnos = HashSet::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([root]);
    while let Some(pgno) = queue.pop_front() {
        if pgno == 0 || !visited.insert(pgno) {
            continue;
        }
        let offset = pgno as usize * page_size;
        if offset + INDEX_PAGE_HEADER_SIZE > input.len()
            || read_i32(input, offset + 4) != Some(INDEX_PAGE_NOUN)
        {
            continue;
        }
        let level = read_u32(input, offset + 8).unwrap_or(0);
        let capacity = (page_size - INDEX_PAGE_HEADER_SIZE) / INDEX_ENTRY_SIZE;
        for i in 0..capacity {
            let entry = offset + INDEX_PAGE_HEADER_SIZE + i * INDEX_ENTRY_SIZE;
            if entry + INDEX_ENTRY_SIZE > input.len() {
                break;
            }
            let ref0 = read_u32(input, entry).unwrap_or(0);
            if ref0 == 0 {
                break;
            }
            let ref1 = read_u32(input, entry + 4).unwrap_or(0);
            let child = read_u32(input, entry + 8).unwrap_or(0);
            if level > 0 {
                queue.push_back(child);
            } else if !(ref0 == START_MARKER && ref1 == START_MARKER) {
                refnos.insert((ref0, ref1));
            }
        }
    }
    refnos
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let db_file = args
        .iter()
        .position(|a| a == "--db-file")
        .and_then(|i| args.get(i + 1))
        .expect("--db-file <path> required");
    let refnos_arg = args
        .iter()
        .position(|a| a == "--refnos")
        .and_then(|i| args.get(i + 1))
        .expect("--refnos a/b,c/d required");
    let targets: Vec<(u32, u32)> = refnos_arg
        .split(',')
        .filter_map(|pair| {
            let (a, b) = pair.trim().split_once('/')?;
            Some((a.parse().ok()?, b.parse().ok()?))
        })
        .collect();

    let bytes = std::fs::read(db_file).expect("read db file");
    let latest_ses = read_u32(&bytes, 0x28)
        .filter(|v| *v != 0)
        .or_else(|| read_u32(&bytes, 0x40))
        .expect("latest session pgno");
    let page_size = detect_page_size(&bytes, latest_ses);
    let chain = session_chain(&bytes, page_size, latest_ses);
    println!(
        "file={db_file} page_size={page_size} latest_ses={latest_ses} sessions={}",
        chain.len()
    );

    // session -> refno 集合（链可能很长，只展开包含目标判定所需的全部 session）
    let mut per_session: Vec<(u32, u32, HashSet<(u32, u32)>)> = Vec::new();
    for (ses, root) in &chain {
        let set = if *root == 0 {
            HashSet::new()
        } else {
            enumerate_root(&bytes, page_size, *root)
        };
        per_session.push((*ses, *root, set));
    }

    for target in &targets {
        let mut present_in = Vec::new();
        for (idx, (ses, _root, set)) in per_session.iter().enumerate() {
            if set.contains(target) {
                present_in.push(format!("ses#{idx}(pg{ses})"));
            }
        }
        let latest_has = per_session
            .first()
            .map(|(_, _, set)| set.contains(target))
            .unwrap_or(false);
        println!(
            "refno {}/{} latest={} history_hits=[{}]",
            target.0,
            target.1,
            if latest_has { "PRESENT" } else { "ABSENT" },
            present_in.join(",")
        );
    }

    // 汇总：最新 session 与最旧 session 的计数对比
    if let (Some((ses_new, _, set_new)), Some((ses_old, _, set_old))) =
        (per_session.first(), per_session.last())
    {
        println!(
            "summary latest ses pg{} refnos={} oldest ses pg{} refnos={}",
            ses_new,
            set_new.len(),
            ses_old,
            set_old.len()
        );
    }

    // 对目标 refno dump 最新 session 索引 loc 与记录头字节，定位 entry_from_loc 拒绝原因。
    if let Some((_, root)) = chain.first() {
        dump_target_locs(&bytes, page_size, *root, &targets);
    }
}

fn dump_target_locs(input: &[u8], page_size: usize, root: u32, targets: &[(u32, u32)]) {
    let target_set: HashSet<(u32, u32)> = targets.iter().copied().collect();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([root]);
    while let Some(pgno) = queue.pop_front() {
        if pgno == 0 || !visited.insert(pgno) {
            continue;
        }
        let offset = pgno as usize * page_size;
        if offset + INDEX_PAGE_HEADER_SIZE > input.len()
            || read_i32(input, offset + 4) != Some(INDEX_PAGE_NOUN)
        {
            continue;
        }
        let level = read_u32(input, offset + 8).unwrap_or(0);
        let capacity = (page_size - INDEX_PAGE_HEADER_SIZE) / INDEX_ENTRY_SIZE;
        for i in 0..capacity {
            let entry = offset + INDEX_PAGE_HEADER_SIZE + i * INDEX_ENTRY_SIZE;
            if entry + INDEX_ENTRY_SIZE > input.len() {
                break;
            }
            let ref0 = read_u32(input, entry).unwrap_or(0);
            if ref0 == 0 {
                break;
            }
            let ref1 = read_u32(input, entry + 4).unwrap_or(0);
            let child = read_u32(input, entry + 8).unwrap_or(0);
            let packed = read_u32(input, entry + 12).unwrap_or(0);
            if level > 0 {
                queue.push_back(child);
                continue;
            }
            if !target_set.contains(&(ref0, ref1)) {
                continue;
            }
            let offset_words = packed >> 12;
            let byte_offset = child as usize * page_size + offset_words as usize * 2;
            let dump_end = (byte_offset + 40).min(input.len());
            let hex: Vec<String> = input
                .get(byte_offset..dump_end)
                .unwrap_or(&[])
                .chunks(4)
                .map(|c| c.iter().map(|b| format!("{b:02x}")).collect::<String>())
                .collect();
            println!(
                "loc {}/{} leaf=pg{pgno} elem_pg={child} words={offset_words} packed_low12={:#x} byte_off={byte_offset} bytes=[{}]",
                ref0,
                ref1,
                packed & 0xFFF,
                hex.join(" ")
            );
        }
    }
}
