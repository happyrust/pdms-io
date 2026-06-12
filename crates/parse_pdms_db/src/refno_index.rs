use std::collections::{BTreeMap, HashSet, VecDeque};

use aios_core::RefU64;
use aios_core::db::EleDataEntry;
use dashmap::DashMap;

const DEFAULT_PAGE_SIZE: usize = 2048;
const INDEX_PAGE_NOUN: i32 = 0x00CC_47DF;
const INDEX_PAGE_HEADER_SIZE: usize = 0x1C;
const INDEX_ENTRY_SIZE: usize = 16;
const START_MARKER_REF0: u32 = 0x8000_0001;
const START_MARKER_REF1: u32 = 0x8000_0001;
const WORLD_NOUN: i32 = 0x000B_EB83;

#[derive(Debug, Clone, Copy)]
struct IndexLoc {
    refno: RefU64,
    pgno: u32,
    offset_words: u32,
}

#[derive(Debug)]
struct IndexPage {
    level: u32,
    locs: Vec<IndexLoc>,
}

/// Locate a single refno through the latest session B-tree index.
pub fn find_refno_entry(input: &[u8], target: RefU64) -> Option<EleDataEntry> {
    let page_size = detect_page_size(input)?;
    let root_pgno = latest_index_root_pgno(input, page_size)?;

    // 快路径：B-tree 下钻（O(log n)）。
    if let Some(loc) = find_loc_recursive(input, page_size, root_pgno, target, &mut HashSet::new())
        && let Some((_, entry)) = entry_from_loc(input, page_size, loc)
    {
        return Some(entry);
    }

    // 兜底：删除空洞会让中间层 key 乱序，单路径下钻可能走偏
    // （旧 PdmsIO 同样以 collect_leaf_pages 扩展搜索兜底）；
    // 遍历叶子页精确查找，保证与全表构建语义一致。
    find_loc_by_leaf_walk(input, page_size, root_pgno, target)
}

fn find_loc_by_leaf_walk(
    input: &[u8],
    page_size: usize,
    root_pgno: u32,
    target: RefU64,
) -> Option<EleDataEntry> {
    let mut queue = VecDeque::from([root_pgno]);
    let mut visited = HashSet::new();
    while let Some(pgno) = queue.pop_front() {
        if !visited.insert(pgno) {
            continue;
        }
        let Some(page) = parse_index_page(input, page_size, pgno) else {
            continue;
        };
        if page.level == 0 {
            for loc in page.locs {
                if !loc.is_start_marker()
                    && loc.refno == target
                    && let Some((_, entry)) = entry_from_loc(input, page_size, loc)
                {
                    return Some(entry);
                }
            }
        } else {
            for loc in page.locs {
                if !loc.is_start_marker() && loc.pgno > 0 {
                    queue.push_back(loc.pgno);
                }
            }
        }
    }
    None
}

