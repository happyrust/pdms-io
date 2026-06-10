//! `Rdb` — 经 [`PageSource`] 取页的**只读**格式视图（spec 002 T103 读侧）。
//!
//! 设计（见 specs/002 tasks.md T103 实现注记）：
//! - **读侧**导航（会话链 / B 树枚举 / 主记录定位）在此经页源按需取页，
//!   使 Phase 2 `PdmsIO` 能在 [`PagedFile`] 上做只读委托而无需整文件加载。
//! - **写侧**（COW 整页克隆/追加/page0 补丁）按设计保留 `Edb` 的 flat-buffer
//!   模型（001 验证语义），不经本视图。
//! - 全属性解码 `decode_full`（含 DA 链跨页追逐）留在 `Edb` 路径；Phase 2
//!   委托清单（contracts C1）只需要导航 + 原始记录字节，二者本视图均覆盖。
//!
//! 实现：惰性影子缓冲——按需取页填入 `shadow`，已加载页与源字节逐字节一致，
//! 导航算法与 lib.rs 同式（同一套偏移算术），保证行为零漂移；`pages_read()`
//! 暴露已取页数，供 SC-006（增量读 < 全文件页数）观测。

use std::collections::HashSet;

use crate::page_source::{InMemory, PagedFile, PageSource};
use crate::{E3dError, HDR_LATEST, INDEX_NOUN, SES_LAST, SES_ROOT, SES_SESNO, SES_END, looks_like_noun};

/// B 树叶项：`(refno0, refno1, data_pgno, word_off)`（与 lib.rs `walk` 输出同构）。
pub type LeafEntry = (u32, u32, usize, u32);

/// 会话页元数据（字段偏移与 `pdms_io` 的 `SessionPageData` 同源：
/// last@0x04 / sesno@0x0C / end@0x14 / root@0x1C；spec 002 T203）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SesInfo {
    pub pgno: u32,
    pub sesno: u32,
    pub last_ses_pgno: u32,
    pub end_pgno: u32,
    pub root_pgno: u32,
}

/// 只读格式视图：泛型于页源。
pub struct Rdb<S: PageSource> {
    src: S,
    ps: usize,
    n_pages: usize,
    shadow: Vec<u8>,
    loaded: Vec<bool>,
}

impl Rdb<InMemory> {
    /// 以整文件字节构造（`InMemory` 页源）。
    pub fn from_bytes(buf: Vec<u8>) -> Rdb<InMemory> {
        let src = InMemory::from_bytes(buf);
        let n = src.n_pages();
        Rdb::new(src, n)
    }

    pub fn open_in_memory(path: &str) -> Result<Rdb<InMemory>, E3dError> {
        Ok(Rdb::from_bytes(std::fs::read(path)?))
    }
}

impl Rdb<PagedFile> {
    /// 以 LRU 分页文件页源打开（页大小按 contracts C3.2 探测）。
    pub fn open(path: &str) -> Result<Rdb<PagedFile>, E3dError> {
        let src = PagedFile::open(path)?;
        let n = src.n_pages()?;
        Ok(Rdb::new(src, n))
    }
}

impl<S: PageSource> Rdb<S> {
    /// 自定义页源构造；`n_pages` 为完整页数（trait 保持最小，长度由调用方提供）。
    pub fn new(mut src: S, n_pages: usize) -> Rdb<S> {
        let ps = src.page_size();
        Rdb { src, ps, n_pages, shadow: vec![0u8; ps * n_pages], loaded: vec![false; n_pages] }
    }

    pub fn page_size(&self) -> usize {
        self.ps
    }

    pub fn n_pages(&self) -> usize {
        self.n_pages
    }

    /// 已从页源取过的页数（SC-006 观测点）。
    pub fn pages_read(&self) -> usize {
        self.loaded.iter().filter(|x| **x).count()
    }

    fn ensure(&mut self, pg: usize) -> Result<(), E3dError> {
        if pg >= self.n_pages {
            return Err(E3dError::Write(format!(
                "page {pg} out of range ({} full pages)",
                self.n_pages
            )));
        }
        if !self.loaded[pg] {
            let bytes = self.src.page(0, pg as u32)?;
            self.shadow[pg * self.ps..(pg + 1) * self.ps].copy_from_slice(bytes);
            self.loaded[pg] = true;
        }
        Ok(())
    }

