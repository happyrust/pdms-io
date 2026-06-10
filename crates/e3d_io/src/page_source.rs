//! `PageSource` — 格式核心与物理 I/O 的唯一边界 (spec 002, contracts C2).
//!
//! 002 收敛目标：`e3d_io` 的格式逻辑只经由本抽象取页，物理来源可替换：
//! - [`InMemory`]：整文件 buffer（等价 `Edb` 现状；CLI/测试默认，零成本）。
//! - [`PagedFile`]：文件句柄 + LRU 页缓存（承接 v1 `page_manager.rs` 的
//!   容量/驱逐/命中统计语义，读侧），供增量 watcher / 大库场景流式读取。
//!
//! 不变量（contracts C2）：
//! - I1: `page()` 返回长度恒 == `page_size()`；越界返回错误而非截断页。
//! - I2: 文件未变更期间，同一 `(ext_no, pgno)` 重复取页字节一致（缓存透明）。
//! - I3: 本模块不解释页内容（格式语义只在核心层）；std-only，零第三方依赖。
//! - I4: `ext_no` 仅透传，现阶段恒 0（多 extent 为 002 范围外）。

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::E3dError;

/// 头部 word：`page_size/4`（与 `Edb::from_bytes` 同源）。
const HDR_PAGE_WORDS: usize = 0x34;
/// 头部 word：latest session pgno（与 lib.rs `HDR_LATEST` 同值；探测用）。
const HDR_LATEST_SES: usize = 0x28;
/// 合法页大小集合。
const VALID_PAGE_SIZES: [usize; 3] = [512, 2048, 4096];
/// 页大小探测候选顺序（contracts C3.2：2K → 4K → 512，兜底 2K）。
const PROBE_ORDER: [usize; 3] = [2048, 4096, 512];
/// `page_type == Session` 的页类型值。
const PAGE_TYPE_SESSION: i32 = 3;

fn be_u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// 按 `(ext_no, pgno)` 提供页字节的最小抽象。
pub trait PageSource {
    /// 页大小（512/2048/4096），对单个打开实例恒定。
    fn page_size(&self) -> usize;
    /// 取第 `pgno` 页的完整字节（长度 == `page_size()`）。`ext_no` 现阶段恒 0。
    fn page(&mut self, ext_no: u32, pgno: u32) -> Result<&[u8], E3dError>;
}

/// 从 64B 头部字节推断页大小：`word(0x34)*4`，非法值兜底 2048（与 `Edb::from_bytes` 一致）。
pub fn page_size_from_header(header: &[u8]) -> usize {
    if header.len() < HDR_PAGE_WORDS + 4 {
        return 2048;
    }
    let ps = (be_u32_at(header, HDR_PAGE_WORDS) as usize) * 4;
    if VALID_PAGE_SIZES.contains(&ps) { ps } else { 2048 }
}

// ---------------------------------------------------------------------------
// InMemory
// ---------------------------------------------------------------------------

/// 整文件 buffer 页源（等价 `Edb` 现状存储模型）。
pub struct InMemory {
    buf: Vec<u8>,
    ps: usize,
}

impl InMemory {
    /// 从整文件字节构造；页大小按头部推断（与 `Edb::from_bytes` 同规则）。
    pub fn from_bytes(buf: Vec<u8>) -> InMemory {
        let ps = page_size_from_header(&buf);
        InMemory { buf, ps }
    }