/// Build the current refno table from the latest session B-tree index.
///
/// This avoids the old full-file byte scan in `gen_ref_type_pos_table`. If the index
/// cannot be decoded or does not validate against element records, callers should
/// fall back to the byte scan.
pub fn gen_ref_type_pos_table_from_index(
    input: &[u8],
) -> Option<(DashMap<RefU64, EleDataEntry>, RefU64)> {
    let page_size = detect_page_size(input)?;
    let root_pgno = latest_index_root_pgno(input, page_size)?;

    let mut ordered = BTreeMap::<RefU64, EleDataEntry>::new();
    let mut queue = VecDeque::from([root_pgno]);
    let mut visited = HashSet::new();

    while let Some(pgno) = queue.pop_front() {
        if !visited.insert(pgno) {
            continue;
        }
        // 个别页（如缓存页/损坏页）解析失败时跳过该页即可，与旧 build_index_map 行为一致。
        let Some(page) = parse_index_page(input, page_size, pgno) else {
            continue;
        };
        if page.level == 0 {
            for loc in page.locs {
                if loc.is_start_marker() || loc.pgno == 0 || loc.offset_words == 0 {
                    continue;
                }
                let Some((refno, entry)) = entry_from_loc(input, page_size, loc) else {
                    continue;
                };
                let should_insert = ordered
                    .get(&refno)
                    .map(|old| old.pos < entry.pos)
                    .unwrap_or(true);
                if should_insert {
                    ordered.insert(refno, entry);
                }
            }
        } else {
            for loc in page.locs {
                // 跳过起始标记指向的缓存页子树，只遍历当前会话有效索引。
                if !loc.is_start_marker() && loc.pgno > 0 {
                    queue.push_back(loc.pgno);
                }
            }
        }
    }

    // 与旧 byte-scan 语义一致：CATA 等库可能没有 WORL 元素，此时返回默认 refno。
    let world_refno = ordered
        .iter()
        .find_map(|(refno, entry)| (entry.noun_hash == WORLD_NOUN).then_some(*refno))
        .unwrap_or_default();
    let table = DashMap::with_capacity(ordered.len());
    for (refno, entry) in ordered {
        table.insert(refno, entry);
    }
    if table.is_empty() {
        None
    } else {
        Some((table, world_refno))
    }
}

impl IndexLoc {
    fn is_start_marker(self) -> bool {
        self.refno.get_0() == START_MARKER_REF0 && self.refno.get_1() == START_MARKER_REF1
    }

    fn byte_offset(self, page_size: usize) -> Option<usize> {
        let page = (self.pgno as usize).checked_mul(page_size)?;
        let offset = (self.offset_words as usize).checked_mul(2)?;
        page.checked_add(offset)
    }
}

fn find_loc_recursive(
    input: &[u8],
    page_size: usize,
    pgno: u32,
    target: RefU64,
    visited: &mut HashSet<u32>,
) -> Option<IndexLoc> {
    if !visited.insert(pgno) {
        return None;
    }
    let page = parse_index_page(input, page_size, pgno)?;
    if page.level == 0 {
        return page
            .locs
            .into_iter()
            .find(|loc| !loc.is_start_marker() && loc.refno == target);
    }

    // 删除空洞会让中间层 key 出现乱序，单一 lower_bound 可能下钻到错误子树；
    // 这里按优先级尝试多个候选子页（与旧实现对 x1<x0 乱序对的命中语义对齐）。
    for next_pgno in choose_child_pages(&page.locs, target) {
        if let Some(found) = find_loc_recursive(input, page_size, next_pgno, target, visited) {
            return Some(found);
        }
    }
    None
}

fn choose_child_pages(locs: &[IndexLoc], target: RefU64) -> Vec<u32> {
    let mut start_marker = None;
    let mut valid = Vec::new();
    for loc in locs {
        if loc.is_start_marker() {
            start_marker = Some(*loc);
            continue;
        }
        valid.push(*loc);
    }

    if valid.is_empty() {
        // 起始标记指向缓存页子树，仅在没有任何有效 entry 时兜底尝试。
        return start_marker.map(|loc| loc.pgno).into_iter().collect();
    }

    let mut candidates = Vec::new();
    let push = |pgno: u32, candidates: &mut Vec<u32>| {
        if pgno > 0 && !candidates.contains(&pgno) {
            candidates.push(pgno);
        }
    };

    // 1) 经典 lower_bound：最后一个 key <= target 的子页。
    let mut lower_bound = None;
    for loc in &valid {
        if loc.refno <= target {
            lower_bound = Some(loc.pgno);
        } else {
            break;
        }
    }
    if let Some(pgno) = lower_bound {
        push(pgno, &mut candidates);
    }

    // 2) 乱序对（x1 < x0，删除空洞）两侧的子页都可能覆盖 target。
    for pair in valid.windows(2) {
        if pair[1].refno < pair[0].refno {
            push(pair[0].pgno, &mut candidates);
            push(pair[1].pgno, &mut candidates);
            // 乱序之后的 key 不再可信，全部纳入候选。
            continue;
        }
    }
    let mut disordered = false;
    for pair in valid.windows(2) {
        if pair[1].refno < pair[0].refno {
            disordered = true;
        }
        if disordered && pair[1].refno <= target {
            push(pair[1].pgno, &mut candidates);
        }
    }

    // 3) target 小于首个有效 key 时，落入最左有效子树。
    if target < valid[0].refno {
        push(valid[0].pgno, &mut candidates);
    }

    candidates
}