    /// 绝对字节区间视图（按需取覆盖页）。零长度返回空切片。
    pub fn slice(&mut self, off: usize, len: usize) -> Result<&[u8], E3dError> {
        if len == 0 {
            return Ok(&[]);
        }
        let (p0, p1) = (off / self.ps, (off + len - 1) / self.ps);
        for pg in p0..=p1 {
            self.ensure(pg)?;
        }
        Ok(&self.shadow[off..off + len])
    }

    /// 绝对偏移处大端 u32（与 `Edb::u` 同义，按需取页）。
    pub fn u(&mut self, o: usize) -> Result<u32, E3dError> {
        let b = self.slice(o, 4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn is_index(&mut self, pg: usize) -> bool {
        pg > 0
            && pg * self.ps + 8 <= self.n_pages * self.ps
            && self.u(pg * self.ps + 4).map(|w| w == INDEX_NOUN).unwrap_or(false)
    }

    /// 最新会话的索引根（与 `Edb::latest_root` 同义）。
    pub fn latest_root(&mut self) -> Result<usize, E3dError> {
        let ses = self.u(HDR_LATEST)? as usize;
        Ok(self.u(ses * self.ps + SES_ROOT)? as usize)
    }

    /// 会话索引根，最新在前（与 lib.rs `session_roots` 同式：SES_LAST 回溯,环防,上限 256）。
    pub fn session_roots(&mut self) -> Result<Vec<usize>, E3dError> {
        let (mut out, mut seen) = (Vec::new(), HashSet::new());
        let mut pg = self.u(HDR_LATEST)? as usize;
        while pg != 0 && pg < self.n_pages && seen.insert(pg) && out.len() < 256 {
            out.push(self.u(pg * self.ps + SES_ROOT)? as usize);
            pg = self.u(pg * self.ps + SES_LAST)? as usize;
        }
        Ok(out)
    }

    /// 完整会话链（newest-first;spec 002 T203 单源,供 `PdmsIO::init_ses_maps` 委托）。
    /// 终止条件与 v1 等价：`pg==0` 停;负 `last_ses_pageno`（按 u32 读为巨值）由
    /// `pg < n_pages` 界止;环由 `seen` 防;无 256 上限（v1 同样无界,逐页唯一访问有自然上界）。
    pub fn session_chain(&mut self) -> Result<Vec<SesInfo>, E3dError> {
        let (mut out, mut seen) = (Vec::new(), HashSet::new());
        let mut pg = self.u(HDR_LATEST)? as usize;
        while pg != 0 && pg < self.n_pages && seen.insert(pg) {
            let base = pg * self.ps;
            let last = self.u(base + SES_LAST)?;
            out.push(SesInfo {
                pgno: pg as u32,
                sesno: self.u(base + SES_SESNO)?,
                last_ses_pgno: last,
                end_pgno: self.u(base + SES_END)?,
                root_pgno: self.u(base + SES_ROOT)?,
            });
            pg = last as usize;
        }
        Ok(out)
    }

    /// 枚举 `root` 下全部 B 树叶项（与 lib.rs `walk` 同式：word6 界定 + 哨兵左子下降）。
    pub fn leaves(&mut self, root: usize) -> Result<Vec<LeafEntry>, E3dError> {
        let (mut out, mut seen) = (Vec::new(), HashSet::new());
        self.walk_into(root, &mut out, &mut seen, 4_000_000)?;
        Ok(out)
    }

    fn walk_into(
        &mut self,
        pg: usize,
        out: &mut Vec<LeafEntry>,
        seen: &mut HashSet<usize>,
        max: usize,
    ) -> Result<(), E3dError> {
        if seen.contains(&pg) || out.len() >= max || !self.is_index(pg) {
            return Ok(());
        }
        seen.insert(pg);
        let base = pg * self.ps;
        let pw = self.ps / 4;
        let nent = (pw - 7 - self.u(base + 24)? as usize) / 4; // word6 = free words -> entry count
        for k in 0..nent {
            if out.len() >= max {
                break;
            }
            let eo = base + (7 + 4 * k) * 4;
            let (r0, r1) = (self.u(eo)?, self.u(eo + 4)?);
            let cpg = self.u(eo + 8)? as usize;
            let off = self.u(eo + 12)? >> 12;
            if off == 0 && self.is_index(cpg) {
                self.walk_into(cpg, out, seen, max)?;
            } else if (r0, r1) != (0x80000001, 0x80000001) {
                out.push((r0, r1, cpg, off));
            }
        }
        Ok(())
    }

    /// B+ 树**目标式点查**（spec 002 T204 单源；与 lib.rs `btree_descend` 同式：
    /// 每个内部节点取"最右一个 separator <= key"的孩子下降——含 `ci=0` 默认
    /// （首项 > key 时仍降首子）与哨兵 `0x80000001` = -inf 语义,深度上限 40）。
    /// 叶命中返回原始数据位置 `(data_pgno, word_off)`（不做主记录过滤,过滤语义
    /// 见 [`Self::record_off_via_root`]）;未命中 `None`。O(log n),为 `PdmsIO`
    /// `search_latest_refno` 族的换芯素材。
    pub fn btree_find(
        &mut self,
        root: usize,
        key: (u32, u32),
    ) -> Result<Option<(usize, u32)>, E3dError> {
        fn key_le(a: (u32, u32), b: (u32, u32)) -> bool {
            if a == (crate::SENTINEL, crate::SENTINEL) {
                return true;
            }
            if b == (crate::SENTINEL, crate::SENTINEL) {
                return false;
            }
            a <= b
        }
        let mut pg = root;
        for _ in 0..40 {
            let base = pg * self.ps;
            let pw = self.ps / 4;
            let level = self.u(base + 8)?;
            let nent = (pw - 7).saturating_sub(self.u(base + 24)? as usize) / 4;
            if level == 0 {
                for k in 0..nent {
                    let eo = base + (7 + 4 * k) * 4;
                    if (self.u(eo)?, self.u(eo + 4)?) == key {
                        let cpg = self.u(eo + 8)? as usize;
                        let off = self.u(eo + 12)? >> 12;
                        return Ok(Some((cpg, off)));
                    }
                }
                return Ok(None);
            }
            let mut ci = 0usize;
            for k in 0..nent {
                let eo = base + (7 + 4 * k) * 4;
                let e = (self.u(eo)?, self.u(eo + 4)?);
                if key_le(e, key) {
                    ci = k;
                } else {
                    break;
                }
            }
            if nent == 0 {
                return Ok(None);
            }
            pg = self.u(base + (7 + 4 * ci) * 4 + 8)? as usize;
        }
        Ok(None)
    }

    /// `refno` 在指定会话 `root` 下的主记录字节偏移（与 lib.rs `record_off_via_root`
    /// 同式过滤：word0 高 16 位为 0、impl 字数 8..=512、noun 可反哈希）。
    pub fn record_off_via_root(
        &mut self,
        root: usize,
        refno: (u32, u32),
    ) -> Result<Option<usize>, E3dError> {
        let leaves = self.leaves(root)?;
        let total = self.n_pages * self.ps;
        for (r0, r1, pg, off) in &leaves {
            if (*r0, *r1) != refno || *off == 0 {
                continue;
            }
            let bo = pg * self.ps + (*off as usize) * 2;
            if bo + 44 > total {
                continue;
            }
            let w0 = self.u(bo)?;
            if (w0 >> 16) != 0 || !(8..=512).contains(&(w0 & 0xFFFF)) {
                continue;
            }
            if looks_like_noun(self.u(bo + 12)?) {
                return Ok(Some(bo));
            }
        }
        Ok(None)
    }

    /// 主记录原始字节：头 11 词 + 隐式区（word0 低 16 位声明的 impl 字数,封顶 256 词,
    /// 与 `decode_full` 的读取窗口同口径）。Phase 2 `parse_raw_element` 委托的素材。
    pub fn record_bytes(&mut self, bo: usize) -> Result<Vec<u8>, E3dError> {
        let w0 = self.u(bo)?;
        let impl_words = ((w0 & 0xFFFF) as usize).clamp(11, 256);
        Ok(self.slice(bo, impl_words * 4)?.to_vec())
    }

    /// 变长元素记录读取（specs/002 T205;v1 `ElementRecordReader` 语义同式移植）：
    /// 自适应窗口 16K→64K,按记录结构定界截断——隐式区(word0 声明长度+padding 顺延)、
    /// `0x0001` 显式块 / `0x0002` 成员块及其 `0x07` 追加段、`00000000+00000007`
    /// 双词终止、孤立 `00000007` 终止启发、相邻记录起始启发。
    /// 注:v1 以真实文件长度为界(可触及尾部不完整页),本视图以完整页数为界——
    /// 真实 E3D 库 COW 整页追加,二者无差;损坏/截尾文件上的差异已档。
    pub fn element_record(&mut self, start_offset: u64) -> Result<Vec<u8>, E3dError> {
        const INITIAL: usize = 16 * 1024;
        const MAX: usize = 64 * 1024;

        let file_len = (self.n_pages * self.ps) as u64;
        let initial_available = file_len.saturating_sub(start_offset) as usize;
        if initial_available == 0 {
            return Err(E3dError::Write(format!(
                "element record start_offset beyond EOF (start_offset={start_offset:#X}, file_len={file_len:#X})"
            )));
        }

        let mut target = INITIAL.min(initial_available);
        loop {
            let data = self.slice(start_offset as usize, target)?;
            if let Some(end) = record_end(data)? {
                return Ok(data[..end].to_vec());
            }
            if target >= MAX {
                return Ok(data.to_vec());
            }
            let grown = (target * 2).min(MAX);
            let available = file_len.saturating_sub(start_offset) as usize;
            let next = grown.min(available);
            if next == target {
                let data = self.slice(start_offset as usize, target)?;
                return Ok(data.to_vec());
            }
            target = next;
        }
    }
}

// ---------------------------------------------------------------------------
// 元素记录定界（v1 ElementRecordReader::find_record_end 一族的同式移植,纯函数）
// ---------------------------------------------------------------------------

const PADDING_ZERO: [u8; 4] = [0x00, 0x00, 0x00, 0x00];
const PADDING_SEVEN: [u8; 4] = [0x00, 0x00, 0x00, 0x07];

fn record_end(data: &[u8]) -> Result<Option<usize>, E3dError> {
    let prefix = skip_padding_len(data);
    if prefix + 4 > data.len() {
        return Ok(None);
    }

    let impl_len_words = i32::from_be_bytes(data[prefix..prefix + 4].try_into().unwrap());
    if impl_len_words <= 0 {
        return Err(E3dError::Write(format!("impl_len 非法: {impl_len_words}")));
    }

    let declared_impl_len = impl_len_words as usize * 4;
    if prefix + declared_impl_len > data.len() {
        return Ok(None);
    }

    let actual_impl_len = extend_impl_len(declared_impl_len, &data[prefix..]);
    let mut pos = prefix + actual_impl_len;
    let mut saw_explicit_block = false;

    while pos + 4 <= data.len() {
        // 优先识别“明确”的结束标记：00 00 00 00 + 00 00 00 07
        if pos + 8 <= data.len()
            && data[pos..pos + 4] == PADDING_ZERO
            && data[pos + 4..pos + 8] == PADDING_SEVEN
        {
            return Ok(Some(pos + 8));
        }

        // 单独的 0x00000007：可能是 padding，也可能是 0x07 追加段的起始。
        // 只有在它“看起来不像追加段”时，才当作结束标记。
        if data[pos..pos + 4] == PADDING_SEVEN {
            let looks_like_segment = pos + 6 <= data.len()
                && data[pos + 4] == 0x00
                && (data[pos + 5] == 0x01 || data[pos + 5] == 0x02);
            if !looks_like_segment {
                return Ok(Some(pos + 4));
            }
            pos += 4;
            continue;
        }

        if data[pos..pos + 4] == PADDING_ZERO {
            pos += 4;
            continue;
        }

        if saw_explicit_block && looks_like_element_record_start(data, pos) {
            return Ok(Some(pos));
        }

        let flag = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let len_words = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;

        if flag == 0x0001 || flag == 0x0002 {
            if len_words == 0 {
                pos += 4;
                continue;
            }
            let block_len = len_words * 4;
            if pos + block_len > data.len() {
                return Ok(None);
            }
            pos += block_len;
            if flag == 0x0001 {
                saw_explicit_block = true;
            }

            pos = match advance_over_segments(data, pos, flag as u8) {
                Some(p) => p,
                None => return Ok(None),
            };
            continue;
        }

        // 有些 E3D 记录在 implicit/member 区和后续显式块之间夹着当前解析器
        // 尚不认识的 word。不能在第一个未知 word 就截断，否则会丢掉后面的
        // explicit block（例如 PHEI 可能在十几 KB 之后）。
        pos += 4;
    }

    Ok(None)
}

fn looks_like_element_record_start(data: &[u8], pos: usize) -> bool {
    if pos + 16 > data.len() {
        return false;
    }

    let impl_len_words = i32::from_be_bytes(data[pos..pos + 4].try_into().unwrap());
    if impl_len_words <= 0 {
        return false;
    }

    let Some(impl_len) = (impl_len_words as usize).checked_mul(4) else {
        return false;
    };
    let Some(end) = pos.checked_add(impl_len) else {
        return false;
    };
    if impl_len < 16 || end > data.len() {
        return false;
    }

    let refno = &data[pos + 4..pos + 12];
    if refno.iter().all(|&b| b == 0) {
        return false;
    }

    let noun_hash = i32::from_be_bytes(data[pos + 12..pos + 16].try_into().unwrap());
    noun_hash != 0
}

fn skip_padding_len(input: &[u8]) -> usize {
    let mut pos = 0;
    while pos + 4 <= input.len() {
        let next = &input[pos..pos + 4];
        if next == PADDING_ZERO || next == PADDING_SEVEN {
            pos += 4;
        } else {
            break;
        }
    }
    pos
}

fn extend_impl_len(declared: usize, input: &[u8]) -> usize {
    let mut actual = declared;
    while actual + 4 <= input.len() {
        let next = &input[actual..actual + 4];
        if next == PADDING_ZERO || next == PADDING_SEVEN {
            actual += 4;
        } else {
            break;
        }
    }
    actual
}

fn advance_over_segments(data: &[u8], mut pos: usize, flag: u8) -> Option<usize> {
    while pos + 8 <= data.len()
        && data[pos..pos + 4] == PADDING_SEVEN
        && data[pos + 4] == 0x00
        && data[pos + 5] == flag
    {
        let seg_len_words = u16::from_be_bytes([data[pos + 6], data[pos + 7]]) as usize;
        if seg_len_words == 0 {
            return None;
        }
        let seg_total = seg_len_words * 4 + 4;
        if pos + seg_total > data.len() {
            return None;
        }
        pos += seg_total;
    }
    Some(pos)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Edb;

    const SAM: &str = r"D:\work\plant\pdms-io\pdms-test-data\sam7200_0001";

    fn sam_present() -> bool {
        std::path::Path::new(SAM).exists()
    }

    /// Rdb<InMemory> 导航 == Edb 同名 API（同一字节、同一算术 ⇒ 必须逐项一致）。
    #[test]
    fn rdb_in_memory_matches_edb_navigation() {
        if !sam_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let bytes = std::fs::read(SAM).unwrap();
        let db = Edb::from_bytes(bytes.clone());
        let mut rv = Rdb::from_bytes(bytes);

        assert_eq!(rv.page_size(), db.page_size());
        assert_eq!(rv.latest_root().unwrap(), db.latest_root());
        assert_eq!(rv.session_roots().unwrap(), crate::session_roots(&db));

        let root = db.latest_root();
        let mut lib_leaves = Vec::new();
        crate::walk(&db, root, &mut lib_leaves, &mut std::collections::HashSet::new(), 4_000_000);
        let rv_leaves = rv.leaves(root).unwrap();
        assert_eq!(rv_leaves, lib_leaves, "leaf enumeration must be identical");
        assert!(rv_leaves.len() >= 10_000, "sam7200 has 10392 elements");
    }

    /// T105 元素级（SC-003）：同库经 InMemory 与 PagedFile 两页源,叶项枚举、
    /// 主记录定位与记录原始字节逐项一致。
    #[test]
    fn rdb_paged_file_equals_in_memory_element_level() {
        if !sam_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let mut mem = Rdb::open_in_memory(SAM).unwrap();
        let mut pf = Rdb::open(SAM).unwrap();
        assert_eq!(mem.page_size(), pf.page_size());
        assert_eq!(mem.n_pages(), pf.n_pages());

        let (ra, rb) = (mem.latest_root().unwrap(), pf.latest_root().unwrap());
        assert_eq!(ra, rb);
        let (la, lb) = (mem.leaves(ra).unwrap(), pf.leaves(rb).unwrap());
        assert_eq!(la, lb, "element enumeration must match across page sources");

        // 全部主记录逐项对比定位 + 原始记录字节（含 /WB1 在内的全库覆盖）。
        let mut checked = 0usize;
        for (r0, r1, _, off) in &la {
            if *off == 0 {
                continue;
            }
            let a = mem.record_off_via_root(ra, (*r0, *r1)).unwrap();
            let b = pf.record_off_via_root(rb, (*r0, *r1)).unwrap();
            assert_eq!(a, b, "record locate mismatch for ({r0:#x},{r1:#x})");
            if let Some(bo) = a {
                assert_eq!(
                    mem.record_bytes(bo).unwrap(),
                    pf.record_bytes(bo).unwrap(),
                    "raw record bytes mismatch at {bo:#x}"
                );
                checked += 1;
            }
            if checked >= 500 {
                break; // 500 条记录字节级抽查；枚举本身已全库等值
            }
        }
        assert!(checked >= 400, "expected to byte-check hundreds of records, got {checked}");
    }

    /// T203：会话链双源等值 + 与 session_roots 互证 + 链自洽（newest-first、
    /// sesno 严格递减、last 指针指向下一项）。
    #[test]
    fn rdb_session_chain_consistent() {
        if !sam_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let mut mem = Rdb::open_in_memory(SAM).unwrap();
        let mut pf = Rdb::open(SAM).unwrap();
        let (ca, cb) = (mem.session_chain().unwrap(), pf.session_chain().unwrap());
        assert_eq!(ca, cb, "session chain must match across page sources");
        assert!(!ca.is_empty());
        let roots: Vec<usize> = ca.iter().map(|s| s.root_pgno as usize).collect();
        assert_eq!(roots, mem.session_roots().unwrap(), "roots projection must agree");
        for w in ca.windows(2) {
            assert!(w[0].sesno > w[1].sesno, "newest-first sesno order");
            assert_eq!(w[0].last_ses_pgno, w[1].pgno, "chain pointer integrity");
        }
    }

    /// T204：`btree_find` 点查 == 全树枚举（10392 键穷举,双源等值;含缺席键 None）。
    #[test]
    fn rdb_btree_find_matches_walk_enumeration() {
        if !sam_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let mut mem = Rdb::open_in_memory(SAM).unwrap();
        let mut pf = Rdb::open(SAM).unwrap();
        let root = mem.latest_root().unwrap();

        let leaves = mem.leaves(root).unwrap();
        let mut bykey: std::collections::HashMap<(u32, u32), Vec<(usize, u32)>> =
            std::collections::HashMap::new();
        for (r0, r1, pg, off) in &leaves {
            bykey.entry((*r0, *r1)).or_default().push((*pg, *off));
        }

        let mut checked = 0usize;
        for (key, locs) in &bykey {
            let a = mem.btree_find(root, *key).unwrap();
            let b = pf.btree_find(root, *key).unwrap();
            assert_eq!(a, b, "btree_find must agree across page sources for {key:?}");
            let got = a.unwrap_or_else(|| panic!("walk key {key:?} must be findable by descent"));
            assert!(locs.contains(&got), "descent loc {got:?} not among walk locs for {key:?}");
            checked += 1;
        }
        assert!(checked >= 10_000, "expected to verify >=10k keys, got {checked}");

        // 缺席键：低于全树最小 / 不存在的随机键均必须 None。
        assert_eq!(mem.btree_find(root, (0, 0)).unwrap(), None);
        assert_eq!(mem.btree_find(root, (0xDEAD_0000, 0xBEEF)).unwrap(), None);
    }

    /// SC-006（导航口径）：点状导航（会话链 + 根定位）只触达少量页。
    #[test]
    fn rdb_point_navigation_reads_few_pages() {
        if !sam_present() {
            eprintln!("[skip] data absent");
            return;
        }
        let mut pf = Rdb::open(SAM).unwrap();
        pf.latest_root().unwrap();
        pf.session_roots().unwrap();
        let total = pf.n_pages();
        let read = pf.pages_read();
        assert!(read * 10 < total, "point navigation read {read}/{total} pages");
    }
}