    /// 显式指定页大小（测试/特殊样本用）。
    pub fn with_page_size(buf: Vec<u8>, ps: usize) -> InMemory {
        InMemory { buf, ps }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<InMemory, E3dError> {
        Ok(InMemory::from_bytes(std::fs::read(path)?))
    }

    /// 整体字节视图（与 `Edb::bytes()` 对应；迁移期适配用）。
    pub fn bytes(&self) -> &[u8] {
        &self.buf
    }

    /// 取回整文件字节（`Edb` 等 flat-buffer 消费者的下沉接线用，spec 002 T102）。
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

impl PageSource for InMemory {
    fn page_size(&self) -> usize {
        self.ps
    }

    fn page(&mut self, _ext_no: u32, pgno: u32) -> Result<&[u8], E3dError> {
        let start = pgno as usize * self.ps;
        let end = start + self.ps;
        if end > self.buf.len() {
            return Err(E3dError::Write(format!(
                "page {pgno} out of range (file has {} full pages of {} bytes)",
                self.buf.len() / self.ps,
                self.ps
            )));
        }
        Ok(&self.buf[start..end])
    }
}

// ---------------------------------------------------------------------------
// PagedFile (LRU)
// ---------------------------------------------------------------------------

/// 缓存统计（承接 v1 `PageManager::CacheStats` 读侧语义）。
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub reads: u64,
    pub evictions: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

struct CachedPage {
    data: Vec<u8>,
    last_access: u64,
}

/// 文件句柄 + LRU 页缓存页源（读侧）。
///
/// 为"反复读取大文件的小部分"（增量 watcher、按索引点查）设计；
/// 容量满时驱逐最久未访问页（读侧无脏页概念）。
pub struct PagedFile {
    file: File,
    ps: usize,
    capacity: usize,
    cache: HashMap<(u32, u32), CachedPage>,
    tick: u64,
    stats: CacheStats,
}

impl PagedFile {
    pub const DEFAULT_CAPACITY: usize = 1024;

    /// 打开文件并按 contracts C3.2 探测页大小：
    /// 以头部 latest-session pgno 在候选 {2048,4096,512}（按序）探测
    /// `page_type == Session(3)`；全失败回退头部 `0x34` 推断（再兜底 2048）。
    pub fn open(path: impl AsRef<Path>) -> Result<PagedFile, E3dError> {
        Self::open_with_capacity(path, Self::DEFAULT_CAPACITY)
    }

    pub fn open_with_capacity(
        path: impl AsRef<Path>,
        capacity: usize,
    ) -> Result<PagedFile, E3dError> {
        let mut file = File::open(path)?;
        let ps = Self::detect_page_size(&mut file)?;
        Ok(PagedFile {
            file,
            ps,
            capacity: capacity.max(1),
            cache: HashMap::new(),
            tick: 0,
            stats: CacheStats::default(),
        })
    }

    /// 显式页大小打开（跳过探测；测试/已知样本用）。
    pub fn open_with_page_size(
        path: impl AsRef<Path>,
        ps: usize,
        capacity: usize,
    ) -> Result<PagedFile, E3dError> {
        if !VALID_PAGE_SIZES.contains(&ps) {
            return Err(E3dError::Write(format!("invalid page size {ps}")));
        }
        let file = File::open(path)?;
        Ok(PagedFile {
            file,
            ps,
            capacity: capacity.max(1),
            cache: HashMap::new(),
            tick: 0,
            stats: CacheStats::default(),
        })
    }

    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    fn detect_page_size(file: &mut File) -> Result<usize, E3dError> {
        let file_len = file.metadata()?.len();
        let mut header = [0u8; 64];
        file.seek(SeekFrom::Start(0))?;
        let n = file.read(&mut header)?;
        if n < 64 {
            return Ok(2048);
        }
        let probe_pgno = be_u32_at(&header, HDR_LATEST_SES);
        if probe_pgno > 0 {
            for ps in PROBE_ORDER {
                let off = probe_pgno as u64 * ps as u64;
                if off + 4 > file_len {
                    continue;
                }
                file.seek(SeekFrom::Start(off))?;
                let mut buf = [0u8; 4];
                if file.read_exact(&mut buf).is_err() {
                    continue;
                }
                if i32::from_be_bytes(buf) == PAGE_TYPE_SESSION {
                    return Ok(ps);
                }
            }
        }
        Ok(page_size_from_header(&header))
    }

    fn evict_if_full(&mut self) {
        while self.cache.len() >= self.capacity {
            // LRU：驱逐 last_access 最小者（读侧无脏页，直接丢弃）。
            let victim = self
                .cache
                .iter()
                .min_by_key(|(_, p)| p.last_access)
                .map(|(k, _)| *k);
            match victim {
                Some(k) => {
                    self.cache.remove(&k);
                    self.stats.evictions += 1;
                }
                None => break,
            }
        }
    }
}

impl PageSource for PagedFile {
    fn page_size(&self) -> usize {
        self.ps
    }

    fn page(&mut self, ext_no: u32, pgno: u32) -> Result<&[u8], E3dError> {
        let key = (ext_no, pgno);
        self.tick += 1;
        let tick = self.tick;

        if let Some(p) = self.cache.get_mut(&key) {
            p.last_access = tick;
            self.stats.hits += 1;
            // NLL 限制下重新借用。
            return Ok(&self.cache.get(&key).unwrap().data);
        }

        self.stats.misses += 1;
        let off = pgno as u64 * self.ps as u64;
        let file_len = self.file.metadata()?.len();
        if off + self.ps as u64 > file_len {
            return Err(E3dError::Write(format!(
                "page {pgno} out of range (file has {} full pages of {} bytes)",
                file_len / self.ps as u64,
                self.ps
            )));
        }
        let mut data = vec![0u8; self.ps];
        self.file.seek(SeekFrom::Start(off))?;
        self.file.read_exact(&mut data)?;
        self.stats.reads += 1;

        self.evict_if_full();
        self.cache.insert(key, CachedPage { data, last_access: tick });
        Ok(&self.cache.get(&key).unwrap().data)
    }
}

// ---------------------------------------------------------------------------
// Tests (synthetic, sample-free; real-sample consistency lives with lib tests)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    /// 造一个 ps=2048 的合成库：page0 头部 + n 个内容页（页首 4 字节 = 页号标记）。
    fn synth(ps: usize, n_pages: usize, lie_header: bool, ses_pgno: u32) -> Vec<u8> {
        let mut buf = vec![0u8; ps * (n_pages + 1)];
        let claimed = if lie_header { 512 / 4 } else { ps / 4 } as u32;
        buf[HDR_PAGE_WORDS..HDR_PAGE_WORDS + 4].copy_from_slice(&claimed.to_be_bytes());
        buf[HDR_LATEST_SES..HDR_LATEST_SES + 4].copy_from_slice(&ses_pgno.to_be_bytes());
        for p in 1..=n_pages {
            let marker = (p as u32).to_be_bytes();
            buf[p * ps..p * ps + 4].copy_from_slice(&marker);
        }
        if ses_pgno > 0 {
            // 把 session 页 page_type 写成 3 供探测命中。
            let off = ses_pgno as usize * ps;
            buf[off..off + 4].copy_from_slice(&PAGE_TYPE_SESSION.to_be_bytes());
        }
        buf
    }

    fn write_temp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("e3d_io_pgsrc_{}_{}", std::process::id(), name));
        let mut f = File::create(&p).unwrap();
        f.write_all(bytes).unwrap();
        p
    }

    #[test]
    fn in_memory_page_bounds_and_content() {
        let ps = 2048;
        let buf = synth(ps, 3, false, 0);
        let mut src = InMemory::from_bytes(buf.clone());
        assert_eq!(src.page_size(), ps);
        // I1: 长度恒等于页大小；内容与直接切片一致。
        for pgno in 0..4u32 {
            let page = src.page(0, pgno).unwrap().to_vec();
            assert_eq!(page.len(), ps);
            assert_eq!(&page[..], &buf[pgno as usize * ps..(pgno as usize + 1) * ps]);
        }
        // 越界 = 错误而非截断。
        assert!(src.page(0, 4).is_err());
    }

    #[test]
    fn in_memory_infers_page_size_with_fallback() {
        // 头部声明非法值 -> 兜底 2048。
        let mut buf = vec![0u8; 4096];
        buf[HDR_PAGE_WORDS..HDR_PAGE_WORDS + 4].copy_from_slice(&7u32.to_be_bytes());
        assert_eq!(InMemory::from_bytes(buf).page_size(), 2048);
        // 合法 512。
        let mut buf = vec![0u8; 4096];
        buf[HDR_PAGE_WORDS..HDR_PAGE_WORDS + 4].copy_from_slice(&(512u32 / 4).to_be_bytes());
        assert_eq!(InMemory::from_bytes(buf).page_size(), 512);
    }

    #[test]
    fn paged_file_matches_in_memory_and_counts_cache() {
        let ps = 2048;
        let buf = synth(ps, 5, false, 0);
        let path = write_temp("consistency", &buf);

        let mut mem = InMemory::from_bytes(buf);
        let mut pf = PagedFile::open_with_page_size(&path, ps, 8).unwrap();
        assert_eq!(pf.page_size(), ps);

        // I2 + SC-003(合成版)：两页源逐页一致；重复取页命中缓存。
        for round in 0..2 {
            for pgno in 0..6u32 {
                let a = mem.page(0, pgno).unwrap().to_vec();
                let b = pf.page(0, pgno).unwrap().to_vec();
                assert_eq!(a, b, "page {pgno} mismatch (round {round})");
            }
        }
        let s = pf.stats();
        assert_eq!(s.misses, 6, "first round misses each page once");
        assert_eq!(s.hits, 6, "second round hits cache");
        assert_eq!(s.reads, 6);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn paged_file_lru_evicts_least_recently_used() {
        let ps = 512;
        let buf = synth(ps, 4, false, 0);
        let path = write_temp("lru", &buf);
        let mut pf = PagedFile::open_with_page_size(&path, ps, 2).unwrap();

        pf.page(0, 1).unwrap();
        pf.page(0, 2).unwrap(); // cache: {1,2}
        pf.page(0, 3).unwrap(); // 驱逐 1
        assert!(pf.stats().evictions >= 1);
        // 被驱逐页重读仍正确（I2）。
        let p1 = pf.page(0, 1).unwrap().to_vec();
        assert_eq!(&p1[..4], &1u32.to_be_bytes());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn paged_file_probe_overrides_lying_header() {
        // 头部谎称 512，但 session 页按真实 ps=2048 落位（ams1112 同款；契约 C3.2）。
        let ps = 2048;
        let buf = synth(ps, 3, true, 2);
        let path = write_temp("probe", &buf);
        let pf = PagedFile::open(&path).unwrap();
        assert_eq!(pf.page_size(), ps, "probe must trust page_type, not header");

        let _ = std::fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // 真实样本测试（specs/002 T105/T106；缺样本优雅跳过，沿用 001 惯例）
    // -----------------------------------------------------------------------

    const SAM: &str = r"D:\work\plant\pdms-io\pdms-test-data\sam7200_0001";
    const AMS: &str = r"D:\work\plant\pdms-io\test-file\ams1112_0001";

    /// T105（页级，SC-003 前半）：sam7200 经 `InMemory` 与 `PagedFile` 全文件
    /// 逐页字节一致；越界两源同判错。元素级枚举一致性依赖 T103 取页点改造，
    /// 落地后升级本测试为全库枚举对比。
    #[test]
    fn real_sample_dual_source_page_consistency() {
        if !std::path::Path::new(SAM).exists() {
            eprintln!("[skip] data absent");
            return;
        }
        let buf = std::fs::read(SAM).unwrap();
        let mut mem = InMemory::from_bytes(buf.clone());
        let mut pf = PagedFile::open(SAM).unwrap();
        assert_eq!(pf.page_size(), mem.page_size(), "probe vs header page size");
        let ps = mem.page_size();
        let full_pages = (buf.len() / ps) as u32;
        assert!(full_pages > 0);
        for pgno in 0..full_pages {
            let a = mem.page(0, pgno).unwrap().to_vec();
            let b = pf.page(0, pgno).unwrap().to_vec();
            assert_eq!(a, b, "page {pgno} mismatch");
        }
        assert!(mem.page(0, full_pages).is_err());
        assert!(pf.page(0, full_pages).is_err());
    }

    /// T105（探测样本，契约 C3.2）：ams1112 头部 page_size 不可信；以探测出的
    /// 页大小取 latest-session 页，其 page_type 必须真是 Session(3)（自洽校验，
    /// 不依赖外部先验）。
    #[test]
    fn real_sample_ams1112_probe_lands_session_page() {
        if !std::path::Path::new(AMS).exists() {
            eprintln!("[skip] data absent");
            return;
        }
        let mut pf = PagedFile::open(AMS).unwrap();
        assert!(VALID_PAGE_SIZES.contains(&pf.page_size()));
        let probe_pgno = {
            let p0 = pf.page(0, 0).unwrap();
            be_u32_at(p0, HDR_LATEST_SES)
        };
        if probe_pgno == 0 {
            eprintln!("[skip] no latest-session pointer");
            return;
        }
        let sp = pf.page(0, probe_pgno).unwrap();
        assert_eq!(
            i32::from_be_bytes([sp[0], sp[1], sp[2], sp[3]]),
            PAGE_TYPE_SESSION,
            "probed page size must land a real Session page"
        );
    }

    /// T106（SC-006）：增量式点查（头页 + 最新会话邻域，反复访问）的物理读页数
    /// 远小于全文件页数，且缓存命中可观测。
    #[test]
    fn real_sample_incremental_reads_fewer_pages() {
        if !std::path::Path::new(SAM).exists() {
            eprintln!("[skip] data absent");
            return;
        }
        let file_len = std::fs::metadata(SAM).unwrap().len();
        let mut pf = PagedFile::open(SAM).unwrap();
        let total_pages = file_len / pf.page_size() as u64;
        let ses = {
            let p0 = pf.page(0, 0).unwrap();
            be_u32_at(p0, HDR_LATEST_SES)
        };
        for _ in 0..3 {
            pf.page(0, 0).unwrap();
            if ses > 0 {
                pf.page(0, ses).unwrap();
                for d in 1..=4u32 {
                    if ses > d {
                        pf.page(0, ses - d).unwrap();
                    }
                }
            }
        }
        let s = pf.stats();
        assert!(
            (s.reads as u64) < total_pages,
            "incremental access must read fewer pages than file total ({} >= {})",
            s.reads,
            total_pages
        );
        assert!(s.hits > 0, "repeated access must hit cache");
    }
}