fn entry_from_loc(input: &[u8], page_size: usize, loc: IndexLoc) -> Option<(RefU64, EleDataEntry)> {
    let raw_start = loc.byte_offset(page_size)?;
    let data_start = skip_record_padding(input, raw_start)?;
    if data_start.checked_add(16)? > input.len() {
        return None;
    }
    let refno = RefU64::from(&input[data_start + 4..data_start + 12]);
    if refno != loc.refno {
        return None;
    }
    let noun_hash = i32::from_be_bytes(input[data_start + 12..data_start + 16].try_into().ok()?);
    Some((
        refno,
        EleDataEntry {
            pos: data_start + 4,
            noun_hash,
        },
    ))
}

fn skip_record_padding(input: &[u8], mut pos: usize) -> Option<usize> {
    while pos.checked_add(4)? <= input.len() {
        let word = &input[pos..pos + 4];
        if word == [0, 0, 0, 0] || word == [0, 0, 0, 7] {
            pos += 4;
        } else {
            return Some(pos);
        }
    }
    None
}

fn detect_page_size(input: &[u8]) -> Option<usize> {
    let latest_ses_pgno = latest_ses_pgno(input)?;
    for page_size in [DEFAULT_PAGE_SIZE, 4096, 512] {
        let offset = (latest_ses_pgno as usize).checked_mul(page_size)?;
        if offset.checked_add(4)? <= input.len() && read_i32(input, offset)? == 3 {
            return Some(page_size);
        }
    }
    Some(DEFAULT_PAGE_SIZE)
}

fn latest_ses_pgno(input: &[u8]) -> Option<u32> {
    read_u32(input, 0x28).or_else(|| read_u32(input, 0x40))
}

fn latest_index_root_pgno(input: &[u8], page_size: usize) -> Option<u32> {
    let ses_pgno = latest_ses_pgno(input)?;
    let ses_offset = (ses_pgno as usize).checked_mul(page_size)?;
    if read_i32(input, ses_offset)? != 3 {
        return None;
    }
    let root = read_u32(input, ses_offset + 0x1C)?;
    (root != 0).then_some(root)
}

fn parse_index_page(input: &[u8], page_size: usize, pgno: u32) -> Option<IndexPage> {
    let offset = (pgno as usize).checked_mul(page_size)?;
    if offset.checked_add(INDEX_PAGE_HEADER_SIZE)? > input.len() {
        return None;
    }
    if read_i32(input, offset + 4)? != INDEX_PAGE_NOUN {
        return None;
    }

    let level = read_u32(input, offset + 8)?;
    let capacity = (page_size.saturating_sub(INDEX_PAGE_HEADER_SIZE)) / INDEX_ENTRY_SIZE;
    let declared_entries = read_u32(input, offset + 0x10)? as usize;
    let max_entries = if declared_entries == 0 {
        capacity
    } else {
        declared_entries.min(capacity)
    };

    let mut locs = Vec::new();
    for i in 0..max_entries {
        let entry_offset = offset + INDEX_PAGE_HEADER_SIZE + i * INDEX_ENTRY_SIZE;
        if entry_offset.checked_add(INDEX_ENTRY_SIZE)? > input.len() {
            break;
        }
        let ref0 = read_u32(input, entry_offset)?;
        if ref0 == 0 {
            break;
        }
        let ref1 = read_u32(input, entry_offset + 4)?;
        let pgno = read_u32(input, entry_offset + 8)?;
        let packed = read_u32(input, entry_offset + 12)?;
        locs.push(IndexLoc {
            refno: RefU64::from_two_nums(ref0, ref1),
            pgno,
            offset_words: packed >> 12,
        });
    }

    (!locs.is_empty()).then_some(IndexPage { level, locs })
}

