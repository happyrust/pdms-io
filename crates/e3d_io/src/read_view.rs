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
use crate::{E3dError, HDR_LATEST, INDEX_NOUN, SES_LAST, SES_ROOT, looks_like_noun};

/// B 树叶项：`(refno0, refno1, data_pgno, word_off)`（与 lib.rs `walk` 输出同构）。
pub type LeafEntry = (u32, u32, usize, u32);

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