fn read_u32(input: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        input.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_i32(input: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_be_bytes(
        input.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION_PGNO: u32 = 1;
    const INDEX_ROOT_PGNO: u32 = 2;
    const ELEMENT_PGNO: u32 = 3;

    fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn put_i32(bytes: &mut [u8], offset: usize, value: i32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn write_index_entry(
        bytes: &mut [u8],
        offset: usize,
        refno: RefU64,
        pgno: u32,
        offset_words: u32,
    ) {
        put_u32(bytes, offset, refno.get_0());
        put_u32(bytes, offset + 4, refno.get_1());
        put_u32(bytes, offset + 8, pgno);
        put_u32(bytes, offset + 12, offset_words << 12);
    }

    fn write_element_record(bytes: &mut [u8], offset: usize, refno: RefU64, noun_hash: i32) {
        put_u32(bytes, offset, 8);
        put_u32(bytes, offset + 4, refno.get_0());
        put_u32(bytes, offset + 8, refno.get_1());
        put_i32(bytes, offset + 12, noun_hash);
    }

    #[test]
    fn parse_index_page_honors_declared_entry_count() {
        let page_size = DEFAULT_PAGE_SIZE;
        let mut bytes = vec![0u8; page_size * 4];
        let active_refno = RefU64::from_two_nums(13246, 243899);
        let stale_refno = RefU64::from_two_nums(13246, 243900);
        let active_offset_words = 8u32;
        let stale_offset_words = 20u32;

        put_u32(&mut bytes, 0x28, SESSION_PGNO);

        let session_offset = SESSION_PGNO as usize * page_size;
        put_i32(&mut bytes, session_offset, 3);
        put_u32(&mut bytes, session_offset + 0x1C, INDEX_ROOT_PGNO);

        let index_offset = INDEX_ROOT_PGNO as usize * page_size;
        put_i32(&mut bytes, index_offset, 1);
        put_i32(&mut bytes, index_offset + 4, INDEX_PAGE_NOUN);
        put_u32(&mut bytes, index_offset + 8, 0);
        put_u32(&mut bytes, index_offset + 0x10, 1); // dword[4]: declared entry count

        let entry_offset = index_offset + INDEX_PAGE_HEADER_SIZE;
        write_index_entry(
            &mut bytes,
            entry_offset,
            active_refno,
            ELEMENT_PGNO,
            active_offset_words,
        );
        // Non-zero leftover entry in the page free area must be ignored.
        write_index_entry(
            &mut bytes,
            entry_offset + INDEX_ENTRY_SIZE,
            stale_refno,
            ELEMENT_PGNO,
            stale_offset_words,
        );

        let active_record_offset =
            ELEMENT_PGNO as usize * page_size + active_offset_words as usize * 2;
        let stale_record_offset =
            ELEMENT_PGNO as usize * page_size + stale_offset_words as usize * 2;
        write_element_record(&mut bytes, active_record_offset, active_refno, 0x123456);
        write_element_record(&mut bytes, stale_record_offset, stale_refno, 0x654321);

        let (table, _) = gen_ref_type_pos_table_from_index(&bytes).expect("index should parse");
        assert!(table.contains_key(&active_refno));
        assert!(
            !table.contains_key(&stale_refno),
            "entries beyond the declared count are page free-area leftovers"
        );
        assert!(find_refno_entry(&bytes, stale_refno).is_none());
    }
}
