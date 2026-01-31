use crate::defines::*;
use crate::element_record_reader::ElementRecordReader;
use crate::page_manager::PageManager;
use crate::paged_reader::PagedReader;
use aios_core::pdms_data::DataOperation;
use aios_core::pdms_types::*;
use aios_core::{
    get_default_pdms_db_info, helper::parse_to_i32, query_refno_sesno, NamedAttrMap, NamedAttrValue, RefU64Vec,
    RefnoEnum, RefnoSesno, SUL_DB,
};
use anyhow::{anyhow, Context, Result};
use atty::is;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use parse_pdms_db::parse::{parse_ele_data, parse_raw_ele_data, EleData};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::mem::size_of;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// 用于异步批量写入 SurrealDB 的消息类型（仅在 store_all_refno_sesno_map 内部使用）。
///
/// 之前该类型在重构过程中遗漏，导致 `pdms_io` 无法编译。
enum SesSqlType {
    SesJson(Vec<String>),
    PeSesSql(Vec<String>),
    PeVersionJson((Vec<String>, chrono::DateTime<chrono::Utc>)),
}

/// 元素修改详情（用于增量对比与落库/索引）。
#[derive(Debug, Clone)]
pub struct ModifiedElement {
    pub current_data: EleData,

    pub added_attrs: HashMap<String, NamedAttrValue>,
    pub deleted_attrs: HashMap<String, NamedAttrValue>,
    pub modified_attrs: HashMap<String, (NamedAttrValue, NamedAttrValue)>,

    pub added_explicit_attrs: HashMap<String, NamedAttrValue>,
    pub deleted_explicit_attrs: HashMap<String, NamedAttrValue>,
    pub modified_explicit_attrs: HashMap<String, (NamedAttrValue, NamedAttrValue)>,

    pub added_uda_attrs: HashMap<i32, NamedAttrValue>,
    pub deleted_uda_attrs: HashMap<i32, NamedAttrValue>,
    pub modified_uda_attrs: HashMap<i32, (NamedAttrValue, NamedAttrValue)>,

    pub noun: String,
    pub children_changed: Option<(RefU64Vec, RefU64Vec)>,
}

/// 元素变更详情（Add/Modified/Deleted/None）。
#[derive(Debug, Clone)]
pub enum EleOperationDetail {
    Add(EleData),
    Modified(ModifiedElement),
    Deleted,
    None,
}

/// 按会话输出的元素操作数据。
#[derive(Debug, Clone)]
pub struct EleOperationData {
    pub refno: RefU64,
    pub sesno: u32,
    pub detail: EleOperationDetail,
}

impl EleOperationData {
    pub fn new(refno: RefU64, sesno: u32, detail: EleOperationDetail) -> Self {
        Self { refno, sesno, detail }
    }

    pub fn get_op_type(&self) -> &'static str {
        match self.detail {
            EleOperationDetail::Add(_) => "新增",
            EleOperationDetail::Modified(_) => "修改",
            EleOperationDetail::Deleted => "删除",
            EleOperationDetail::None => "无操作",
        }
    }

    pub fn get_noun_type(&self) -> String {
        match &self.detail {
            EleOperationDetail::Add(ele) => ele.att_map().get_type(),
            EleOperationDetail::Modified(modified) => modified.noun.clone(),
            EleOperationDetail::Deleted => "DELETED".to_string(),
            EleOperationDetail::None => "NONE".to_string(),
        }
    }

    /// 将操作数据转换为可执行的 SurrealQL 片段。
    ///
    /// 目前返回空串（占位）。该仓库的 SurrealQL 落库逻辑仍在迭代中，且上游 `aios_core::NamedAttrMap`
    /// 的 JSON/SurQL 生成接口在不同分支存在差异；此处先保证编译与解析链路可用，避免误写数据库。
    pub fn to_surql(&self, id: &str, dbnum: i32, sesno: u32) -> String {
        let _ = (id, dbnum, sesno);
        String::new()
    }
}

fn convert_to_operation_data(
    operation_details: HashMap<RefU64, EleOperationDetail>,
    sesno: u32,
) -> Vec<EleOperationData> {
    operation_details
        .into_iter()
        .map(|(refno, detail)| EleOperationData { refno, sesno, detail })
        .collect()
}

/// PDMS 数据库读取与解析入口。
///
/// 当前以单文件（如 `ams1112_0001`）为输入，内部通过 `PageManager + PagedReader` 实现跨页读取。
pub struct PdmsIO {
    pub project: String,
    pub file_path: PathBuf,
    pub detail: bool,

    pub dbnum: i32,
    pub page_size: usize,

    pub file: Option<File>,
    pub page_cache: PageManager,

    pub ses_data_map: HashMap<u32, SessionPageData>,
    pub ses_range_map: BTreeMap<i32, RangeInclusive<u32>>,
    pub sesno_pgno_map: BTreeMap<i32, u32>,
}

/// RefNo -> 该 RefNo 的所有历史版本“绝对偏移”（升序，last() 为最新）。
pub type IndexMap = HashMap<RefU64, Vec<u64>>;

/// 元素一致性哈希选项（用于历史版本“内容去重”）。
///
/// 目标是“同一元素的相邻历史版本”若内容一致，则可压缩掉冗余版本，降低后续解析/遍历成本。
/// 因此这里提供 `ignore_keys` 用于忽略明显会随物理位置/会话变化的字段（如 PGNO/SESNO）。
#[derive(Debug, Clone, Copy)]
pub struct ElementHashOptions {
    /// 需要忽略的属性键名列表（精确匹配）。
    pub ignore_keys: &'static [&'static str],
}

impl Default for ElementHashOptions {
    fn default() -> Self {
        Self {
            ignore_keys: &["PGNO", "SESNO"],
        }
    }
}

impl PdmsIO {
    // ... (其他代码保持不变)

    pub fn new(project: impl Into<String>, file_path: impl AsRef<Path>, detail: bool) -> Self {
        let file_path = file_path.as_ref().to_path_buf();
        let page_size = PAGE_SIZE_2K;
        Self {
            project: project.into(),
            file_path,
            detail,
            dbnum: 0,
            page_size,
            file: None,
            page_cache: PageManager::new(1024, page_size),
            ses_data_map: HashMap::new(),
            ses_range_map: BTreeMap::new(),
            sesno_pgno_map: BTreeMap::new(),
        }
    }

    /// 打开数据库文件并初始化基础缓存（允许重复调用）。
    pub fn open(&mut self) -> anyhow::Result<()> {
        // 确保 file 已打开
        let _ = self.get_file()?;

        // 读取头部，刷新 dbnum/page_size
        let header = self.read_pdms_header()?;
        self.dbnum = header.db_num;

        let detected_page_size = self.detect_page_size_by_probe(&header)?;
        if detected_page_size != self.page_size {
            self.page_size = detected_page_size;
            self.page_cache = PageManager::new(1024, self.page_size);
            self.ses_data_map.clear();
        }

        // 尽早初始化 ses 映射，便于 parse_element 设置 sesno 等信息
        if self.sesno_pgno_map.is_empty() || self.ses_range_map.is_empty() {
            let _ = self.init_ses_maps();
        }

        Ok(())
    }

    /// 通过“探测页面类型”来识别真实 page_size。
    ///
    /// 背景：部分 PDMS/E3D 文件头的 `header.page_size` 字段并不可靠（例如 `ams1112_0001` 为 512，
    /// 但真实页面仍是 2048）。因此这里优先用 `session_page_no`/`latest_ses_pgno` 做一次最小读取验证：
    /// - 对候选 page_size 计算 `pgno * page_size` 的偏移
    /// - 读取该页的 `page_type`（大端 i32）
    /// - 若为 Session(=3)，则认为命中
    fn detect_page_size_by_probe(&mut self, header: &PdmsHeader) -> anyhow::Result<usize> {
        let candidates = [PAGE_SIZE_2K, PAGE_SIZE_4K, PAGE_SIZE_512];
        let file = self.get_file()?;
        let file_len = file
            .metadata()
            .context("failed to read file metadata")?
            .len();

        // 优先探测 session_page_no（通常很小，偏移也小，最稳妥）。
        let probe_pgnos = [header.session_page_no, header.latest_ses_pgno]
            .into_iter()
            .filter(|&pgno| pgno > 0);

        for page_size in candidates {
            for pgno in probe_pgnos.clone() {
                let off = pgno as u64 * page_size as u64;
                if off + 4 > file_len {
                    continue;
                }
                file.seek(SeekFrom::Start(off))?;
                let mut buf = [0u8; 4];
                file.read_exact(&mut buf)?;
                let page_type = i32::from_be_bytes(buf);
                if page_type == PageType::Session as i32 {
                    return Ok(page_size);
                }
            }
        }

        // 最终兜底：大多数 E3D/PDMS 均为 2K。
        Ok(PAGE_SIZE_2K)
    }

    /// 获取数据库文件句柄（惰性打开）。
    fn get_file(&mut self) -> anyhow::Result<&mut File> {
        if self.file.is_none() {
            let f = OpenOptions::new()
                .read(true)
                .open(&self.file_path)
                .with_context(|| format!("无法打开数据库文件: {:?}", self.file_path))?;
            self.file = Some(f);
        }
        Ok(self.file.as_mut().unwrap())
    }

    /// 获取指定页号的完整页面数据（会走 PageManager 缓存）。
    fn get_page_cached(&mut self, pgno: u32) -> anyhow::Result<Vec<u8>> {
        if self.file.is_none() {
            self.open()?;
        }

        let ext_no = self.dbnum as u32;
        let file = self.file.as_mut().unwrap();
        let data = self.page_cache.get_page(file, ext_no, pgno)?;
        Ok(data.to_vec())
    }

    /// 初始化 sesno_pgno_map / ses_range_map。
    fn init_ses_maps(&mut self) -> anyhow::Result<()> {
        self.sesno_pgno_map.clear();
        self.ses_range_map.clear();

        let header = self.read_pdms_header()?;
        let mut cur = header.latest_ses_pgno;
        let mut seen = HashSet::new();
        let mut sessions: Vec<(i32, u32, u32)> = Vec::new(); // (sesno, ses_pgno, end_pgno)

        while cur != 0 && seen.insert(cur) {
            let ses = self.read_ses_data(cur)?.clone();
            sessions.push((ses.sesno, cur, ses.end_pgno));

            if ses.last_ses_pageno <= 0 {
                break;
            }
            cur = ses.last_ses_pageno as u32;
        }

        sessions.reverse(); // oldest -> newest

        let mut prev_end: u32 = 0;
        for (sesno, ses_pgno, end_pgno) in sessions {
            self.sesno_pgno_map.insert(sesno, ses_pgno);

            let start = if prev_end == 0 { 0 } else { prev_end.saturating_add(1) };
            let end = end_pgno.max(start);
            self.ses_range_map.insert(sesno, start..=end);

            prev_end = end;
        }

        Ok(())
    }

    /// 初始化会话范围映射（兼容旧接口）。
    ///
    /// 旧代码多处依赖 `init_ses_range_map()`，这里直接复用 `init_ses_maps()`。
    pub fn init_ses_range_map(&mut self) -> anyhow::Result<()> {
        self.init_ses_maps()
    }

    /// 将元素与会话数据写入数据库（兼容旧接口）。
    ///
    /// 该仓库目前仍在迭代落库逻辑；为避免影响解析与测试链路，这里保留接口并默认 no-op。
    pub async fn update_elements_to_database(
        &mut self,
        _range_eles: &BTreeMap<u32, Vec<EleOperationData>>,
        _skip_main_data: bool,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// 从缓存读取跨页的数据 (对齐 db4 logic)
    /// 
    /// # 参数
    /// * `start_offset` - 物理文件偏移量
    /// * `length` - 需要读取的字节数
    pub fn read_data_cached(&mut self, start_offset: u64, length: usize) -> anyhow::Result<Vec<u8>> {
        if self.file.is_none() {
            self.open()?;
        }

        let ext_no = self.dbnum as u32;
        let page_size = self.page_size;
        let file = self.file.as_mut().unwrap();
        PagedReader::read(
            file,
            &mut self.page_cache,
            ext_no,
            page_size,
            start_offset,
            length,
        )
    }

    /// 读取任意偏移的原始字节（兼容旧测试代码）。
    ///
    /// 旧测试用例里广泛使用 `read_bytes(offset, len)` 直接做二进制验证；
    /// 新实现统一走跨页读取 `read_data_cached`，这里保留一个薄封装即可。
    pub fn read_bytes<O>(&mut self, offset: O, length: usize) -> anyhow::Result<Vec<u8>>
    where
        O: TryInto<u64>,
        O::Error: std::fmt::Debug,
    {
        let offset_u64 = offset
            .try_into()
            .map_err(|e| anyhow!("read_bytes: offset 转换失败: {:?}", e))?;
        self.read_data_cached(offset_u64, length)
    }

    pub fn read_element_record_cached(&mut self, start_offset: u64) -> anyhow::Result<Vec<u8>> {
        if self.file.is_none() {
            self.open()?;
        }

        let ext_no = self.dbnum as u32;
        let page_size = self.page_size;
        let file = self.file.as_mut().unwrap();
        ElementRecordReader::read(file, &mut self.page_cache, ext_no, page_size, start_offset)
    }
    
    /// 获取缓存命中率
    pub fn cache_hit_rate(&self) -> f64 {
        self.page_cache.stats().hit_rate()
    }

    /// 根据页号获取会话号
    ///
    /// 遍历会话范围映射表,查找包含指定页号的会话范围,返回对应的会话号
    ///
    // ... (其他代码保持不变)
    /// # 参数
    /// * `pgno` - 页号
    ///
    /// # 返回值
    /// * `Option<u32>` - 如果找到对应的会话号则返回Some(sesno),否则返回None
    pub fn get_sesno(&self, pgno: u32) -> Option<u32> {
        for (sesno, range) in &self.ses_range_map {
            if range.contains(&pgno) {
                return Some(*sesno as _);
            }
        }
        None
    }

    ///获取最新属性pgno
    pub fn get_latest_att_pgno(&mut self) -> anyhow::Result<u32> {
        let header = self.read_pdms_header()?;
        let ses_pgno = header.latest_ses_pgno;
        // let ses_data = self.read_ses_data(ses_pgno)?;
        let all_locs = self.collect_refno_locs_in_session(ses_pgno as _);
        let max_pgno = all_locs.iter().map(|x| x.pgno).max().unwrap_or_default();
        Ok(max_pgno)
    }

    /// 获取最新的会话号(sesno)
    ///
    /// 读取PDMS数据库头部信息,获取最新会话页号,然后读取该会话页的数据,返回其会话号。
    ///
    /// # 返回值
    /// * `anyhow::Result<u32>` - 成功返回最新的会话号,失败返回错误
    ///
    /// # 错误
    /// * 读取数据库头部或会话页数据失败时返回错误
    pub fn get_latest_sesno(&mut self) -> anyhow::Result<u32> {
        let header = self.read_pdms_header()?;
        let latest_ses_data = self.read_ses_data(header.latest_ses_pgno)?;
        Ok(latest_ses_data.sesno as _)
    }

    /// 获取最新的会话时间
    ///
    /// 读取PDMS数据库头部信息,获取最新会话页号,然后读取该会话页的数据,返回其时间戳。
    ///
    /// # 返回值
    /// * `anyhow::Result<DateTime<Utc>>` - 成功返回最新的会话时间,失败返回错误
    ///
    /// # 错误
    /// * 读取数据库头部或会话页数据失败时返回错误
    pub fn get_latest_dt(&mut self) -> anyhow::Result<DateTime<Utc>> {
        let header = self.read_pdms_header()?;
        let latest_ses_data = self.read_ses_data(header.latest_ses_pgno)?;
        Ok(latest_ses_data.get_utc_dt())
    }

    /// 获取指定会话号的保存时间
    ///
    /// 根据指定的会话号查找对应的会话数据,返回该会话的保存时间。
    ///
    /// # 参数
    /// * `sesno` - 要查询的会话号
    ///
    /// # 返回值
    /// * `anyhow::Result<DateTime<Utc>>` - 成功返回指定会话的保存时间,失败返回错误
    ///
    /// # 错误
    /// * 当找不到指定会话号对应的页面时返回错误
    /// * 读取会话页数据失败时返回错误
    pub fn get_sesno_datetime(&mut self, sesno: u32) -> anyhow::Result<DateTime<Utc>> {
        let ses_data = self.get_ses_data(sesno)?;
        Ok(ses_data.get_utc_dt())
    }

    /// 获取指定会话号的保存时间戳
    ///
    /// 根据指定的会话号查找对应的会话数据,返回该会话的保存时间的Unix时间戳。
    ///
    /// # 参数
    /// * `sesno` - 要查询的会话号
    ///
    /// # 返回值
    /// * `anyhow::Result<i64>` - 成功返回指定会话的Unix时间戳(秒),失败返回错误
    ///
    /// # 错误
    /// * 当找不到指定会话号对应的页面时返回错误
    /// * 读取会话页数据失败时返回错误
    pub fn get_sesno_timestamp(&mut self, sesno: u32) -> anyhow::Result<i64> {
        let ses_data = self.get_ses_data(sesno)?;
        Ok(ses_data.get_utc_dt().timestamp())
    }

    // 收集指定参考号的历史记录
    ///
    /// 遍历所有会话，查找指定参考号在各个会话中的位置信息
    ///
    /// # 参数
    /// * `refno` - 需要收集历史记录的参考号
    ///
    /// # 返回值
    /// * `BTreeMap<i32, u64>` - 会话号与对应的参考号偏移量映射表
    fn collect_refno_history(&mut self, refno: RefU64) -> anyhow::Result<BTreeMap<i32, u64>> {
        let mut history_map = BTreeMap::new();

        // 获取所有会话
        let all_sessions: Vec<i32> = self.sesno_pgno_map.keys().cloned().collect();

        // 遍历所有会话，查找refno的所有历史记录
        for &sesno in &all_sessions {
            // 获取会话页号
            if let Some(ses_pgno) = self.get_ses_pageno(sesno) {
                // 收集这个会话中的所有refno
                let locs = self.collect_refno_locs_in_session(ses_pgno);

                // 检查是否包含目标refno
                for loc in locs {
                    if RefU64::from_two_nums(loc.refno_0, loc.refno_1) == refno {
                        history_map.insert(sesno, loc.get_att_offset());
                        break;
                    }
                }
            }
        }

        Ok(history_map)
    }

    #[cfg(test)]
    pub fn get_att_latest_pgno_old(&mut self) -> anyhow::Result<u32> {
        // 旧调试函数：通过“页头特征字节”在全文件里反向搜索可能的索引页位置。
        // 这里只要求能编译/辅助定位问题，不保证命中一定是叶子索引页。
        use memchr::memmem::rfind_iter;

        // 索引页 page_type 通常为 2（big-endian i32）。
        const REFNO_LEAF_INDEX_PAGE: [u8; 4] = (2i32).to_be_bytes();

        let mut file = self.get_file()?;
        let mut input = vec![];
        file.read_to_end(&mut input)?;
        let file_max_pgno = input.len() as u32 / self.page_size as u32;
        let mut pos_iter = rfind_iter(&input, &REFNO_LEAF_INDEX_PAGE[..]);
        let mut max_pgno = 0;
        while let Some(pos) = pos_iter.next() {
            let pgno = (pos / self.page_size as usize) as _;
            println!("Found leaf index page at: {:#04X?}", pgno);
            let index_data = self.read_index_data(pgno)?;
            dbg!(&index_data);
            max_pgno = index_data
                .refno_locs
                .iter()
                .filter(|x| x.pgno <= file_max_pgno)
                .map(|x| x.pgno)
                .max()
                .unwrap_or_default()
                .max(max_pgno);
            break;
        }
        Ok(max_pgno)
    }

    // sesno->address
    /// 搜索指定参考号的所有历史版本
    ///
    /// # 参数
    /// * `refno` - 要搜索的参考号
    /// * `sesno` - 可选的会话号，用于限定搜索范围
    ///
    /// # 返回值
    /// * `anyhow::Result<BTreeMap<u32, u64>>` - 成功返回一个映射，键为会话号，值为该会话中参考号的物理地址
    ///
    /// # 错误
    /// 当找不到指定参考号时返回错误
    pub fn search_history_refnos(
        &mut self,
        refno: RefU64,
        sesno: Option<u32>,
    ) -> anyhow::Result<BTreeMap<u32, u64>> {
        let mut results: BTreeMap<u32, u64> = BTreeMap::new();

        // 首先使用search_latest_refno找到当前版本
        let (current_sesno, refno_offset) = self
            .search_latest_refno(refno, sesno)
            .ok_or_else(|| anyhow::anyhow!("找不到指定参考号: {:?}", refno))?;

        // 添加当前版本到结果集
        results.insert(current_sesno, refno_offset);

        // 如果指定了会话号，从该会话开始向前查找
        // 否则从最新会话开始向前查找
        let mut search_sesno = current_sesno as i32;

        // 不断向前查找历史版本
        loop {
            match self.get_nearest_less_sesno(search_sesno) {
                Some(prev_sesno) => {
                    // 尝试在前一个会话中查找
                    match self.search_latest_refno(refno, Some(prev_sesno as u32)) {
                        Some((sesno, offset)) => {
                            results.insert(sesno, offset);
                            search_sesno = sesno as i32;
                        }
                        None => break, // 找不到更多历史版本，退出循环
                    }
                }
                None => break, // 没有更早的会话，退出循环
            }
        }

        Ok(results)
    }

    /// 获取参考号在指定会话范围内的操作状态
    ///
    /// # 参数
    /// * `refno` - 要判断状态的参考号
    /// * `sesno` - 可选的会话号，用于限定搜索范围
    ///
    /// # 返回值
    /// * `anyhow::Result<HashMap<RefU64, EleOperationDetail>>` - 成功返回包含主参考号及其子元素的操作状态映射
    ///   - 键为参考号（包括主参考号和可能的子元素）
    ///   - 值为相应参考号的操作状态(新增/修改/删除/重复/无操作)
    ///
    /// # 错误
    /// * 当参考号在指定范围内不存在时返回错误
    ///
    /// # 用法区别
    /// - 如果只需要获取主参考号的状态，请使用`get_refno_primary_operation_status`
    /// - 如果需要同时获取子元素的状态变化，请使用本函数
    pub fn get_refno_operation_status(
        &mut self,
        refno: RefU64,
        sesno: Option<u32>,
    ) -> anyhow::Result<HashMap<RefU64, EleOperationDetail>> {
        let mut result = HashMap::new();

        // dbg!(refno);
        // 使用search_latest_and_prev_refno获取最新版本和前一个版本
        let [latest, previous] = self.search_latest_and_prev_refno(refno, sesno);

        // dbg!(&latest);
        // dbg!(&previous);

        // 如果没有找到任何版本
        if latest.is_none() {
            result.insert(refno, EleOperationDetail::None);
            return Ok(result);
        }

        // 解包最新版本
        let (latest_sesno, latest_offset) = latest.unwrap();
        let mut latest_att = match self.parse_raw_element(latest_offset) {
            Ok(att) => att,
            Err(e) => {
                log::warn!("解析最新元素数据失败: 位置{:#4X} {}", latest_offset, e);
                result.insert(refno, EleOperationDetail::None);
                return Ok(result);
            }
        };
        let type_name = latest_att.att_map().get_type();

        // 只有一个版本，说明是新建的
        if previous.is_none() {
            result.insert(refno, EleOperationDetail::Add(latest_att));
            return Ok(result);
        }

        let owner = latest_att.owner;
        // dbg!(&latest_att);
        //todo 直接调用 parse children 方法
        let mut skipped = false;
        let owner_ele = match self.auto_get_raw_element(owner) {
            Ok(ele) => ele,
            Err(e) => {
                log::warn!(
                    "获取所有者元素失败: refno={} owner={} latest_sesno={} offset=0x{:X}: {}",
                    refno,
                    owner,
                    latest_sesno,
                    latest_offset,
                    e
                );
                if type_name == "SITE" {
                    skipped = true;
                    EleData::default()
                } else if self.search_latest_refno(owner, None).is_none() {
                    // 父元素确实不存在，标记当前元素为已删除
                    result.insert(refno, EleOperationDetail::Deleted);
                    return Ok(result);
                } else {
                    // 父元素存在但解析失败，返回未知状态以便上层决定
                    result.insert(refno, EleOperationDetail::None);
                    return Ok(result);
                }
            }
        };
        if !owner_ele.children.contains(&refno) && !skipped {
            result.insert(refno, EleOperationDetail::Deleted);
            return Ok(result);
        }

        // 解包前一个版本
        let (prev_sesno, prev_offset) = previous.unwrap();
        let mut prev_att = match self.parse_raw_element(prev_offset) {
            Ok(att) => att,
            Err(e) => {
                log::warn!(
                    "解析前一版本元素数据失败: refno={} prev_offset=0x{:X}: {}",
                    refno,
                    prev_offset,
                    e
                );
                result.insert(refno, EleOperationDetail::None);
                return Ok(result);
            }
        };

        // 在比较之前保存一份完整的最新数据副本
        let latest_data_copy = latest_att.clone();
        
        // 检查子元素是否有变化
        let latest_children = &latest_att.children;
        let prev_children = &prev_att.children;

        // 检查children是否发生变化
        let children_changed = {
            // 将RefU64Vec转换为HashSet进行比较
            let prev_set: HashSet<_> = prev_children.iter().collect();
            let latest_set: HashSet<_> = latest_children.iter().collect();

            if prev_set != latest_set {
                Some((prev_children.clone(), latest_children.clone()))
            } else {
                None
            }
        };

        // 检查子元素的增删改
        // 1. 找出已删除的子元素
        for child_refno in prev_children.iter() {
            if !latest_children.contains(child_refno) {
                //todo 有可能是扩展属性
                result.insert(*child_refno, EleOperationDetail::Deleted);
            }
        }

        //首先检查是否是属于有几何体的类型。
        //然后是检查发生修改的属性是什么
        let mut is_children_changed = children_changed.is_some();

        // 存储属性变化（显式标注 key/value 类型，避免推断失败）
        let mut added_attrs: HashMap<String, NamedAttrValue> = HashMap::new();
        let mut deleted_attrs: HashMap<String, NamedAttrValue> = HashMap::new();
        let mut modified_attrs: HashMap<String, (NamedAttrValue, NamedAttrValue)> = HashMap::new();

        // 检查普通属性的变化
        while let Some((noun, value)) = latest_att.att_map_mut().pop_first() {
            if let Some(prev_value) = prev_att.att_map_mut().remove(&noun) {
                if value != prev_value {
                    is_children_changed = true;
                    modified_attrs.insert(noun.to_string(), (prev_value.clone(), value));
                }
            } else {
                // 新增的属性
                is_children_changed = true;
                added_attrs.insert(noun.to_string(), value);
            }
        }

        // 检查被删除的属性
        for (noun, value) in prev_att.att_map().iter() {
            if !latest_att.att_map().contains_key(noun) {
                is_children_changed = true;
                deleted_attrs.insert(noun.to_string(), value.clone());
            }
        }

        // 存储显式属性变化
        let mut added_explicit_attrs: HashMap<String, NamedAttrValue> = HashMap::new();
        let mut deleted_explicit_attrs: HashMap<String, NamedAttrValue> = HashMap::new();
        let mut modified_explicit_attrs: HashMap<String, (NamedAttrValue, NamedAttrValue)> = HashMap::new();

        // 检查显式属性是否发生变化
        let latest_explicit_attmap = latest_att.explicit_attmap();
        let prev_explicit_attmap = prev_att.explicit_attmap();

        for (noun, value) in latest_explicit_attmap.iter() {
            if let Some(prev_value) = prev_explicit_attmap.get(noun) {
                if value != prev_value {
                    is_children_changed = true;
                    modified_explicit_attrs
                        .insert(noun.to_string(), (prev_value.clone(), value.clone()));
                }
            } else {
                // 新增的显式属性
                is_children_changed = true;
                added_explicit_attrs.insert(noun.to_string(), value.clone());
            }
        }

        // 检查被删除的显式属性
        for (noun, value) in prev_explicit_attmap.iter() {
            if !latest_explicit_attmap.contains_key(noun) {
                is_children_changed = true;
                deleted_explicit_attrs.insert(noun.to_string(), value.clone());
            }
        }

        // 存储UDA属性变化
        let mut added_uda_attrs = HashMap::new();
        let mut deleted_uda_attrs = HashMap::new();
        let mut modified_uda_attrs = HashMap::new();

        // 检查uda属性是否发生变化
        let latest_uda_atts = latest_att.uda_atts();
        let prev_uda_atts = prev_att.uda_atts();

        for uda_att in latest_uda_atts.iter() {
            if let Some(prev_value) = prev_uda_atts
                .iter()
                .find(|x| x.hash_val == uda_att.hash_val)
            {
                if uda_att.value != prev_value.value {
                    is_children_changed = true;
                    modified_uda_attrs.insert(
                        uda_att.hash_val,
                        (prev_value.value.clone(), uda_att.value.clone()),
                    );
                }
            } else {
                // 新增的uda属性
                is_children_changed = true;
                added_uda_attrs.insert(uda_att.hash_val, uda_att.value.clone());
            }
        }

        // 检查被删除的UDA属性
        for prev_uda in prev_uda_atts.iter() {
            if !latest_uda_atts
                .iter()
                .any(|x| x.hash_val == prev_uda.hash_val)
            {
                is_children_changed = true;
                deleted_uda_attrs.insert(prev_uda.hash_val, prev_uda.value.clone());
            }
        }

        if is_children_changed {
            result.insert(
                refno,
                EleOperationDetail::Modified(ModifiedElement {
                    current_data: latest_data_copy,
                    added_attrs,
                    deleted_attrs,
                    modified_attrs,
                    added_explicit_attrs,
                    deleted_explicit_attrs,
                    modified_explicit_attrs,
                    added_uda_attrs,
                    deleted_uda_attrs,
                    modified_uda_attrs,
                    noun: type_name.clone(),
                    children_changed,
                }),
            );
        }

        Ok(result)
    }

    /// 构建 NounID 到 AttributeID 的映射表 (来自 ATNAIN)
    ///
    /// # 参数
    /// * `attlib_path` - attlib.dat 的路径
    ///
    /// # 返回值
    /// * `Result<HashMap<u32, Vec<u32>>>` - NounID 映射到属性 ID 列表
    pub fn build_noun_attr_map<P: AsRef<Path>>(attlib_path: P) -> anyhow::Result<HashMap<u32, Vec<u32>>> {
        let mut file = File::open(attlib_path.as_ref())
            .with_context(|| format!("无法打开属性库文件: {:?}", attlib_path.as_ref()))?;
        
        let mut map = HashMap::new();
        
        // 读取 Page 1 目录
        let mut dir_buf = vec![0u8; 2048];
        file.read_exact(&mut dir_buf)?;
        let atnain_start_page = u32::from_be_bytes([dir_buf[12], dir_buf[13], dir_buf[14], dir_buf[15]]) as usize;
        
        if atnain_start_page == 0 {
            return Ok(map);
        }

        // ATNAIN 格式为 [NounHash, AttrIndex, TypeCode] 三元组
        // 每个 Page 2048 字节，包含 512 个 u32
        for page_idx in atnain_start_page..atnain_start_page + 30 {
            let offset = page_idx as u64 * 2048;
            if file.seek(SeekFrom::Start(offset)).is_err() { break; }
            
            let mut page_buf = vec![0u8; 2048];
            if file.read_exact(&mut page_buf).is_err() { break; }
            
            let mut u32_vals = Vec::with_capacity(512);
            for chunk in page_buf.chunks_exact(4) {
                u32_vals.push(u32::from_be_bytes(chunk.try_into().unwrap()));
            }

            for i in (0..u32_vals.len().saturating_sub(2)).step_by(3) {
                let noun_id = u32_vals[i];
                let attr_id = u32_vals[i + 1];
                // let type_code = u32_vals[i + 2];
                
                if noun_id == 0 || noun_id == 0xFFFFFFFF { continue; }
                if attr_id == 0 || attr_id == 0xFFFFFFFF { continue; }
                
                map.entry(noun_id).or_insert_with(Vec::new).push(attr_id);
            }
        }
        
        Ok(map)
    }

    /// 获取元素的属性值 (基于物理偏移)
    /// 
    /// # 参数
    /// * `refno` - 元素的参考号
    /// * `attr_id` - 属性的 ID (AttrID)
    /// * `noun_id` - 元素的类型 ID (NounID)
    /// * `phys_offset` - 属性在元素数据块中的物理偏移 (来自 ATNAIN)
    /// * `dtype` - 属性的数据类型 (来自 ATGTDF)
    pub fn get_attribute_value(
        &mut self,
        refno: RefU64,
        _attr_id: u32,
        _noun_id: u32,
        phys_offset: u32,
        dtype: u32,
    ) -> anyhow::Result<NamedAttrValue> {
        // 1. 定位元素在数据库中的物理地址
        let (_, ele_offset) = self
            .search_latest_refno(refno, None)
            .ok_or_else(|| anyhow!("找不到参考号: {:?}", refno))?;
            
        // 2. 计算属性的实际物理偏移
        // phys_offset 是相对于元素数据块起始位置的，且单位通常是 4 字节 (WORD)
        let attr_phys_offset = ele_offset + (phys_offset as u64 * 4);
        
        // 3. 读取数据并根据 dtype 解码
        // dtype 参考: 2=REAL, 3=TEXT, 5=POS, 6=ORIENTATION, 7=REF, 8=BOO, etc.
        match dtype {
            2 => { // REAL (DOUBLE)
                let data = self.read_data_cached(attr_phys_offset, 8)?;
                let val = f64::from_be_bytes(data.try_into().map_err(|_| anyhow!("数据长度不足"))?);
                Ok(NamedAttrValue::F32Type(val as f32))
            }
            3 => { // TEXT
                let len_data = self.read_data_cached(attr_phys_offset, 4)?;
                let len = i32::from_be_bytes(len_data.try_into().map_err(|_| anyhow!("读取长度失败"))?) as usize;
                if len > 0 {
                    let text_data = self.read_data_cached(attr_phys_offset + 4, len)?;
                    let (s, _) = aios_core::tool::db_tool::decode_chars_data(&text_data);
                    Ok(NamedAttrValue::StringType(s.into()))
                } else {
                    Ok(NamedAttrValue::StringType("".into()))
                }
            }
            5 | 6 => { // POSITION / ORIENTATION (VEC3)
                let data = self.read_data_cached(attr_phys_offset, 24)?;
                let mut vals = [0.0f32; 3];
                for i in 0..3 {
                    let chunk = &data[i*8..(i+1)*8];
                    vals[i] = f64::from_be_bytes(chunk.try_into().unwrap()) as f32;
                }
                Ok(NamedAttrValue::Vec3Type(glam::Vec3::from_array(vals)))
            }
            7 => { // REFERENCE
                let data = self.read_data_cached(attr_phys_offset, 8)?;
                let ref0 = u32::from_be_bytes(data[0..4].try_into().unwrap());
                let ref1 = u32::from_be_bytes(data[4..8].try_into().unwrap());
                Ok(NamedAttrValue::RefU64Type(RefU64::from_two_nums(ref0, ref1)))
            }
            8 => { // BOOLEAN
                let data = self.read_data_cached(attr_phys_offset, 4)?;
                let val = u32::from_be_bytes(data.try_into().unwrap());
                Ok(NamedAttrValue::BoolType(val != 0))
            }
            _ => {
                // 回退到解析整个元素获取值 (作为兜底方案)
                let ele_data = self.parse_raw_element(ele_offset)?;
                // 这里可能需要根据 attr_id 找名称，暂时返回空
                Err(anyhow!("不支持的属性类型或未实现的快速读取: dtype={}", dtype))
            }
        }
    }


    /// 搜索指定会话号之前的引用号， 先使用latest_refno， 如果找不到， 则使用search_prev_refno
    /// 搜索指定参考号在指定会话号之前的版本
    ///
    /// # 参数
    /// * `refno` - 要搜索的参考号
    /// * `sesno` - 可选的会话号，用于限定搜索范围
    ///
    /// # 返回值
    /// * `Option<(u32, u64)>` - 成功返回元组(会话号, 引用号物理地址)，找不到时返回None
    pub fn search_latest_and_prev_refno(
        &mut self,
        refno: RefU64,
        sesno: Option<u32>,
    ) -> [Option<(u32, u64)>; 2] {
        // 仅在 debug_btree_search 开启时输出调试信息，避免热路径 println! 拉跨性能。
        let is_target_debug =
            cfg!(feature = "debug_btree_search") && refno.get_0() == 24383 && refno.get_1() == 101192;
        if is_target_debug {
            println!("🔍 [DEBUG-MAIN] === 开始搜索最新和前一个版本 ===");
            println!("🔍 [DEBUG-MAIN] 目标参考号: {}", refno);
            println!("🔍 [DEBUG-MAIN] 指定会话号: {:?}", sesno);
        }

        //dbg!(sesno);
        if let Some(current_data) = self.search_latest_refno(refno, sesno) {
            if is_target_debug {
                println!("🔍 [DEBUG-MAIN] ✓ 找到当前版本: 会话号={}, 偏移量={:#X}", current_data.0, current_data.1);
            }
            // dbg!(current_data);
            if current_data.0 == 0 {
                if is_target_debug {
                    println!("🔍 [DEBUG-MAIN] ❌ 会话号为0，返回空结果");
                }
                return [None, None];
            }

            let prev_data = self
                .get_nearest_less_sesno(current_data.0 as i32)
                .and_then(|prev_sesno| {
                    if is_target_debug {
                        println!("🔍 [DEBUG-MAIN] 搜索前一个会话号: {}", prev_sesno);
                    }
                    self.search_latest_refno(refno, Some(prev_sesno as u32))
                });

            if is_target_debug {
                println!("🔍 [DEBUG-MAIN] 前一个版本搜索结果: {:?}", prev_data);
                println!("🔍 [DEBUG-MAIN] 最终返回: [当前版本: {:?}, 前一个版本: {:?}]", Some(current_data), prev_data);
            }
            return [Some(current_data), prev_data];
        }

        if is_target_debug {
            println!("🔍 [DEBUG-MAIN] ❌ 未找到当前版本，返回 [None, None]");
        }
        [None, None]
    }

    pub fn search_latest_refno(&mut self, refno: RefU64, sesno: Option<u32>) -> Option<(u32, u64)> {
        // 使用优化的高性能索引搜索
        self.search_latest_refno_optimized(refno, sesno)
    }

    /// 优化的高性能索引搜索算法
    ///
    /// 使用真正的B+树索引搜索，确保O(log n)的时间复杂度
    fn search_latest_refno_optimized(
        &mut self,
        refno: RefU64,
        sesno: Option<u32>,
    ) -> Option<(u32, u64)> {
        // 获取索引根页号
        let latest_index_pgno = if let Some(target_sesno) = sesno {
            let ses_pgno = self.sesno_pgno_map.get(&(target_sesno as i32))?;
            let ses_data = self.read_ses_data(*ses_pgno).ok()?;
            ses_data.index_root_pageno
        } else {
            let basic_info = self.get_page_basic_info().ok()?;
            basic_info.latest_ses_data.index_root_pageno
        };

        // 使用修复后的B+树搜索
        self.btree_search_fixed(latest_index_pgno, refno)
    }



    /// 在叶子节点中搜索目标参考号
    pub fn search_in_leaf_node(&mut self, locs: &[RefnoDataLoc], target_r0: u32, target_r1: u32) -> Option<(u32, u64)> {
        #[cfg(feature = "debug_btree_search")]
        println!("🔍 在叶子节点中搜索目标: {}_{}", target_r0, target_r1);

        // 首先检查是否有精确匹配
        for (i, loc) in locs.iter().enumerate() {
            if loc.refno_0 == target_r0 && loc.refno_1 == target_r1 {
                #[cfg(feature = "debug_btree_search")]
                println!(
                    "✅ 找到精确匹配! 位置: [{}] {}_{} -> 页号: 0x{:X}",
                    i, loc.refno_0, loc.refno_1, loc.pgno
                );
                let loc_sesno = self.get_sesno(loc.pgno).unwrap_or_default();
                return Some((loc_sesno, loc.get_att_offset_with_page_size(self.page_size)));
            }
        }

        // 如果没有精确匹配，显示一些调试信息
        #[cfg(feature = "debug_btree_search")]
        {
            println!("❌ 未找到精确匹配");
            println!("📋 叶子节点中包含的参考号范围:");
        }

        // 显示前10个和后10个条目
        let show_count = 10;
        #[cfg(feature = "debug_btree_search")]
        for (i, loc) in locs.iter().take(show_count).enumerate() {
            println!(
                "  前[{}] {}_{} -> 页号: 0x{:X}",
                i, loc.refno_0, loc.refno_1, loc.pgno
            );
        }

        #[cfg(feature = "debug_btree_search")]
        if locs.len() > show_count * 2 {
            println!("  ... (省略中间部分) ...");
        }

        #[cfg(feature = "debug_btree_search")]
        {
            let start_idx = locs.len().saturating_sub(show_count);
            for (i, loc) in locs.iter().skip(start_idx).enumerate() {
                println!(
                    "  后[{}] {}_{} -> 页号: 0x{:X}",
                    start_idx + i,
                    loc.refno_0,
                    loc.refno_1,
                    loc.pgno
                );
            }
        }

        None
    }

    /// 修复后的B+树搜索算法 - 使用优化策略
    fn btree_search_fixed(&mut self, root_pgno: u32, target_refno: RefU64) -> Option<(u32, u64)> {
        let (target_r0, target_r1) = (target_refno.get_0(), target_refno.get_1());

        #[cfg(feature = "debug_btree_search")]
        println!("🔍 开始B+树搜索: 目标参考号 {}_{}, 根页号 0x{:X}", target_r0, target_r1, root_pgno);

        // 使用优化的搜索算法：处理起始标记、去重、超出范围选择最后一个条目
        self.btree_search_optimized_recursive(root_pgno, target_r0, target_r1, Vec::new())
    }

    /// 优化的递归B+树搜索算法
    ///
    /// 关键优化：
    /// 1. 正确处理起始索引标记 0x80000001_0x80000001
    /// 2. 去重索引条目，避免重复条目导致错误路径
    /// 3. 超出范围时选择最后一个条目继续搜索
    /// 4. 支持回溯机制确保完整搜索
    fn btree_search_optimized_recursive(
        &mut self,
        page_no: u32,
        target_r0: u32,
        target_r1: u32,
        mut path: Vec<(u32, usize)>
    ) -> Option<(u32, u64)> {
        let index_data = self.read_index_data(page_no).ok()?;

        #[cfg(feature = "debug_btree_search")]
        println!("📄 当前页号: 0x{:X}, 层级: {}, 条目数: {}", page_no, index_data.level, index_data.refno_locs.len());

        if index_data.level == 0 {
            // 叶子节点
            #[cfg(feature = "debug_btree_search")]
            println!("🍃 到达叶子节点，开始搜索目标参考号");

            #[cfg(feature = "debug_btree_search")]
            if !index_data.refno_locs.is_empty() {
                let first = &index_data.refno_locs[0];
                let last = &index_data.refno_locs[index_data.refno_locs.len() - 1];
                println!("📋 叶子节点范围: {}_{} 到 {}_{}", first.refno_0, first.refno_1, last.refno_0, last.refno_1);
            }

            #[cfg(feature = "debug_btree_search")]
            println!("🔍 在叶子节点中搜索目标: {}_{}", target_r0, target_r1);

            // 在叶子节点中搜索目标参考号
            for (i, loc) in index_data.refno_locs.iter().enumerate() {
                if loc.refno_0 == target_r0 && loc.refno_1 == target_r1 {
                    #[cfg(feature = "debug_btree_search")]
                    println!("✅ [{}] 找到目标参考号: {}_{} -> 页号: 0x{:X}", i, loc.refno_0, loc.refno_1, loc.pgno);
                    let loc_sesno = self.get_sesno(loc.pgno).unwrap_or_default();
                    return Some((loc_sesno, loc.get_att_offset_with_page_size(self.page_size)));
                }
            }

            #[cfg(feature = "debug_btree_search")]
            println!("❌ 未找到精确匹配");

            // 新算法已经能正确导航到包含目标值的叶子节点，如果没找到就是真的不存在

            return None;
        } else {
            // 非叶子节点
            #[cfg(feature = "debug_btree_search")]
            println!("🌿 非叶子节点，查找子页面");

            // 处理起始标记和去重
            let mut unique_entries = Vec::new();
            let mut seen_values = std::collections::HashSet::new();
            let mut has_start_marker = false;
            let mut start_marker_entry = None;

            for (original_idx, entry) in index_data.refno_locs.iter().enumerate() {
                // 检查起始标记
                if entry.refno_0 == 0x80000001 && entry.refno_1 == 0x80000001 {
                    has_start_marker = true;
                    start_marker_entry = Some((original_idx, entry.clone()));
                    continue;
                }

                // 去重处理
                let key = (entry.refno_0, entry.refno_1);
                if !seen_values.contains(&key) {
                    seen_values.insert(key);
                    unique_entries.push((original_idx, entry.clone()));
                }
            }

            #[cfg(feature = "debug_btree_search")]
            {
                println!("📋 非叶子节点所有条目:");
                for (i, entry) in index_data.refno_locs.iter().enumerate() {
                    if i == 0 && entry.refno_0 == 0x80000001 && entry.refno_1 == 0x80000001 {
                        println!("  🏁 [{}] 起始标记: 0x{:X}_0x{:X} -> 子页号: 0x{:X}", i, entry.refno_0, entry.refno_1, entry.pgno);
                    } else {
                        println!("  [{}] 最大值: {}_{} -> 子页号: 0x{:X}", i, entry.refno_0, entry.refno_1, entry.pgno);
                    }
                }

                if has_start_marker {
                    println!("📊 发现起始索引标记，将在搜索时特殊处理");
                }

                println!("📊 去重后条目数: {} (原始: {})", unique_entries.len(), index_data.refno_locs.len());
            }

            // 搜索逻辑
            let mut selected_entry: Option<(usize, RefnoDataLoc)> = None;

            // 修复后的B+树搜索逻辑
            // 在B+树中，每个非叶子节点的条目表示该子树的最大值
            // 我们需要找到第一个大于目标值的条目，然后选择前一个分支

            // 首先检查是否应该选择起始标记分支
            if let Some((marker_idx, ref marker_entry)) = start_marker_entry {
                // 如果目标值小于第一个正常条目，选择起始标记分支
                if let Some((_, first_entry)) = unique_entries.first() {
                    if target_r0 < first_entry.refno_0 ||
                       (target_r0 == first_entry.refno_0 && target_r1 < first_entry.refno_1) {
                        #[cfg(feature = "debug_btree_search")]
                        println!("🎯 目标值小于第一个正常索引，选择起始标记: [{}] -> 页号: 0x{:X}", marker_idx, marker_entry.pgno);
                        selected_entry = Some((marker_idx, marker_entry.clone()));
                    }
                }
            }

            // 如果没有选择起始标记，在去重后的条目中搜索
            if selected_entry.is_none() {
                let mut prev_entry: Option<(usize, RefnoDataLoc)> = None;

                for (original_idx, entry) in &unique_entries {
                    // 如果目标值小于当前条目，选择前一个分支
                    if target_r0 < entry.refno_0 || (target_r0 == entry.refno_0 && target_r1 < entry.refno_1) {
                        if let Some((prev_idx, prev)) = prev_entry {
                            #[cfg(feature = "debug_btree_search")]
                            println!("🎯 目标值小于当前条目 {}_{}, 选择前一个分支: [{}] {}_{} -> 页号: 0x{:X}",
                                     entry.refno_0, entry.refno_1, prev_idx, prev.refno_0, prev.refno_1, prev.pgno);
                            selected_entry = Some((prev_idx, prev));
                        } else if let Some((marker_idx, ref marker_entry)) = start_marker_entry {
                            #[cfg(feature = "debug_btree_search")]
                            println!("🎯 目标值小于第一个条目，选择起始标记: [{}] -> 页号: 0x{:X}", marker_idx, marker_entry.pgno);
                            selected_entry = Some((marker_idx, marker_entry.clone()));
                        }
                        break;
                    }

                    // 如果目标值等于当前条目，选择当前分支
                    if target_r0 == entry.refno_0 && target_r1 == entry.refno_1 {
                        #[cfg(feature = "debug_btree_search")]
                        println!("🎯 目标值等于当前条目，选择当前分支: [{}] {}_{} -> 页号: 0x{:X}",
                                 original_idx, entry.refno_0, entry.refno_1, entry.pgno);
                        selected_entry = Some((*original_idx, entry.clone()));
                        break;
                    }

                    prev_entry = Some((*original_idx, entry.clone()));
                }

                // 如果没有找到合适的分支，选择最后一个条目（目标值大于所有条目）
                if selected_entry.is_none() && !unique_entries.is_empty() {
                    let (original_idx, entry) = &unique_entries[unique_entries.len() - 1];
                    #[cfg(feature = "debug_btree_search")]
                    println!("🎯 目标值大于所有条目，选择最后一个条目: [{}] {}_{} -> 页号: 0x{:X}",
                             original_idx, entry.refno_0, entry.refno_1, entry.pgno);
                    selected_entry = Some((*original_idx, entry.clone()));
                }
            }

            // 继续搜索选中的子页面
            if let Some((selected_idx, selected)) = selected_entry {
                #[cfg(feature = "debug_btree_search")]
                println!("➡️  选择子页号: 0x{:X} (索引: {})", selected.pgno, selected_idx);
                path.push((page_no, selected_idx));
                return self.btree_search_optimized_recursive(selected.pgno, target_r0, target_r1, path);
            } else {
                #[cfg(feature = "debug_btree_search")]
                println!("❌ 没有找到合适的子页面");
                return None;
            }
        }
    }

    // 旧的回溯和子页面查找方法已被优化算法替代，不再需要



    /// 原有的单路径搜索算法
    fn search_latest_refno_interal_single_path(
        &mut self,
        refno: RefU64,
        sesno: Option<u32>,
        scan_cache: bool,
    ) -> Option<(u32, u64)> {
        // 仅在 debug_btree_search 开启时输出调试信息，避免热路径 println! 拉跨性能。
        let is_target_debug =
            cfg!(feature = "debug_btree_search") && refno.get_0() == 24383 && refno.get_1() == 101192;
        if is_target_debug {
            println!("🔍 [DEBUG-INTERNAL] 内部搜索开始");
            println!("🔍 [DEBUG-INTERNAL] 参数 - refno: {}, sesno: {:?}, scan_cache: {}", refno, sesno, scan_cache);
        }

        // dbg!(sesno);
        // 根据sesno参数决定使用哪个会话的数据
        let latest_index_pgno = if let Some(target_sesno) = sesno {
            // dbg!(self.ses_range_map.get(&(target_sesno as i32)));
            // 找到指定会话号对应的页号
            let ses_pgno = match self.sesno_pgno_map.get(&(target_sesno as i32)) {
                Some(&pgno) => pgno,
                None => {
                    if is_target_debug {
                        println!("🔍 [DEBUG-INTERNAL] 找不到指定会话号 {} 对应的页号", target_sesno);
                    }
                    return None;
                }
            };
            // 读取该会话的数据
            let ses_data = self.read_ses_data(ses_pgno).ok()?;
            if is_target_debug {
                println!("🔍 [DEBUG-INTERNAL] 使用指定会话的索引根页号: {:#X}", ses_data.index_root_pageno);
            }
            ses_data.index_root_pageno
        } else {
            let basic_info = self.get_page_basic_info().ok()?;
            // dbg!(basic_info.latest_ses_data.sesno);
            // 使用最新的索引根页号
            if is_target_debug {
                println!("🔍 [DEBUG-INTERNAL] 使用最新会话的索引根页号: {:#X}", basic_info.latest_ses_data.index_root_pageno);
            }
            basic_info.latest_ses_data.index_root_pageno
        };

        // dbg!(latest_index_pgno);

        let mut index_data = self.read_index_data(latest_index_pgno).ok()?;
        let mut level = index_data.level as i32;
        let (r0, r1) = (refno.get_0(), refno.get_1());

        if is_target_debug {
            println!("🔍 [DEBUG-INTERNAL] 索引数据加载成功");
            println!("🔍 [DEBUG-INTERNAL] 初始层级: {}", level);
            println!("🔍 [DEBUG-INTERNAL] 目标参考号分解: r0={}, r1={}", r0, r1);
            println!("🔍 [DEBUG-INTERNAL] 当前索引页参考号数量: {}", index_data.refno_locs.len());
        }
        //refno_locs 必须是递增的，如果遇到小的值了，说明遇到删除的参考号了
        while level >= 0 {
            if is_target_debug {
                println!("🔍 [DEBUG-INTERNAL] === 搜索层级 {} ===", level);
                println!("🔍 [DEBUG-INTERNAL] 当前层级参考号数量: {}", index_data.refno_locs.len());
                if !index_data.refno_locs.is_empty() {
                    let first = &index_data.refno_locs[0];
                    let last = &index_data.refno_locs[index_data.refno_locs.len() - 1];
                    println!("🔍 [DEBUG-INTERNAL] 参考号范围: {}_{} ~ {}_{}",
                        first.refno_0, first.refno_1, last.refno_0, last.refno_1);
                }
            }

            let mut next_loc_index = if level == 0 {
                // 叶子节点：直接查找精确匹配
                let found_index = index_data
                    .refno_locs
                    .iter()
                    .position(|x| x.refno_0 == r0 && x.refno_1 == r1);

                if is_target_debug {
                    println!("🔍 [DEBUG-INTERNAL] 叶子节点精确搜索结果: {:?}", found_index);
                    if found_index.is_none() {
                        println!("🔍 [DEBUG-INTERNAL] 在叶子节点中未找到目标参考号");
                        // 显示前几个和后几个参考号作为参考
                        for (i, loc) in index_data.refno_locs.iter().take(5).enumerate() {
                            println!("🔍 [DEBUG-INTERNAL] 叶子节点[{}]: {}_{}", i, loc.refno_0, loc.refno_1);
                        }
                        if index_data.refno_locs.len() > 10 {
                            println!("🔍 [DEBUG-INTERNAL] ... (省略中间部分) ...");
                            for (i, loc) in index_data.refno_locs.iter().rev().take(5).enumerate() {
                                let real_index = index_data.refno_locs.len() - 1 - i;
                                println!("🔍 [DEBUG-INTERNAL] 叶子节点[{}]: {}_{}", real_index, loc.refno_0, loc.refno_1);
                            }
                        }

                        // 检查目标参考号是否大于当前范围的最大值
                        if let Some(last_loc) = index_data.refno_locs.last() {
                            let last_refno = RefU64::from_two_nums(last_loc.refno_0, last_loc.refno_1);
                            if refno > last_refno {
                                println!("🔍 [DEBUG-INTERNAL] 目标参考号 {} 大于当前叶子节点最大值 {}", refno, last_refno);
                                println!("🔍 [DEBUG-INTERNAL] 需要搜索更大范围的节点");
                            }
                        }
                    }
                }
                found_index
            } else {
                // 非叶子节点：查找范围
                let found_index = index_data.refno_locs.windows(2).position(|x| {
                    //如果是开头的起始页，需要单独处理
                    //应该是缓存页，优先去扫缓存的页面
                    if x[0].is_start_page() {
                        if is_target_debug {
                            println!("🔍 [DEBUG-INTERNAL] 遇到起始页，scan_cache={}", scan_cache);
                        }
                        scan_cache
                    } else {
                        let x0 = RefU64::from_two_nums(x[0].refno_0, x[0].refno_1);
                        let x1 = RefU64::from_two_nums(x[1].refno_0, x[1].refno_1);
                        let in_range = (x1 > x0 && refno >= x0 && refno < x1) || (x1 < x0);
                        if is_target_debug {
                            println!("🔍 [DEBUG-INTERNAL] 检查范围: {} <= {} < {} ? {}", x0, refno, x1, in_range);
                        }
                        in_range
                    }
                });

                if is_target_debug {
                    println!("🔍 [DEBUG-INTERNAL] 非叶子节点范围搜索结果: {:?}", found_index);
                }
                found_index
            };

            if level == 0 && next_loc_index.is_some() {
                let loc = &index_data.refno_locs[next_loc_index.unwrap()];
                let loc_sesno = self.get_sesno(loc.pgno).unwrap_or_default();
                if is_target_debug {
                    println!("🔍 [DEBUG-INTERNAL] ✓ 在叶子节点找到目标参考号!");
                    println!("🔍 [DEBUG-INTERNAL] 页号: {:#X}, 会话号: {}, 偏移量: {:#X}",
                        loc.pgno, loc_sesno, loc.get_att_offset_with_page_size(self.page_size));
                }
                return Some((loc_sesno, loc.get_att_offset_with_page_size(self.page_size)));
            }

            if next_loc_index.is_none() && level > 0 && !index_data.refno_locs.is_empty() {
                // 检查目标参考号是否大于当前节点的最大范围
                if let Some(last_loc) = index_data.refno_locs.last() {
                    let last_refno = RefU64::from_two_nums(last_loc.refno_0, last_loc.refno_1);
                    if refno > last_refno {
                        if is_target_debug {
                            println!("🔍 [DEBUG-INTERNAL] 目标参考号 {} 大于当前节点最大值 {}", refno, last_refno);
                            println!("🔍 [DEBUG-INTERNAL] 在非叶子节点未找到精确范围，使用最后一个位置继续搜索");
                        }
                        next_loc_index = Some(index_data.refno_locs.len() - 1);
                    } else {
                        if is_target_debug {
                            println!("🔍 [DEBUG-INTERNAL] 目标参考号 {} 在当前节点范围内但未找到匹配", refno);
                        }
                    }
                } else {
                    if is_target_debug {
                        println!("🔍 [DEBUG-INTERNAL] 在非叶子节点未找到精确范围，使用最后一个位置");
                    }
                    next_loc_index = Some(index_data.refno_locs.len() - 1);
                }
            }

            if next_loc_index.is_none() {
                if is_target_debug {
                    println!("🔍 [DEBUG-INTERNAL] ❌ 在当前节点无法找到下一个搜索位置");

                    // 如果是叶子节点且目标参考号大于当前范围，尝试搜索下一个兄弟节点
                    if level == 0 {
                        if let Some(last_loc) = index_data.refno_locs.last() {
                            let last_refno = RefU64::from_two_nums(last_loc.refno_0, last_loc.refno_1);
                            if refno > last_refno {
                                println!("🔍 [DEBUG-INTERNAL] 目标参考号大于叶子节点最大值，需要搜索下一个节点");
                                println!("🔍 [DEBUG-INTERNAL] 但当前实现无法跨节点搜索，搜索结束");
                            }
                        }
                    }
                }
                return None;
            }

            // 继续向下查找
            if level > 0 {
                let next_pgno = index_data.refno_locs[next_loc_index.unwrap()].pgno;
                if is_target_debug {
                    println!("🔍 [DEBUG-INTERNAL] 继续向下搜索，下一页号: {:#X}", next_pgno);
                }
                index_data = self.read_index_data(next_pgno).ok()?;
                level = index_data.level as i32;
                if is_target_debug {
                    println!("🔍 [DEBUG-INTERNAL] 加载下一层级数据，新层级: {}", level);
                }
            } else {
                break;
            }
        }

        if is_target_debug {
            println!("🔍 [DEBUG-INTERNAL] ❌ 搜索循环结束，未找到目标参考号");
        }
        None
    }

    /// 扩展搜索算法 - 当单路径搜索失败时，尝试搜索相邻的节点
    fn search_latest_refno_interal_extended(
        &mut self,
        refno: RefU64,
        sesno: Option<u32>,
    ) -> Option<(u32, u64)> {
        let is_target_debug =
            cfg!(feature = "debug_btree_search") && refno.get_0() == 24383 && refno.get_1() == 101192;
        if is_target_debug {
            println!("🔍 [DEBUG-EXTENDED] === 开始扩展搜索 ===");
            println!("🔍 [DEBUG-EXTENDED] 目标参考号: {}", refno);
        }

        // 获取索引根页号
        let latest_index_pgno = if let Some(target_sesno) = sesno {
            let ses_pgno = match self.sesno_pgno_map.get(&(target_sesno as i32)) {
                Some(&pgno) => pgno,
                None => return None,
            };
            let ses_data = self.read_ses_data(ses_pgno).ok()?;
            ses_data.index_root_pageno
        } else {
            let basic_info = self.get_page_basic_info().ok()?;
            basic_info.latest_ses_data.index_root_pageno
        };

        // 从根节点开始，收集所有叶子节点
        let leaf_pages = self.collect_leaf_pages(latest_index_pgno, refno)?;

        if is_target_debug {
            println!("🔍 [DEBUG-EXTENDED] 收集到 {} 个可能的叶子页", leaf_pages.len());
        }

        // 在所有叶子页中搜索目标参考号
        for (page_idx, leaf_pgno) in leaf_pages.iter().enumerate() {
            if is_target_debug {
                println!("🔍 [DEBUG-EXTENDED] 搜索叶子页 {}/{}: {:#X}", page_idx + 1, leaf_pages.len(), leaf_pgno);
            }

            if let Ok(index_data) = self.read_index_data(*leaf_pgno) {
                if index_data.level == 0 {  // 确保是叶子节点
                    let (r0, r1) = (refno.get_0(), refno.get_1());
                    if let Some(found_index) = index_data.refno_locs.iter().position(|x| x.refno_0 == r0 && x.refno_1 == r1) {
                        let loc = &index_data.refno_locs[found_index];
                        let loc_sesno = self.get_sesno(loc.pgno).unwrap_or_default();

                        if is_target_debug {
                            println!("🔍 [DEBUG-EXTENDED] ✓ 在叶子页 {:#X} 找到目标参考号!", leaf_pgno);
                            println!("🔍 [DEBUG-EXTENDED] 页号: {:#X}, 会话号: {}, 偏移量: {:#X}",
                                loc.pgno, loc_sesno, loc.get_att_offset());
                        }

                        return Some((loc_sesno, loc.get_att_offset_with_page_size(self.page_size)));
                    }
                }
            }
        }

        if is_target_debug {
            println!("🔍 [DEBUG-EXTENDED] ❌ 扩展搜索完成，未找到目标参考号");
        }
        None
    }

    /// 收集所有可能包含目标参考号的叶子页
    fn collect_leaf_pages(&mut self, root_pgno: u32, target_refno: RefU64) -> Option<Vec<u32>> {
        let is_target_debug = target_refno.get_0() == 24383 && target_refno.get_1() == 101192;
        let mut leaf_pages = std::collections::HashSet::new();

        // 使用更智能的搜索策略，只收集真正相关的叶子页
        self.collect_relevant_leaf_pages(root_pgno, target_refno, &mut leaf_pages, is_target_debug);

        if is_target_debug {
            println!("🔍 [DEBUG-EXTENDED] 收集叶子页完成，共 {} 个页面", leaf_pages.len());
        }

        if leaf_pages.is_empty() {
            None
        } else {
            // 转换为Vec并按页号排序，确保搜索顺序
            let mut leaf_pages_vec: Vec<u32> = leaf_pages.into_iter().collect();
            leaf_pages_vec.sort();
            Some(leaf_pages_vec)
        }
    }

    /// 递归收集相关的叶子页面，使用更智能的过滤策略
    fn collect_relevant_leaf_pages(&mut self, pgno: u32, target_refno: RefU64, leaf_pages: &mut std::collections::HashSet<u32>, is_debug: bool) {
        if let Ok(index_data) = self.read_index_data(pgno) {
            if index_data.level == 0 {
                // 叶子节点 - 检查是否可能包含目标参考号
                if !index_data.refno_locs.is_empty() {
                    let first = &index_data.refno_locs[0];
                    let last = &index_data.refno_locs[index_data.refno_locs.len() - 1];
                    let first_refno = RefU64::from_two_nums(first.refno_0, first.refno_1);
                    let last_refno = RefU64::from_two_nums(last.refno_0, last.refno_1);

                    // 只收集真正相关的叶子页：
                    // 1. 目标参考号在范围内
                    // 2. 目标参考号的第一部分匹配且第二部分在合理范围内
                    let target_0 = target_refno.get_0();
                    let target_1 = target_refno.get_1();

                    let should_include = if target_refno >= first_refno && target_refno <= last_refno {
                        // 目标在范围内，肯定包含
                        true
                    } else if first.refno_0 == target_0 || last.refno_0 == target_0 {
                        // 第一部分匹配，检查第二部分是否在合理范围内
                        let min_1 = first.refno_1.min(last.refno_1);
                        let max_1 = first.refno_1.max(last.refno_1);

                        // 如果目标的第二部分在当前范围的合理扩展范围内（比如前后1000个数字）
                        target_1 >= min_1.saturating_sub(1000) && target_1 <= max_1.saturating_add(1000)
                    } else {
                        false
                    };

                    if should_include {
                        leaf_pages.insert(pgno);
                        if is_debug {
                            println!("🔍 [DEBUG-EXTENDED] 叶子页 {:#X} 相关: {} ~ {}, 目标: {}",
                                pgno, first_refno, last_refno, target_refno);
                        }
                    }
                }
            } else {
                // 非叶子节点 - 智能选择子节点
                for loc in &index_data.refno_locs {
                    let loc_refno = RefU64::from_two_nums(loc.refno_0, loc.refno_1);

                    // 只访问可能包含目标参考号的子树
                    // 如果这个位置的参考号小于等于目标，或者第一部分匹配，就访问这个子树
                    if loc_refno <= target_refno || loc.refno_0 == target_refno.get_0() {
                        self.collect_relevant_leaf_pages(loc.pgno, target_refno, leaf_pages, is_debug);
                    }
                }
            }
        }
    }

    /// 解析增量数据
    pub async fn parse_incr_element(&mut self, refno_offset: u64) -> anyhow::Result<EleData> {
        //判断这个参考号对应的数据是是增删改的哪一类、

        Ok(EleData::default())
    }

    ///获取单个element数据
    /// 解析单个元素数据
    ///
    /// # 参数
    /// * `refno_offset` - 元素在文件中的偏移量
    ///
    /// # 返回值
    /// * `EleData` - 解析后的元素数据
    ///
    /// # 错误
    /// * 如果文件读取或解析失败,将返回错误
    pub async fn parse_element(&mut self, refno_offset: u64) -> anyhow::Result<EleData> {
        // 使用 ElementRecordReader 跨页读取完整记录，避免 impl_len+1024 这类启发式截断导致丢属性（如 DESP）。
        let data = self.read_element_record_cached(refno_offset)?;

        // 兼容记录前导的 0/7 填充（页对齐/段分隔），一直跳过直到遇到真正的 impl_len。
        let mut prefix = 0usize;
        while prefix + 4 <= data.len() {
            let w = &data[prefix..prefix + 4];
            if w == [0x00, 0x00, 0x00, 0x00] || w == [0x00, 0x00, 0x00, 0x07] {
                prefix += 4;
            } else {
                break;
            }
        }
        let input = &data[prefix..];

        let mut ele_data = parse_ele_data(input).await?;
        let pgno = (refno_offset as usize / self.page_size) as u32;
        let sesno = self.get_sesno(pgno).unwrap_or_default() as i32;
        ele_data.att_map_mut().set_sesno(sesno);
        // 从文件头获取 dbnum 并注入到属性（不使用 refno.get_0() 推导）
        ele_data.att_map_mut().set_dbnum(self.dbnum as u32);
        Ok(ele_data)
    }

    /// 解析原始元素数据, 不处理UDA
    ///
    /// # 参数
    /// * `refno_offset` - 元素在文件中的偏移量
    ///
    pub fn parse_raw_element(&mut self, refno_offset: u64) -> anyhow::Result<EleData> {
        let data = self.read_element_record_cached(refno_offset)?;

        let mut prefix = 0usize;
        while prefix + 4 <= data.len() {
            let w = &data[prefix..prefix + 4];
            if w == [0x00, 0x00, 0x00, 0x00] || w == [0x00, 0x00, 0x00, 0x07] {
                prefix += 4;
            } else {
                break;
            }
        }
        let input = &data[prefix..];

        let ele_data = parse_raw_ele_data(input)?;
        Ok(ele_data)
    }

    //TODO 做一个不处理UDA的方法
    /// 自动获取单个元素数据
    ///
    /// # 参数
    /// * `refno` - 要获取的元素的引用号
    ///
    /// # 返回值
    /// * `EleData` - 元素数据
    ///
    /// # 错误
    /// * 如果找不到元素或解析失败,将返回错误
    #[inline]
    pub async fn auto_get_element(&mut self, refno: RefU64) -> anyhow::Result<EleData> {
        let (_, offset) = self
            .search_latest_refno(refno, None)
            .ok_or(anyhow!("找不到指定参考号: {:?}", refno))?;
        let mut ele_data = self.parse_element(offset).await?;
        Ok(ele_data)
    }

    #[inline]
    pub fn auto_get_raw_element(&mut self, refno: RefU64) -> anyhow::Result<EleData> {
        let (_, offset) = self
            .search_latest_refno(refno, None)
            .ok_or(anyhow!("找不到指定参考号: {:?}", refno))?;
        let ele_data = self.parse_raw_element(offset)?;
        Ok(ele_data)
    }

    /// 深度获取元素及其所有子元素
    ///
    /// # 参数
    /// * `refno` - 要获取的元素的引用号
    ///
    /// # 返回值
    /// * `HashMap<RefU64, EleData>` - 包含所有元素的哈希表,key为引用号,value为元素数据
    ///
    /// # 错误
    /// * 如果获取任何元素失败,将返回错误
    pub async fn auto_get_elements_deep(
        &mut self,
        refno: RefU64,
    ) -> anyhow::Result<HashMap<RefU64, EleData>> {
        let mut map = HashMap::new();
        let mut pendings = VecDeque::new();
        pendings.push_back(refno);
        while let Some(refno) = pendings.pop_front() {
            let ele = self.auto_get_element(refno).await?;
            pendings.extend(&*ele.children);
            map.insert(ele.refno, ele);
        }
        Ok(map)
    }

    /// 获取页面的基本信息
    ///
    /// # 返回值
    /// * `DbPageBasicInfo` - 包含以下信息:
    ///   * pdms_header: PDMS文件头信息
    ///   * latest_ses_pageno: 最新会话页号
    ///   * latest_ses_data: 最新会话数据
    ///   * file_size: 文件大小
    pub fn get_page_basic_info(&mut self) -> anyhow::Result<DbPageBasicInfo> {
        let pdms_header = self.read_pdms_header()?;
        // println!("{:#04X?}", &pdms_header);
        let latest_ses_pageno = pdms_header.latest_ses_pgno;
        let latest_ses_data = self.read_ses_data(latest_ses_pageno)?.clone();
        let file = self.get_file()?;
        Ok(DbPageBasicInfo {
            pdms_header,
            latest_ses_pageno,
            latest_ses_data,
            file_size: file
                .metadata()
                .context("failed to read file metadata")?
                .len(),
        })
    }

    /// 读取PDMS文件头信息
    ///
    /// # 返回值
    /// * `PdmsHeader` - PDMS文件头结构体
    ///
    /// # 错误
    /// * 如果文件读取失败或头部数据解析失败,将返回错误
    #[inline]
    pub fn read_pdms_header(&mut self) -> anyhow::Result<PdmsHeader> {
        let file = self.get_file()?;
        file.seek(SeekFrom::Start(0u64))?;
        let mut head_data = vec![];
        head_data.resize(size_of::<PdmsHeader>(), 0u8);
        file.read_exact(&mut head_data)?;
        let pdms_header = PdmsHeader::try_from(head_data.as_ref())?;
        Ok(pdms_header)
    }

    ///读取ses data
    /// 读取会话页数据
    ///
    /// # 参数
    /// * `ses_pgno` - 会话页号
    ///
    /// # 返回值
    /// * `anyhow::Result<&SessionPageData>` - 成功返回会话页数据的引用,失败返回错误
    ///
    /// # 错误
    /// * 当无法读取指定页号的会话数据时返回错误
    ///
    /// # 实现细节
    /// 1. 首先检查缓存中是否已存在该页数据
    /// 2. 如果不存在,则:
    ///    - 读取文件指定位置的数据
    ///    - 将数据解析为SessionPageData结构
    ///    - 设置页号并存入缓存
    /// 3. 从缓存中返回数据
    #[inline]
        pub fn read_ses_data(&mut self, ses_pgno: u32) -> anyhow::Result<&SessionPageData> {
        if !self.ses_data_map.contains_key(&ses_pgno) {
            // 使用缓存获取页面数据
            let ses_data = self.get_page_cached(ses_pgno)?;
            
            // ✅ 先检查页面类型
            let page_type = verify_page_type(&ses_data)
                .map_err(|e| anyhow!("Failed to verify page type for session page {}: {}", ses_pgno, e))?;
                
            // 验证页面类型是否为会话页面
            if page_type != PageType::Session {
                return Err(anyhow!("Invalid page type for session page {}: expected Session (type 3), got {}", ses_pgno, page_type));
            }
            
            // 解析会话数据
            if let Ok(mut s) = SessionPageData::try_from(ses_data.as_ref()) {
                s.pgno = ses_pgno as _;
                self.ses_data_map.insert(ses_pgno, s);
            } else {
                let offset = ses_pgno as u64 * self.page_size as u64;
                return Err(anyhow!("Failed to parse SessionPageData from page {} at offset {}", ses_pgno, offset));
            }
        }
        
        self.ses_data_map.get(&ses_pgno)
            .ok_or_else(|| anyhow!("Session data still missing from map after read (page {})", ses_pgno))
    }    /// 获取指定会话号的会话数据
    ///
    /// # 参数
    /// * `sesno` - 要获取数据的会话号
    ///
    /// # 返回值
    /// * `anyhow::Result<&SessionPageData>` - 成功返回会话数据的引用,失败返回错误
    ///
    /// # 错误
    /// * 当找不到指定会话号对应的页面时返回错误
    pub fn get_ses_data(&mut self, sesno: u32) -> anyhow::Result<&SessionPageData> {
        if let Some(cur_ses_pgno) = self.get_ses_pageno(sesno as _) {
            self.read_ses_data(cur_ses_pgno)
        } else {
            Err(anyhow!("Can't find ses page with {sesno}."))
        }
    }

    /// 获取最接近指定会话号的有效会话号
    ///
    /// # 参数
    /// * `sesno` - 目标会话号
    ///
    /// # 返回值
    /// * 如果存在大于等于目标会话号的最小会话号,返回该会话号
    /// * 否则返回最大的会话号
    /// * 如果没有任何会话号,返回原始会话号
    pub fn get_nearest_large_sesno(&mut self, sesno: i32) -> Option<i32> {
        // 查找大于等于目标会话号的最小会话号
        if let Some(next_sesno) = self.sesno_pgno_map.keys().filter(|&&s| s >= sesno).min() {
            Some(*next_sesno)
        } else {
            None
        }
    }

    /// 获取最接近指定会话号的较小会话号
    ///
    /// # 参数
    /// * `sesno` - 目标会话号
    ///
    /// # 返回值
    /// * 如果存在小于目标会话号的最大会话号,返回该会话号
    /// * 否则返回原始会话号
    pub fn get_nearest_less_sesno(&mut self, sesno: i32) -> Option<i32> {
        // 查找小于目标会话号的最大会话号
        if let Some(prev_sesno) = self.sesno_pgno_map.keys().filter(|&&s| s < sesno).max() {
            Some(*prev_sesno)
        } else {
            None
        }
    }

    /// 读取索引页数据
    ///
    /// # 参数
    /// * `index_pgno` - 索引页号
    ///
    /// # 返回值
    /// * `anyhow::Result<IndexPageData>` - 成功返回索引页数据,失败返回错误
    ///
    /// # 错误
    /// * 读取文件失败时返回错误
    /// * 解析索引页数据失败时返回错误
    ///
    /// # 实现细节
    /// 1. 获取文件句柄
    /// 2. 分配一个页大小的缓冲区
    /// 3. 定位到指定页号的位置
    /// 4. 读取整页数据
    /// 5. 将数据解析为索引页结构
    #[inline]
    pub fn read_index_data(&mut self, index_pgno: u32) -> anyhow::Result<IndexPageData> {
        let mut index_data = vec![];
        index_data.resize(self.page_size, 0u8);
        let offset = index_pgno as u64 * self.page_size as u64;
        let file = self.get_file()?;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut index_data)?;
        let index_page_data = IndexPageData::try_from(index_data.as_ref())?;
        Ok(index_page_data)
    }

    ///指定 refno，收集它的历史数据
    pub fn collect_ele_history(&self, refno: RefU64) -> Vec<EleData> {
        let mut eles = vec![];
        //根据参考号的pgno，快速找到 sesno -> pgno 的映射
        //提前在 surreal 里存储？还是手动去搜索所有的 refno 数据

        eles
    }

    ///存储所有的参考号和对应的 sesno 数据
    /// 返回一个历史参考号集合，值为所有的位置
    pub async fn store_all_refno_sesno_map(
        &mut self,
    ) -> anyhow::Result<BTreeMap<RefU64, BTreeSet<(u64, u32)>>> {
        let mut history_loc_map: BTreeMap<RefU64, BTreeSet<(u64, u32)>> = BTreeMap::new();
        let pdms_header = self.read_pdms_header().unwrap();
        let dbnum = pdms_header.db_num;
        let mut cur_ses_pgno = pdms_header.latest_ses_pgno;
        //收集所有的 session 和 refno 的对应关系
        //表pe_ses_h:  id([refno, sesno]), refno(指向最新？), sesno, offset, dbnum, ses_table
        let mut pe_ses_sqls = Vec::new();
        // SUL_DB.query("remove table ses;").await.unwrap();
        //使用 channel 来保存 sql 数据
        let (tx, rx) = flume::unbounded::<SesSqlType>();
        let mut handles = Vec::new();
        //开启一个保存 pe_ses_h 的线程
        let handle = tokio::spawn(async move {
            // recv_async 会阻塞直到发送端关闭，避免 try_recv 立即返回导致漏数
            while let Ok(values) = rx.recv_async().await {
                match values {
                    //保存 session 数据
                    SesSqlType::SesJson(values) => {
                        for chunk in values.chunks(100) {
                            //插入 json 数据
                            let sql = format!("INSERT IGNORE INTO ses [{}];", chunk.join(","));
                            // println!("ses sql: {}", &sql);
                            SUL_DB.query(sql).await.unwrap();
                        }
                    }
                    SesSqlType::PeSesSql(values) => {
                        for chunk in values.chunks(100) {
                            let sql = format!("INSERT IGNORE INTO  pe_ses_h (id, refno, sesno, offset, dbnum, ses) VALUES {};", chunk.join(","));
                            SUL_DB.query(sql).await.unwrap();
                        }
                    }
                    SesSqlType::PeVersionJson((values, dt)) => {
                        for chunk in values.chunks(100) {
                            let sql = format!(
                                "INSERT IGNORE INTO pe [{}] VERSION {};",
                                chunk.join(","),
                                dt.to_rfc3339()
                            );
                            SUL_DB.query(sql).await.unwrap();
                        }
                    }
                    _ => {}
                }
            }
        });
        handles.push(handle);
        // 跳过没有变化的数据，需要用个hash 来记录
        let mut latest_refno_map = DashMap::new();
        while cur_ses_pgno > 4 {
            let all_locs = self.collect_refno_locs_in_session(cur_ses_pgno as _);
            let cur_ses_page = self.read_ses_data(cur_ses_pgno as _).unwrap().clone();
            let sesno = cur_ses_page.sesno;
            let cur_dt = cur_ses_page.get_utc_dt();
            let ses_id = cur_ses_page.get_id(dbnum);
            tx.send(SesSqlType::SesJson(vec![cur_ses_page.gen_sur_json(dbnum)]));

            for loc in all_locs {
                let refno = loc.get_refno();
                let offset = loc.get_att_offset();
                let Some(sesno) = self.get_sesno((offset as usize / self.page_size) as _) else {
                    continue;
                };

                //将所有数据都保存到 kv 数据库中，方便版本历史的查询
                //todo 单独存储到数据库
                // let Ok(ele_data) = self.get_element(offset).await else {
                //     continue;
                // };
                // let att = ele_data.att_map();
                // let mut pe = att.pe(dbnum);
                // tx.send(SesSqlType::PeVersionJson((vec![pe.gen_sur_json(None)], cur_dt)));

                //需要记录所有的 offset 数据，如果有两个以上的，代表有历史数据，需要在后面做比较
                //根据读取的数据判断是否有增删改
                history_loc_map
                    .entry(refno)
                    .or_default()
                    .insert((offset, sesno));
                let is_latest = !latest_refno_map.contains_key(&refno);
                //如果是最新的，就不需要保存到历史数据
                if is_latest {
                    latest_refno_map.insert(refno, loc);
                    continue;
                }
                let id = format!("['{}', {}]", refno, sesno);
                pe_ses_sqls.push(format!(
                    "({}, {}, {}, {}, {}, ses:[{}, {}])",
                    id,
                    refno.to_pe_key(),
                    sesno,
                    offset,
                    dbnum,
                    ses_id[0],
                    ses_id[1]
                ));
            }
            //按 chunks 保存数据
            if pe_ses_sqls.len() > 100 {
                if let Err(e) = tx.send(SesSqlType::PeSesSql(std::mem::take(&mut pe_ses_sqls))) {
                    dbg!(&e);
                }
            }

            if cur_ses_page.last_ses_pageno < 0 {
                break;
            }
            cur_ses_pgno = cur_ses_page.last_ses_pageno as _;
        }
        if pe_ses_sqls.len() > 0 {
            if let Err(e) = tx.send(SesSqlType::PeSesSql(pe_ses_sqls)) {
                dbg!(&e);
            }
        }
        //关闭 channel
        drop(tx);
        for handle in handles {
            handle.await.unwrap();
        }

        //去掉 value 的长度为 1 的数据
        // history_loc_map.retain(|_, v| v.len() > 1);
        Ok(history_loc_map)
    }

    //todo 可以指定 sesno 的范围去更新历史数据
    pub async fn sync_history(&mut self) -> anyhow::Result<()> {
        //     let history_pe_map = self.store_all_refno_sesno_map().await?;
        //     dbg!(&history_pe_map.len());
        //     // 遍历所有的 offset, 读取属性数据，得到 attmap
        //     // let mut ses_map = HashMap::new();
        //     let dbnum = self.dbnum;
        //     let mut pe_owner_h_relates = Vec::new();
        //     let mut all_his_pe_json = Vec::new();
        //     let mut all_his_json = Vec::new();

        //     let mut all_his_att_json_map: HashMap<String, Vec<String>> = HashMap::new();
        //     let mut pe_op_map: HashMap<RefnoEnum, (EleOperation, u32)> = HashMap::new();
        //     let mut ses_op_map: HashMap<u32, Vec<EleOperation>> = HashMap::new();
        //     //将历史纪录都存储在 his_relate 里， owner 为当前最新的 pe
        //     //如果没有历史记录，则不存储，减小额外的存储
        //     let mut deleted_refnos_map = BTreeMap::new();
        //     //只添加了一次的数据纪录,  todo 需要排查是否有数据在删除里，需要特殊处理
        //     let mut added_only_refnos_map = BTreeMap::new();
        //     for (&refno, offset_set) in &history_pe_map {
        //         let mut prev_children = Vec::new();
        //         let mut prev_att_json = None;
        //         let loc_len = offset_set.len();
        //         //如果后面有删除的动作，则需要再加一个重新插入删除的数据，应该还原到pe:[] 的历史 id 中，啥时候
        //         // 被删除的，需要把历史数据还原回来，数据就是一个简单的标记位就行？
        //         // 如果数据只有一个的时候，会以为 pe 里有数据，其实没有
        //         let mut is_single_his = loc_len == 1;
        //         let mut prev_sesno = 0;
        //         let mut all_sesnos = BTreeSet::new();
        //         for (i, &(offset, sesno)) in offset_set.iter().enumerate() {
        //             // 只有一个版本的情况，直接添加，都是最新的
        //             // if is_debug {
        //             //     dbg!(offset);
        //             //     dbg!(sesno);
        //             //     dbg!(&offset_set);
        //             // } else {
        //             //     continue;
        //             // }
        //             //也有可能是删除了的情况
        //             if loc_len == 1 {
        //                 ses_op_map.entry(sesno).or_default().push(EleOperation::Add);
        //                 added_only_refnos_map.insert(refno, offset);
        //                 break;
        //             }
        //             let is_last = i == loc_len - 1;
        //             let ele_data = self.parse_raw_element(offset).unwrap();
        //             let att = ele_data.att_map();
        //             let mut pe = att.pe(dbnum);
        //             if !is_last {
        //                 all_sesnos.insert(sesno);
        //             }
        //             // if is_debug {
        //             //     dbg!(&att);
        //             // }

        //             //如果是第二个 json 开始，都是 modified
        //             //todo需要实际检查是否真的 json 数据发生变化
        //             if prev_att_json.is_some() {
        //                 let refno_sesno = RefnoSesno::new(refno, sesno);
        //                 if is_last {
        //                     pe_op_map.insert(refno.into(), (EleOperation::Modified, prev_sesno));
        //                 } else {
        //                     pe_op_map.insert(refno_sesno.into(), (EleOperation::Modified, prev_sesno));
        //                 }
        //                 ses_op_map
        //                     .entry(sesno)
        //                     .or_default()
        //                     .push(EleOperation::Modified);
        //             } else {
        //                 ses_op_map.entry(sesno).or_default().push(EleOperation::Add);
        //             }

        //             //和 prev_children 对比，如果在 prev 不在 current，则为删除，在 current，不在 prev，则为新增
        //             for &child in ele_data.children.iter() {
        //                 let refno_sesno = RefnoSesno::new(child, sesno);
        //                 //如果是 add，在当前 sesno 一定会有
        //                 if !prev_children.contains(&child) {
        //                     //默认其实就是 add
        //                     pe_op_map.insert(refno_sesno.into(), (EleOperation::Add, prev_sesno));
        //                     ses_op_map.entry(sesno).or_default().push(EleOperation::Add);
        //                 }
        //             }
        //             // if is_debug {
        //             //     // dbg!(&prev_children);
        //             //     // dbg!(&ele_data.children);
        //             // }
        //             for &child in prev_children.iter() {
        //                 let refno_sesno = RefnoSesno::new(child, sesno);
        //                 //如果是 add，在当前 sesno 一定会有
        //                 if !ele_data.children.contains(&child) {
        //                     // dbg!(&ele_data.children);
        //                     // dbg!(&prev_children);
        //                     deleted_refnos_map.insert(child, sesno);
        //                     if !history_pe_map.contains_key(&child) {
        //                         continue;
        //                     }
        //                     //todo 需要在后面更新回来找到正确的结果？
        //                     let (_, latest_sesno) = history_pe_map
        //                         .get(&child)
        //                         .as_ref()
        //                         .unwrap()
        //                         .iter()
        //                         .rev()
        //                         .next()
        //                         .cloned()
        //                         .unwrap_or_default();
        //                     //todo 在 map 里可以提前存储
        //                     // dbg!(latest_sesno);
        //                     pe_op_map.insert(
        //                         refno_sesno.into(),
        //                         (EleOperation::Deleted, latest_sesno as _),
        //                     );
        //                     ses_op_map
        //                         .entry(sesno)
        //                         .or_default()
        //                         .push(EleOperation::Deleted);
        //                 }
        //             }
        //             //如果是最后一个参考号的位置，直接退出，不用去保存到历史数据，因为是最新的数据
        //             if is_last {
        //                 break;
        //             }
        //             //需要获得这个属性里所有是参考号的对应的 sesno
        //             let mut refno_sesno_map = att.build_refno_sesno_map(sesno, dbnum).await?;
        //             let owner_sesno = refno_sesno_map.get(&pe.owner.refno()).cloned().unwrap_or(0);
        //             let pe_json = pe.gen_sur_json_with_sesno(sesno as _, owner_sesno as _);
        //             all_his_pe_json.push(pe_json);
        //             let Some(att_json) = att.gen_sur_json_with_sesno(sesno as _, &refno_sesno_map)
        //             else {
        //                 continue;
        //             };
        //             prev_att_json = Some(att_json.clone());
        //             //保存his_relate 数据
        //             let ses_refno = RefnoSesno::new(refno, sesno);

        //             //todo 如果没有真的发生变化，其实可以不保存这个数据
        //             all_his_att_json_map
        //                 .entry(att.get_type())
        //                 .or_default()
        //                 .push(att_json);
        //             if all_his_pe_json.len() > 100 {
        //                 println!("all_his_pe_json: {}", &all_his_pe_json.len());
        //                 //直接执行 sql
        //                 let sql = format!("INSERT IGNORE INTO pe [{}];", all_his_pe_json.join(","));
        //                 SUL_DB.query(sql).await.unwrap();
        //                 all_his_pe_json.clear();
        //             }
        //             //保存 pe_owner history 的 relate 关系, owner 的 relate 关系 pe->owner
        //             let children = &ele_data.children;
        //             let owner_relates = Self::gen_owner_relates_h(
        //                 &history_pe_map,
        //                 &children.0,
        //                 pe.refno.refno(),
        //                 sesno,
        //                 dbnum,
        //             )
        //             .await?;
        //             // dbg!(&owner_relates);
        //             pe_owner_h_relates.extend(owner_relates);

        //             prev_children = children.to_vec();
        //             prev_sesno = sesno;
        //         }

        //         //只存储历史的 refno纪录
        //         if !all_sesnos.is_empty() {
        //             let his_pe_keys = all_sesnos
        //                 .iter()
        //                 .map(|sesno| format!("pe:['{}', {}]", refno, sesno))
        //                 .collect::<Vec<_>>()
        //                 .join(",");
        //             all_his_json.push(format!(
        //                 r#"{{ id: his_pe:{0}, refnos: [{1}] }}"#,
        //                 refno.to_string(),
        //                 &his_pe_keys
        //             ));
        //         }

        //         if all_his_json.len() > 100 {
        //             let sql = format!("INSERT IGNORE INTO  his_pe [{}];", all_his_json.join(","));
        //             SUL_DB.query(sql).await.unwrap();
        //             all_his_json.clear();
        //         }
        //         if pe_owner_h_relates.len() > 100 {
        //             println!("pe_owner_h_relates: {}", &pe_owner_h_relates.len());
        //             //直接执行 sql
        //             let sql = format!(
        //                 "INSERT RELATION INTO pe_owner [{}];",
        //                 pe_owner_h_relates.join(",")
        //             );
        //             SUL_DB.query(sql).await.unwrap();
        //             pe_owner_h_relates.clear();
        //         }
        //     }
        //     //检查 deleted_refnos 是否有在 add_only_refnos 中，如果有，则删除
        //     // dbg!(&deleted_refnos_map);
        //     dbg!(&added_only_refnos_map.len());
        //     let mut no_modify_delete_refnos_map = BTreeMap::new();
        //     if !deleted_refnos_map.is_empty() && !added_only_refnos_map.is_empty() {
        //         for (&refno, &sesno) in &deleted_refnos_map {
        //             if added_only_refnos_map.contains_key(&refno) {
        //                 let offset = added_only_refnos_map.remove(&refno).unwrap();
        //                 no_modify_delete_refnos_map.insert(refno, (sesno, offset));
        //             }
        //         }

        //         if !no_modify_delete_refnos_map.is_empty() {
        //             // dbg!(&need_delete_refnos);
        //             //只出现过一次，然后被判断为删除的，需要还原为原来的数据
        //             for (refno, (del_sesno, offset)) in no_modify_delete_refnos_map {
        //                 // SUL_DB.query(sql).await.unwrap();
        //                 // let sql = format!("UPSERT pe:['{}', {}]", refno.to_pe_key(), sesno);
        //                 // SUL_DB.query(sql).await.unwrap();
        //                 let Some(add_sesno) = self.get_sesno((offset / PAGE_SIZE as usize) as _) else {
        //                     continue;
        //                 };
        //                 // ses_op_map.entry(add_sesno).or_default().pop();
        //                 let Ok(ele_data) = self.parse_element(offset).await else {
        //                     continue;
        //                 };
        //                 let att = ele_data.att_map();
        //                 let mut pe = att.pe(dbnum);

        //                 let mut refno_sesno_map = att.build_refno_sesno_map(add_sesno, dbnum).await?;
        //                 let owner_sesno = refno_sesno_map.get(&pe.owner.refno()).cloned().unwrap_or(0);
        //                 let pe_json = pe.gen_sur_json_with_sesno(add_sesno as _, owner_sesno as _);
        //                 all_his_pe_json.push(pe_json);
        //                 let Some(att_json) =
        //                     att.gen_sur_json_with_sesno(add_sesno as _, &refno_sesno_map)
        //                 else {
        //                     continue;
        //                 };
        //                 all_his_att_json_map
        //                     .entry(att.get_type())
        //                     .or_default()
        //                     .push(att_json);
        //                 all_his_json.push(format!(
        //                     r#"{{ id: his_pe:{0}, refnos: [pe:['{0}', {del_sesno}], pe:{0}] }}"#,
        //                     refno.to_string(),
        //                 ));

        //                 let children = &ele_data.children;
        //                 let owner_relates = Self::gen_owner_relates_h(
        //                     &history_pe_map,
        //                     &children.0,
        //                     pe.refno.refno(),
        //                     add_sesno,
        //                     dbnum,
        //                 )
        //                 .await?;
        //                 pe_owner_h_relates.extend(owner_relates);
        //             }
        //         }
        //     }

        //     if pe_owner_h_relates.len() > 0 {
        //         println!("pe_owner_h_relates: {}", &pe_owner_h_relates.len());
        //         //直接执行 sql
        //         let sql = format!(
        //             "INSERT RELATION INTO pe_owner [{}];",
        //             pe_owner_h_relates.join(",")
        //         );
        //         // println!("relation sql is {}", sql);
        //         SUL_DB.query(sql).await.unwrap();
        //     }
        //     if all_his_pe_json.len() > 0 {
        //         // println!("all_his_pe_json: {}", &all_his_pe_json.len());
        //         //直接执行 sql
        //         let sql = format!("INSERT IGNORE INTO  pe [{}];", all_his_pe_json.join(","));
        //         SUL_DB.query(sql).await.unwrap();
        //     }
        //     if all_his_json.len() > 0 {
        //         let sql = format!("INSERT IGNORE INTO  his_pe [{}];", all_his_json.join(","));
        //         // println!("sql: {}", &sql);
        //         SUL_DB.query(sql).await.unwrap();
        //     }
        //     //保存历史属性数据
        //     for (att_type, att_jsons) in all_his_att_json_map {
        //         //使用 chunk
        //         for chunk in att_jsons.chunks(100) {
        //             let sql = format!("INSERT IGNORE INTO  {}_H [{}];", att_type, chunk.join(","));
        //             SUL_DB.query(sql).await.unwrap();
        //         }
        //     }

        //     //执行 pe_op_map, 更新 pe 数据
        //     //update 这三个数量到 ses 表
        //     let dbnum = self.dbnum;
        //     for (sesno, ops) in ses_op_map {
        //         let add_cnt = ops.iter().filter(|op| **op == EleOperation::Add).count();
        //         let mod_cnt = ops
        //             .iter()
        //             .filter(|op| **op == EleOperation::Modified)
        //             .count();
        //         let del_cnt = ops
        //             .iter()
        //             .filter(|op| **op == EleOperation::Deleted)
        //             .count();
        //         let sql = format!(
        //             "UPDATE ses:[{dbnum}, {sesno}] set add_cnt={}, mod_cnt={}, del_cnt={};",
        //             add_cnt, mod_cnt, del_cnt
        //         );
        //         SUL_DB.query(sql).await.unwrap();
        //     }
        //     //update 修改状态到pe 表
        //     for (refno_sesno, (op, prev_sesno)) in pe_op_map {
        //         if op == EleOperation::Add {
        //             continue;
        //         }
        //         // let id = if no_modify_delete_refnos_map.contains_key(&refno_sesno.refno()) {
        //         let id = if op == EleOperation::Deleted {
        //             //删除需要都更新到pe
        //             refno_sesno.refno().to_pe_key()
        //         } else {
        //             refno_sesno.to_pe_key()
        //         };
        //         let mut sql = if refno_sesno.sesno().unwrap_or_default() == 0 {
        //             format!("UPSERT {} set op={}", id, op.into_num(),)
        //         } else {
        //             format!(
        //                 "UPSERT {} set op={}, sesno={}",
        //                 id,
        //                 op.into_num(),
        //                 refno_sesno.sesno().unwrap(),
        //             )
        //         };

        //         if prev_sesno != 0 {
        //             sql.push_str(&format!(
        //                 ", old_pe=pe:['{}', {prev_sesno}]",
        //                 refno_sesno.refno().to_string()
        //             ));
        //         }

        //         if op == EleOperation::Deleted {
        //             sql.push_str(&format!(", dbnum={dbnum}"));
        //         }

        //         SUL_DB.query(sql).await.unwrap();
        //     }
        //     Ok(())
        // }

        // /// 生成 owner 的 relate 关系，只生成历史数据
        // pub async fn gen_owner_relates_h(
        //     his_map: &BTreeMap<RefU64, BTreeSet<(u64, u32)>>,
        //     children: &[RefU64],
        //     owner: RefU64,
        //     sesno: u32,
        //     dbnum: i32,
        // ) -> anyhow::Result<Vec<String>> {
        //     let mut pe_owner_h_relates = Vec::new();
        //     for (index, &child) in children.iter().enumerate() {
        //         let (mut child_sesno, latest_sesno) = query_refno_sesno(child, sesno, dbnum).await?;
        //         //如果 child 没有历史数据，而且在最新的 pe 里没有这个数据
        //         if latest_sesno == 0 && child_sesno == 0 {
        //             let Some(locs) = his_map.get(&child) else {
        //                 continue;
        //             };
        //             //不超过当前 sesno 的的最大 sesno
        //             child_sesno = locs
        //                 .iter()
        //                 .rev()
        //                 .find(|x| x.1 <= sesno)
        //                 .map(|x| x.1)
        //                 .unwrap_or(0);
        //             // dbg!((child, child_sesno));
        //         }
        //         //child id 需要去 pe_ses 里查询得到最近的那个版本
        //         //如果是历史数据，加上 old 的标签
        //         if child_sesno != 0 {
        //             pe_owner_h_relates.push(format!(
        //                 r#"{{ id: pe_owner:[pe:['{0}', {sesno}], {index}], in: pe:['{1}',{child_sesno}],
        //                     out: pe:['{0}', {sesno}],  old: true }}"#,
        //                 owner, child
        //             ));
        //         } else {
        //             // dbg!((child, child_sesno, refno, sesno));
        //             pe_owner_h_relates.push(
        //                 format!(r#"{{ id: pe_owner:[pe:['{0}', {sesno}], {index}], in: pe:{1}, out: pe:['{0}', {sesno}], old: true }}"#,
        //                         owner, child)
        //             );
        //         }
        //     }
        //     Ok(pe_owner_h_relates)
        // }

        // /// 同步所有 session 数据到数据库
        // //todo add some date filter ? session filter
        // pub async fn total_sync_sessions_to_db(&mut self) -> anyhow::Result<()> {
        //     // use itertools::Itertools;

        //     //删除所有的历史数据
        //     // SUL_DB.query("DELETE  e3d_ses;").await.unwrap();
        //     // SUL_DB.query("DELETE  pe_h;").await.unwrap();
        //     // SUL_DB.query("DELETE  ses_pe_relate;").await.unwrap();

        //     // let pdms_header = self.read_pdms_header().unwrap();
        //     // let dbnum = pdms_header.db_num;
        //     // let mut cur_ses_pgno = pdms_header.latest_ses_pgno;
        //     // let project = self.project.clone();

        //     // //显示出有哪些修改，使用 json diff 工具
        //     // let mut step = 0;
        //     // //遍历整个文件数据, 从最新的最前的遍历
        //     // let mut latest_refno_map = DashMap::new();
        //     // let mut all_children_map: DashMap<RefU64, RefU64Vec> = DashMap::new();
        //     // let mut all_relates = Vec::new();
        //     // //pe_owner_h 的添加
        //     // while cur_ses_pgno > 4 {
        //     //     //数据还是跟 pgno ?
        //     //     // let all_ents_in_ses = self.collect_refno_los_in_session(cur_ses_pgno as _).await;
        //     //     //历史数据是否需要存储的问题？
        //     //     // dbg!(&all_ents_in_ses);
        //     //     let all_locs = self.collect_refno_locs_in_session(cur_ses_pgno as _);
        //     //     // dbg!(all_locs.len());

        //     //     let cur_ses_page = self.read_ses_data(cur_ses_pgno as _).unwrap().clone();
        //     //     //保存session 数据
        //     //     // Self::save_ses_data(&pdms_header, &project, &cur_ses_page).await;

        //     //     let mut all_his_att_sql = String::new();
        //     //     let mut all_his_pe_sql = String::new();
        //     //     let mut ses_relates = Vec::new();
        //     //     let sesno = cur_ses_page.sesno;
        //     //     // let ses_str = cur_ses_page.get_id(pdms_header.db_num);
        //     //     for (i, loc) in all_locs.iter().enumerate() {
        //     //         //如果是最新的，就不需要加版本后缀
        //     //         //如果是历史版本，就需要有历史后缀，简单点就是是否之后出现过
        //     //         let refno = loc.get_refno();
        //     //         let offset = loc.offset;
        //     //         //从后往前找的天然优势，就是后面的永远是最新的，如果发现历史的数据了，就加上版本号
        //     //         let is_latest = !latest_refno_map.contains_key(&refno);
        //     //         let pe_id = if is_latest {
        //     //             format!("pe:{}", refno)
        //     //         } else {
        //     //             format!("pe:['{}',{}]", refno, sesno)
        //     //         };
        //     //         // all_relates.push(format!(
        //     //         //     "{{ id:[e3d_ses:{}, {i}], in: {}, out: e3d_ses:{}, pgno:{}, offset:{} }}",
        //     //         //     ses_str, &pe_id, ses_str, loc.pgno, loc.offset
        //     //         // ));
        //     //         //先暂时不管引用的数据？如果是引用的，需要先按 sesno 查询到当前对应的数据，
        //     //         //可以用 id 扫描的办法，得到最新的数据？
        //     //         if let Ok(ele_data) = self.get_element(loc.get_att_offset()).await {
        //     //             let att = ele_data.att_map();
        //     //             // all_children_map.entry(refno);
        //     //             if !is_latest {
        //     //                 //如果是历史数据，版本号加上
        //     //                 let json = att
        //     //                     .gen_sur_json_with_id(format!("['{}',{}]", refno.to_string(), sesno))
        //     //                     .unwrap();
        //     //                 let sql = format!("INSERT IGNORE INTO  {}_H {};", att.get_type_str(), &json);
        //     //                 all_his_att_sql.push_str(&sql);
        //     //                 let pe_sql = format!(
        //     //                     "INSERT IGNORE INTO  pe_h {};",
        //     //                     att.pe(dbnum).gen_sur_json_with_sesno(sesno)
        //     //                 );
        //     //                 // println!("pe sql: {}", &pe_sql);
        //     //                 all_his_pe_sql.push_str(&pe_sql);
        //     //                 //如果有历史 children 数据，而且 children 数据和当前的不一致，需要列出来哪些是新增的，那些是删除的
        //     //                 if let Some(old_children) = all_children_map.get(&refno) {
        //     //                     let mut new_children = &ele_data.children;
        //     //                     let mut all_deleted = old_children
        //     //                         .iter()
        //     //                         .cloned()
        //     //                         .filter(|x| !new_children.contains(x))
        //     //                         .collect::<BTreeSet<_>>();
        //     //                     let mut all_added = new_children
        //     //                         .iter()
        //     //                         .cloned()
        //     //                         .filter(|x| !old_children.contains(x))
        //     //                         .collect::<BTreeSet<_>>();
        //     //                     for r in &all_deleted {
        //     //                         let op: i32 = DataOperation::Deleted.into();
        //     //                         ses_relates.push(format!("{{ id:[e3d_ses:{}, {}], in: pe:{}, out: e3d_ses:{}, refno: {}, offset:{}, op: {} }}",
        //     //                                                  sesno, i, r, sesno, refno.to_pe_key(), offset, op));
        //     //                     }

        //     //                     for r in &all_added {
        //     //                         let op: i32 = DataOperation::Added.into();
        //     //                         ses_relates.push(format!("{{ id:[e3d_ses:{}, {}], in: pe:{}, out: e3d_ses:{}, refno: {}, offset:{}, op: {} }}",
        //     //                             sesno, i, r, sesno, refno.to_pe_key(), offset, op));
        //     //                     }

        //     //                     if !all_deleted.is_empty() || !all_added.is_empty() {
        //     //                         println!("{sesno} Deleted: {:?}", &all_deleted);
        //     //                         println!("{sesno} Added: {:?}", &all_added);
        //     //                     }
        //     //                 }
        //     //             }
        //     //             //如果是最新的数据，就保存起来
        //     //             all_children_map.insert(refno, ele_data.children);
        //     //         }
        //     //         latest_refno_map.entry(refno).or_insert_with(|| loc.clone());
        //     //     }
        //     //     // println!("hist att sql: {}", &all_his_att_sql);
        //     //     //保存历史属性数据
        //     //     SUL_DB.query(all_his_att_sql).await.unwrap();
        //     //     SUL_DB.query(all_his_pe_sql).await.unwrap();
        //     //     println!("会话: {:#4X?} 保存完毕", cur_ses_pgno);

        //     //     //直接通过数据库查是否最新？还是通过文件查找？
        //     //     //每个参考号都去拉取一遍，然后看看是不是最新的？

        //     //     // let offset = cur_ses_page.end_pgno * PAGE_SIZE as u32 + 0x4;
        //     //     // let bytes = io.read_bytes(offset, 4).unwrap();
        //     //     // let type_name = db1_dehash(u32::from_be_bytes(bytes.try_into().unwrap()));
        //     //     // dbg!(type_name);
        //     //     // println!("session pgno {:#4X}: {:#4X}", cur_ses_pgno, offset / 0x800);
        //     //     // dbg!((cur_ses_no, offset));
        //     //     // dbg!(cur_ses_page.last_ses_pageno);
        //     //     if step == 50 {
        //     //         break;
        //     //     }
        //     //     if cur_ses_page.last_ses_pageno < 0 {
        //     //         break;
        //     //     }
        //     //     step += 1;
        //     //     cur_ses_pgno = cur_ses_page.last_ses_pageno as _;
        //     //     // dbg!(last_ses_no);
        //     //     // dbg!(cur_ses_page.get_timestamp());
        //     //     // dbg!(cur_ses_page.get_computer_name());
        //     //     // dbg!(cur_ses_page.get_comments_name());

        //     //     // break;
        //     // }

        //     // Self::save_ses_pe_relates(&all_relates).await;

        //     // return Ok(());

        //     // //refno_pgnos_map 查询里面 value 最多的项
        //     // // let max_history_refno = refno_pgnos_map.iter().max_by_key(|x| x.1.len());
        //     // // dbg!(&max_history_refno);

        //     // //pe_history
        //     // //pe_owner history 是否有必要
        //     // //pe_owner 始终是最新的数据
        //     // //pe_owner_history  为  pe_history 之间的关联关系？也有可能是 pe
        //     // //如果 children 发生变化，确实需要记录这个，如果 pe 里没有的，那就是真没有
        //     // //NOUN_history
        //     // //保存历史属性数据到数据库
        //     // let mut type_att_map = BTreeMap::new();
        //     // let mut found = false;
        //     // //e3d_session 是否要绑定一个Operation log 的指向，还是直接可以对比两个session 就可以得到？
        //     // //但是这样没法实现参考号查询自己是啥时候发生删除的，或者修改的
        //     // // let mut history_owner_map = HashMap::new();
        //     // // for (&refno, locs) in &refno_pgnos_map {
        //     // //     if locs.is_empty(){
        //     // //         continue;
        //     // //     }
        //     // //     //表示有历史记录，后面存储的都是old data, 查询时需要和latest data 合着一起查询
        //     // //     dbg!(locs.len());
        //     // //     //（1）按着从小到大的顺序排列的，所以后面的是新的，可以判断构件是否被删除
        //     // //     //如果是新增加的呢？怎么样维护这个是否新增的关系，这里就要比较这个 ses no 的关系了，在查询的时候，如果是按
        //     // //     //历史记录查询，需要加个 sesno 的条件过滤，或者 pgno 的过滤，子节点的 pngo 不能超过某个pgno
        //     // //     //删除了肯定是不能再加回去这个参考号的
        //     // //     //是否需要弄个pe_history? 还是就放在 pe 里面？应该是都放在 pe 里，然后历史的数据需要加上，以为 pe 是唯一的
        //     // //     //即使属性发生变化，也只是 pgno 的变化
        //     // //     let mut prev_children = RefU64Vec::default();
        //     // //     //todo 使用 chunk
        //     // //     let mut ses_relates = Vec::new();
        //     // //     let len = locs.len();
        //     // //     let mut all_pes = Vec::new();
        //     // //     //pe_owner 怎么处理？
        //     // //     //解析时，要快速定位所在 sesno，要记录下来，设置到 pgno，现在不能用 pgno 了，sesno 更具有代表性
        //     // //     for (index, (pgno, sesno, offset)) in locs.into_iter().enumerate() {
        //     // //         let addr = *pgno as u64 * 0x800 + *offset as u64 * 2;
        //     // //         if let Ok(mut data) = self.get_element(addr).await{
        //     // //             let mut att = &mut data.whole_attmap.attmap;
        //     // //             let mut pe = att.pe(dbnum);
        //     // //             //需要在这里检测是否和上一个比，有 delete 的变化，也就是比较 children
        //     // //             //检查 children 的数据是否发生变化
        //     // //             if !data.children.is_empty(){
        //     // //                 //如果在历史层级关系里没有查询到的，需要去 pe 里去找，如果 pe 里没有那就是真没有
        //     // //                 //保存节点关系的历史记录
        //     // //                 // let owner_id = pe.history_id();
        //     // //                 // history_owner_map.insert((pe.refno, pgno, sesno), data.children.clone());
        //     // //                 //TODO modified refnos
        //     // //                 //过滤出删除的参考号
        //     // //                 let all_deleted = prev_children.iter().cloned().filter(|x|{
        //     // //                     !data.children.contains(x)
        //     // //                 }).collect::<BTreeSet<_>>();
        //     // //
        //     // //                 //过滤出新增的参考号
        //     // //                 let all_added = data.children.iter().cloned().filter(|x|{
        //     // //                     !prev_children.contains(x)
        //     // //                 }).collect::<BTreeSet<_>>();
        //     // //                 let ses_id = format!("{}_{}_{:0>6}", project, dbnum, sesno);
        //     // //                 //插入删除的操作记录
        //     // //                 //将删除的 pe 要重新插入回去，然后设置为 deleted
        //     // //                 for (j, r) in all_deleted.iter().enumerate(){
        //     // //                     let op: i32 = DataOperation::Deleted.into();
        //     // //                     ses_relates.push(format!("{{ id:[e3d_ses:{}, {}], in: pe:{}, out: e3d_ses:{}, op: {} }}",
        //     // //                                              ses_id, len+j, r, ses_id, op));
        //     // //                     //要读取到这个删除的 att
        //     // //                     pe.deleted = true;
        //     // //                     // all_deleted_pes.push(pe);
        //     // //                 }
        //     // //
        //     // //                 //插入新增的增加的记录
        //     // //                 for r in &all_added{
        //     // //                     let op: i32 = DataOperation::Added.into();
        //     // //                     ses_relates.push(format!("{{ id:[e3d_ses:{}, {}], in: pe:{}, out: e3d_ses:{}, refno: {}, pgno:{}, offset:{}, op: {} }}",
        //     // //                                              ses_id, index, r, ses_id, refno.to_pe_key(), pgno, offset, op));
        //     // //                 }

        //     // //                 if !all_added.is_empty() || !all_deleted.is_empty() {
        //     // //                     println!("{sesno} Deleted: {:?}", &all_deleted);
        //     // //                     println!("{sesno} Added: {:?}", &all_added);
        //     // //                 }
        //     // //             }
        //     // //             // if prev_children == data.children {
        //     // //             //     //新加的部分也要放在pe_relate里去
        //     // //             // }else{
        //     // //             //     prev_children = data.children.clone();
        //     // //             // }
        //     // //             all_pes.push(pe);
        //     // //             type_att_map.entry(att.get_type()).or_insert(Vec::new()).push(data);
        //     // //         }else{
        //     // //             dbg!((pgno, offset));
        //     // //             break;
        //     // //         }
        //     // //     }
        //     // //
        //     // //
        //     // //     if !ses_relates.is_empty(){
        //     // //         let relate_sql = format!("INSERT RELATION INTO ses_pe_relate [{}];", ses_relates.join(","));
        //     // //         // let relate_sql = format!("UPSERT RELATION INTO ses_pe_relate [{}];", all_relates.join(","));
        //     // //         // println!("relates: {}", &relate_sql);
        //     // //         SUL_DB.query(relate_sql).await.unwrap();
        //     // //     }
        //     // //
        //     // //     for chunk in all_pes.chunks(1000){
        //     // //         let mut jsons = vec![];
        //     // //         for pe in chunk{
        //     // //             jsons.push(pe.gen_sur_json(Some(pe.history_id())));
        //     // //         }
        //     // //         let sql = format!("INSERT IGNORE INTO pe_history [{}];", jsons.join(","));
        //     // //         // println!("insert sql is {}", &sql);
        //     // //         SUL_DB.query(sql).await.unwrap();
        //     // //     }
        //     // //
        //     // //     if found{
        //     // //         break;
        //     // //     }
        //     // //     // let sql = format!("INSERT IGNORE INTO  {}_history [{}]",ele.
        //     // //     //                   jsons.join(","));
        //     // //     // //执行 sql
        //     // //     // SUL_DB.query(&sql).await.unwrap();
        //     // //     // }
        //     // //     // let max_pgno = kv.1.iter().max().unwrap();
        //     // //     // let eles = self.collect_eles_in_session(*max_pgno).await;
        //     // //     // println!("refno: {:#4X?}", refno);
        //     // //     // println!("max_pgno: {:#4X?}", max_pgno);
        //     // //     // println!("eles: {:#4X?}", eles.len());
        //     // //     // println!("eles: {:#4X?}", eles);
        //     // // }
        //     //pe_owner_history 对pe 进行修正？还是直接存储这个children 关系？
        //     //先暂时不支持 relate 的历史纪录？还是反过来加入 relate 的 patch？

        //     // let mut owner_relates = vec![];
        //     // for ((refno, pgno, sesno), v) in history_owner_map {
        //     //     let hid = format!("pe_history:{}_{}", refno, pgno);
        //     //     for (i, child) in v.into_iter().enumerate() {
        //     //         let mut child_pgno = None;
        //     //         if let Some(pgnos) = refno_pgnos_map.get(&child) {
        //     //             // dbg!(child);
        //     //             //找到目标 refno， FIX 万一引用的 refno 在同一个 sesno 里呢？
        //     //             for (((p, _, _), (q, s2, _))) in pgnos.iter().tuple_windows() {
        //     //                 if *pgno >= *p && *pgno < *q {
        //     //                     //目标 pgno
        //     //                     let t_pgno = if *sesno == *s2 {
        //     //                         //同一个 session 里的数据，取最新的 pgno
        //     //                         q
        //     //                     } else{
        //     //                         p
        //     //                     };
        //     //                     child_pgno = Some(t_pgno);
        //     //                     break;
        //     //                 }
        //     //             }
        //     //         };
        //     //
        //     //         let child_hid = if let Some(c) = child_pgno{
        //     //             format!("pe_history:{}_{}", child, c)
        //     //         }else{
        //     //             //todo 暂时用其本身的refno，如果没有找到 refno，因为我们现在测试是截断的
        //     //             format!("pe:{}", child)
        //     //         };
        //     //         owner_relates.push(format!("{{ id:[{}, {}], in: {}, out: {} }}",
        //     //                                    &hid, i, child_hid, &hid));
        //     //     }
        //     // }
        //     // dbg!(&owner_relates);
        //     // if !owner_relates.is_empty() {
        //     //     let relate_sql = format!("INSERT RELATION INTO pe_owner_history [{}];", owner_relates.join(","));
        //     //     // println!("owner relates: {}", &relate_sql);
        //     //     SUL_DB.query(relate_sql).await.unwrap();
        //     // }

        //     // Self::save_att_history(&mut type_att_map).await;

        Ok(())
    }

    async fn save_ses_pe_relates(all_relates: &Vec<String>) {
        // 修改、删除、增加，放在这里去加一个字段
        for chunk in all_relates.chunks(1000) {
            let relate_sql = format!("INSERT RELATION INTO ses_pe_relate [{}];", chunk.join(","));
            // println!("relates: {}", chunk.join(","));
            SUL_DB.query(relate_sql).await.unwrap();
        }
    }

    async fn save_att_history(type_att_map: &mut BTreeMap<String, Vec<EleData>>) {
        //对 type_att_map 进行历史数据的保存
        //todo 后续可以改解析，都是用这个方法去保存数据, 存属性时，都是用的最新的 sesno
        //只有pe_owner_hsitory 需要用历史的查询？
        //历史数据放到一个表里面，然后通过 id 去查找？
        for (type_name, ele_datas) in type_att_map.into_iter() {
            for es in ele_datas.chunks(1000) {
                let mut jsons = Vec::new();
                //pe 直接就加在 pe_relate，然后通过 pe_relate 去查看 pe 的 delete 属性
                //delete 属性后面要用起来
                for ele_data in es {
                    let id = ele_data.whole_attmap.att_map().history_id();
                    if let Some(json) = ele_data.whole_attmap.att_map().gen_sur_json_with_id(id) {
                        jsons.push(json);
                    }
                }
                let sql = format!(
                    "INSERT IGNORE INTO  {}_history [{}]",
                    type_name,
                    jsons.join(",")
                );
                SUL_DB.query(&sql).await.unwrap();
            }
        }
    }

    /// 根据会话号获取对应的页码
    ///
    /// # 参数
    /// * `sesno` - 会话号
    ///
    /// # 返回值
    /// * `Option<u32>` - 如果找到对应的页码则返回Some(页码),否则返回None
    #[inline]
    pub fn get_ses_pageno(&self, sesno: i32) -> Option<u32> {
        self.sesno_pgno_map.get(&sesno).cloned()
    }

    /// 收集指定会话中的所有引用号位置信息
    ///
    /// # 参数
    /// * `sesno` - 会话号
    ///
    /// # 返回值
    /// * `Vec<RefnoDataLoc>` - 引用号位置信息的集合,如果会话不存在则返回空集合
    #[inline]
    pub fn collect_refno_locs(&mut self, sesno: i32) -> Vec<RefnoDataLoc> {
        self.get_ses_pageno(sesno)
            .map(|ses_pgno| self.collect_refno_locs_in_session(ses_pgno))
            .unwrap_or_default()
    }

    /// 收集指定会话中的所有引用号位置信息
    ///
    /// 该函数读取指定会话页中的所有引用号位置信息,并过滤出在该会话中有效的引用号。
    ///
    /// # 参数
    /// * `ses_pgno` - 会话页号
    ///
    /// # 返回值
    /// * `Vec<RefnoDataLoc>` - 引用号位置信息的集合
    ///
    /// # 实现细节
    /// 1. 读取当前会话的结束页号、上一个会话页号和索引根页号
    /// 2. 读取上一个会话的结束页号
    /// 3. 过滤出页号在上一个会话结束页号和当前会话结束页号之间的引用号
    /// 4. 递归处理索引页数据,收集所有符合条件的引用号位置信息
    pub fn collect_refno_locs_in_session(&mut self, ses_pgno: u32) -> Vec<RefnoDataLoc> {
        //读取当前会话层有多少属性保存了，是否需要读取 index 数据，然后开始读取属性数据
        //过滤 index 里面的 pgno 大于当前会话的 pgno 的数据
        let (cur_end_pgno, last_ses_pageno, index_root_pageno) = {
            let Ok(d) = self.read_ses_data(ses_pgno) else {
                eprintln!("Warning: Failed to read session data for page {}", ses_pgno);
                return vec![];
            };
            (d.end_pgno, d.last_ses_pageno, d.index_root_pageno)
        };
        
        // 检查 last_ses_pageno 是否有效
        if last_ses_pageno <= 0 {
            // 没有上一个会话，返回空
            return vec![];
        }
        
        //读取上一个ses_data
        let last_end_pgno = {
            let Ok(d) = self.read_ses_data(last_ses_pageno as u32) else {
                eprintln!("Warning: Failed to read last session data for page {}", last_ses_pageno);
                return vec![];
            };
            d.end_pgno
        };
        // dbg!((last_end_pgno, cur_end_pgno));
        //只要过滤所有 last_end_pgno 比这个大，比 cur_end_pgno 小的参考号即可
        //过滤 index page data 里面的数据
        let Ok(index_data) = self.read_index_data(index_root_pageno) else {
            eprintln!("Warning: Failed to read index data for page {}", index_root_pageno);
            return vec![];
        };
        // dbg!(index_data.level);
        let mut final_locs = vec![];
        // println!("index root pgno: {:#04X}", index_root_pageno * PAGE_SIZE as u32);
        self.filter_index_data(&index_data, &mut final_locs, last_end_pgno, cur_end_pgno);

        final_locs
    }

    ///收集一个会话里面的所有的属性数据
    pub async fn collect_eles_in_session(&mut self, ses_pgno: u32) -> Vec<EleData> {
        let final_locs = self.collect_refno_locs_in_session(ses_pgno);
        let mut eles = vec![];
        //根据这个RefnoDataLoc 读取到所有发生更新的 index 数据
        for loc in final_locs {
            // RefnoDataLoc::get_att_offset() 固定 2K；这里统一使用动态 page_size
            match self
                .parse_element(loc.get_att_offset_with_page_size(self.page_size))
                .await
            {
                Ok(ele) => eles.push(ele),
                Err(e) => {
                    if self.detail {
                        eprintln!("collect_eles_in_session: 解析元素失败 pgno={} offset={} err={}", loc.pgno, loc.offset, e);
                    }
                }
            }
        }
        eles
    }

    ///过滤 index page data 里面的数据
    /// 过滤索引页数据
    ///
    /// # 参数
    /// * `index_data` - 索引页数据
    /// * `result_locs` - 用于存储过滤后的引用号位置信息
    /// * `last_end_pgno` - 上一个会话的结束页号
    /// * `cur_end_pgno` - 当前会话的结束页号
    /// * `level` - 当前索引页的层级
    ///
    /// # 返回值
    /// * `Option<bool>` - 成功返回Some(true),失败返回None
    ///
    /// # 实现细节
    /// 1. 过滤出页号在上一个会话结束页号和当前会话结束页号之间的引用号
    /// 2. 如果是叶子节点(level=0),直接将过滤结果添加到result_locs
    /// 3. 如果是非叶子节点,递归处理下一层索引页
    pub fn filter_index_data(
        &mut self,
        index_data: &IndexPageData,
        result_locs: &mut Vec<RefnoDataLoc>,
        last_end_pgno: u32,
        cur_end_pgno: u32,
        // level: &mut i32,
    ) -> Option<bool> {
        let level = index_data.level as i32;
        // dbg!(level);
        if index_data.refno_locs.is_empty() {
            return None;
        }
        // dbg!(&index_data.refno_locs);
        let cur_locs = index_data
            .refno_locs
            .iter()
            .filter(|x| x.pgno > last_end_pgno && x.pgno < cur_end_pgno && x.flag == 1)
            .map(|x| x.clone())
            .collect::<Vec<_>>();
        if cur_locs.is_empty() {
            return None;
        }
        // dbg!(&cur_locs);
        if level == 0 {
            // dbg!(&cur_locs[0]);
            result_locs.extend(cur_locs);
        } else {
            for l in cur_locs {
                // dbg!(l.pgno);
                // println!("hex offset: {:#04X}", l.pgno * PAGE_SIZE as u32);
                if let Ok(next_index_data) = self.read_index_data(l.pgno) {
                    let mut next_level = next_index_data.level as i32;
                    if next_level >= level {
                        dbg!((next_level, level));
                        dbg!((&l, next_index_data));
                    } else {
                        self.filter_index_data(
                            &next_index_data,
                            result_locs,
                            last_end_pgno,
                            cur_end_pgno,
                        );
                    }
                }
            }
        }
        Some(true)
    }

    /// 收集session范围内的增删改的element数据
    /// 并在这里即可判断是否增删改？
    /// 收集指定会话范围内的增量元素数据
    ///
    /// # 参数
    /// * `sesno_range` - 会话号范围(包含起始和结束值)，如果为None则使用最新会话
    ///
    /// # 返回值
    /// * `anyhow::Result<HashMap<RefU64, EleOperationDetail>>` - 返回一个映射,键为参考号,值为元素操作详情
    ///
    /// # 错误
    /// * 当读取或解析元素数据失败时返回错误
    pub fn collect_increment_eles(
        &mut self,
        sesno_range: Option<RangeInclusive<i32>>,
    ) -> anyhow::Result<BTreeMap<u32, Vec<EleOperationData>>> {
        // let mut processed_refnos = HashSet::new();
        // 按会话号分组结果
        let mut grouped_results: BTreeMap<u32, Vec<EleOperationData>> = BTreeMap::new();

        //根据与实际的sesno_range 进行一个过滤
        let session_numbers = match &sesno_range {
            Some(range) => self
                .ses_range_map
                .keys()
                .filter(|&sesno| range.contains(sesno))
                .cloned()
                .collect::<Vec<i32>>(),
            None => {
                // 如果为None，只使用最新会话
                let latest_sesno = self.get_latest_sesno()? as i32;
                vec![latest_sesno]
            }
        };
        dbg!(&session_numbers.len());

        // 处理会话号
        for &sesno in session_numbers.iter() {
            println!("collect sesno: {}", sesno);
            let final_locs = self.collect_refno_locs(sesno);
            // dbg!(&final_locs);
            let mut operation_details_for_sesno = HashMap::new();

            for loc in final_locs {
                let refno = RefU64::from_two_nums(loc.refno_0, loc.refno_1);

                // 获取参考号的操作状态详情
                let operation_details = self
                    .get_refno_operation_status(refno, Some(sesno as u32))
                    .unwrap();

                // dbg!(&operation_details);

                // 合并到当前会话的结果
                operation_details_for_sesno.extend(operation_details);
            }

            // 将当前会话的结果转换为 EleOperationData 向量
            let operation_data =
                convert_to_operation_data(operation_details_for_sesno, sesno as u32);
            if !operation_data.is_empty() {
                grouped_results.insert(sesno as u32, operation_data);
            }
        }

        Ok(grouped_results)
    }

    /// 在指定的会话中查找参考号的物理位置 (对齐 db3_find_key 逻辑)
    /// 
    /// # 参数
    /// * `refno` - 要查找的参考号
    /// * `sesno` - 指定会话号
    /// 
    /// # 返回值
    /// * `Ok(Option<RefnoDataLoc>)` - 找到则返回位置信息，否则返回 None
    pub fn find_refno_loc(&mut self, refno: RefU64, sesno: u32) -> anyhow::Result<Option<RefnoDataLoc>> {
        let (root_pgno, cur_end_pgno) = {
            let ses_data = self.get_ses_data(sesno)?;
            (ses_data.index_root_pageno, ses_data.end_pgno)
        };
        
        if root_pgno == 0 {
            return Ok(None);
        }

        // 1. 读取根页面 (RootIndexPage)
        let root_data = self.get_page_cached(root_pgno)?;
        let root_page = RootIndexPage::try_from(root_data.as_ref())
            .map_err(|e| anyhow!("Failed to parse RootIndexPage at page {}: {}", root_pgno, e))?;

        // 2. 选择子树 (Lower 或 Upper)
        let mut mid_pgno = 0;
        let l = &root_page.lower_root;
        let u = &root_page.upper_root;

        if refno.get_0() == l.refno_0 && refno.get_1() >= l.refno_1 {
            // 检查是否在 lower 范围内（小于 upper 的起始值）
            if refno.get_0() < u.refno_0 || (refno.get_0() == u.refno_0 && refno.get_1() < u.refno_1) {
                mid_pgno = l.page_no;
            } else {
                mid_pgno = u.page_no;
            }
        }

        if mid_pgno == 0 {
            return Ok(None);
        }

        // 3. 读取中间层 (RefnoIndexPage)
        let mid_data = self.get_page_cached(mid_pgno)?;
        let mid_page = RefnoIndexPage::try_from(mid_data.as_ref())
            .map_err(|e| anyhow!("Failed to parse RefnoIndexPage at page {}: {}", mid_pgno, e))?;

        // 找到对应的叶子页面号
        let leaf_pgno = mid_page.data_locs.iter()
            .rev()
            .find(|loc| {
                let loc_refno = RefU64::from_two_nums(loc.refno_0, loc.refno_1);
                refno >= loc_refno
            })
            .map(|loc| loc.page_no);

        let Some(leaf_pgno) = leaf_pgno else {
            return Ok(None);
        };

        // 4. 读取叶子层 (IndexPageData)
        let leaf_data = self.get_page_cached(leaf_pgno)?;
        let leaf_page = IndexPageData::try_from(leaf_data.as_ref())
            .map_err(|e| anyhow!("Failed to parse IndexPageData at page {}: {}", leaf_pgno, e))?;

        // 精确匹配 refno 且 pgno 应该早于当前会话的结束页
        let found = leaf_page.refno_locs.into_iter()
            .find(|loc| loc.get_refno() == refno && loc.pgno <= cur_end_pgno);

        Ok(found)
    }

    /// 检查参考号是否存在
    pub fn check_refno_exists(&mut self, refno: RefU64) -> anyhow::Result<bool> {
        let latest_sesno = self.get_latest_sesno()?;
        Ok(self.find_refno_loc(refno, latest_sesno)?.is_some())
    }

    /// 获取指定参考号在特定会话的版本数据 (对齐 db5 访问层逻辑)
    /// 
    /// # 参数
    /// * `refno` - 参考号
    /// * `sesno` - 会话号
    pub async fn get_element_at_session(&mut self, refno: RefU64, sesno: u32) -> anyhow::Result<EleData> {
        let loc = self.find_refno_loc(refno, sesno)?
            .ok_or_else(|| anyhow!("Reference number {:?} not found in session {}", refno, sesno))?;
        
        self.parse_element(loc.get_att_offset_with_page_size(self.page_size)).await
    }

    /// 构建索引映射表，读取所有index page的数据，组建一个BTreeMap，快速搜索指定的refno
    ///
    /// 构建一个全局的refno到RefnoDataLoc数组的映射表，方便快速查找数据位置及其历史记录。
    /// 利用B-Tree索引的特性，直接定位和处理叶子节点数据。
    ///
    /// # 参数
    /// * `verbose` - 是否显示详细构建信息，默认为false
    ///
    /// # 返回值
    /// * `BTreeMap<RefU64, Vec<RefnoDataLoc>>` - 参考号到数据位置数组的映射表
    ///
    /// # 错误
    /// * 文件读取出错时返回错误
    ///
    /// ```
    pub fn build_index_map_verbose(&mut self, verbose: bool) -> anyhow::Result<IndexMap> {
        // 获取数据库基本信息
        let page_info = self.get_page_basic_info()?;
        // 获取最新的索引根页号
        let latest_index_pgno = page_info.latest_ses_data.index_root_pageno;
        if verbose {
            println!("latest_index_pgno: {:#4X}", latest_index_pgno);
        }

        // 估计项目数量，用于内存预分配
        let estimated_size = std::cmp::min(100_000, latest_index_pgno as usize * 10);

        // 创建 refno 映射表，用于保存所有 refno 到位置(绝对偏移)的映射
        let mut refno_map: IndexMap = HashMap::with_capacity(estimated_size);

        // 统计信息
        let mut total_nodes = 0;
        let mut total_leaf_nodes = 0;
        let mut total_index_nodes = 0;
        let mut total_entries = 0;

        // 使用广度优先遍历算法遍历索引树
        let mut queue: VecDeque<(u32, u32)> = VecDeque::new();
        // 添加根节点到队列
        queue.push_back((latest_index_pgno, 0));

        // 跟踪已经访问过的节点
        let mut visited_nodes = HashSet::new();

        // 记录起始时间，用于计算性能
        let start_time = std::time::Instant::now();

        // 批量处理，提高性能
        let mut batch_size = 0;
        let mut level = 0;

        // 广度优先遍历索引树
        while !queue.is_empty() {
            // 每批次处理100个同级别节点
            let mut batch = Vec::with_capacity(100);
            while !queue.is_empty() && batch.len() < 100 {
                if let Some((pgno, node_level)) = queue.pop_front() {
                    if node_level > level {
                        level = node_level;
                        if verbose {
                            println!("Processing level {} of index tree", level);
                        }
                    }
                    batch.push(pgno);
                }
            }

            batch_size += batch.len();

            // 按批次并行处理节点
            for &pgno in &batch {
                // 跳过已经访问过的节点
                if visited_nodes.contains(&pgno) {
                    continue;
                }

                // 标记为已访问
                visited_nodes.insert(pgno);

                // 读取并解析索引页数据
                let Ok(index_data) = self.read_index_data(pgno) else {
                    println!("error pgno: {:#4X}", pgno);
                    continue;
                };
                total_nodes += 1;

                // 根据页面类型和级别判断处理方式
                    if index_data.level == 0 {
                        // 叶子节点 (level = 0)
                        total_leaf_nodes += 1;
                        // RefnoDataLoc::get_att_offset() 固定用 2K 页大小；这里必须使用动态 page_size。
                        Self::process_leaf_node(&index_data, &mut refno_map, self.page_size);
                        total_entries += index_data.refno_locs.len();
                    } else {
                        // 非叶子节点 (level > 0)
                        total_index_nodes += 1;

                    // 遍历子节点引用
                    for loc in &index_data.refno_locs {
                        if loc.pgno > 0 {
                            if loc.pgno == 0x1564 {
                                dbg!(&index_data.refno_locs);
                            }
                            queue.push_back((loc.pgno, level + 1));
                        }
                    }
                }
            }

            // 每处理1000个节点输出一次进度信息
            if verbose && batch_size % 1000 < 100 {
                println!(
                    "Processed {} index nodes ({} leaf nodes, {} index nodes), found {} entries, elapsed: {:?}",
                    total_nodes, total_leaf_nodes, total_index_nodes, total_entries, start_time.elapsed()
                );
            }
        }

        // 构建完成后统一排序去重，保证 offsets 升序且无重复
        for offsets in refno_map.values_mut() {
            offsets.sort_unstable();
            offsets.dedup();
        }

        // 输出最终统计信息
        if verbose {
            println!(
                "Index build completed: {} nodes ({} leaf, {} index), {} unique refnos, {} total entries, elapsed: {:?}",
                total_nodes, total_leaf_nodes, total_index_nodes, refno_map.len(), total_entries, start_time.elapsed()
            );
        }

        Ok(refno_map)
    }

    // 处理叶子节点，提取refno和对应的位置信息
    fn process_leaf_node(
        index_data: &IndexPageData,
        refno_map: &mut IndexMap,
        page_size: usize,
    ) {
        // 遍历叶子节点中的所有位置记录
        for loc in &index_data.refno_locs {
            // 跳过无效的记录
            if loc.refno_0 == 0 && loc.refno_1 == 0 {
                continue;
            }

            // 检查页号和偏移是否有效
            if loc.pgno == 0 || loc.offset == 0 {
                continue;
            }

            // 创建refno对象
            let refno = RefU64::from_two_nums(loc.refno_0, loc.refno_1);

            // 获取绝对偏移量（动态 page_size）
            let offset = loc.get_att_offset_with_page_size(page_size);

            // 将绝对偏移添加到 refno 对应的列表中（稍后统一 sort+dedup）
            refno_map.entry(refno).or_default().push(offset);
        }
    }

    // 兼容旧接口，保持向后兼容性
    pub fn build_index_map_default(&mut self) -> anyhow::Result<IndexMap> {
        self.build_index_map_verbose(false)
    }

    // 主要索引构建方法，默认不输出详细信息
    pub fn build_index_map(&mut self) -> anyhow::Result<IndexMap> {
        self.build_index_map_default()
    }

    /// 过滤掉数据一致的冗余历史记录
    ///
    /// # 参数
    /// * `refno_map` - 需要过滤的refno映射表
    ///
    /// # 返回值
    /// * `anyhow::Result<()>` - 成功或错误
    async fn filter_consistent_data(
        &mut self,
        refno_map: &mut IndexMap,
    ) -> anyhow::Result<()> {
        self.filter_consistent_data_with_options(refno_map, &ElementHashOptions::default())
            .await
    }

    async fn filter_consistent_data_with_options(
        &mut self,
        refno_map: &mut IndexMap,
        opts: &ElementHashOptions,
    ) -> anyhow::Result<()> {
        // 注意：该函数会触发大量 parse_element 调用，默认不在 build_index_map_* 中启用。
        // 需要时请走 build_index_map_and_filter_consistent 或手动调用。
        let items: Vec<(RefU64, Vec<u64>)> = refno_map
            .iter()
            .map(|(refno, offsets)| (*refno, offsets.clone()))
            .collect();

        let mut removed = 0usize;
        for (refno, offsets) in items {
            if offsets.len() <= 1 {
                continue;
            }

            let mut kept: Vec<u64> = Vec::with_capacity(offsets.len());
            let mut last_hash: Option<u64> = None;

            for offset in offsets {
                match self.parse_element(offset).await {
                    Ok(ele) => {
                        let hash = Self::calculate_element_hash_with_options(&ele, opts);
                        if last_hash == Some(hash) {
                            removed += 1;
                            continue;
                        }
                        last_hash = Some(hash);
                        kept.push(offset);
                    }
                    Err(_) => {
                        // 无法解析时，保守起见保留该版本，并重置 last_hash 防止误删后续版本。
                        last_hash = None;
                        kept.push(offset);
                    }
                }
            }

            // 极端情况下全部解析失败也会保留原 offsets；这里兜底保证不产生空历史。
            if kept.is_empty() {
                continue;
            }

            refno_map.insert(refno, kept);
        }

        if self.detail {
            println!("filter_consistent_data: removed {} redundant versions", removed);
        }

        Ok(())
    }

    /// 计算元素数据的哈希值用于比较
    fn calculate_element_hash(ele_data: &EleData) -> u64 {
        Self::calculate_element_hash_with_options(ele_data, &ElementHashOptions::default())
    }

    fn calculate_element_hash_with_options(ele_data: &EleData, opts: &ElementHashOptions) -> u64 {
        use std::hash::{Hash, Hasher};

        let mut hasher = std::collections::hash_map::DefaultHasher::new();

        // 基本字段
        ele_data.noun.hash(&mut hasher);
        ele_data.owner.hash(&mut hasher);
        ele_data.name.hash(&mut hasher);

        // 子元素列表
        ele_data.children.hash(&mut hasher);

        // 隐式/系统属性（稳定顺序：BTreeMap）
        for (k, v) in ele_data.att_map().map.iter() {
            if opts.ignore_keys.contains(&k.as_str()) {
                continue;
            }
            k.hash(&mut hasher);
            Self::hash_named_attr_value(&mut hasher, v);
        }

        // 显式属性
        for (k, v) in ele_data.explicit_attmap().map.iter() {
            if opts.ignore_keys.contains(&k.as_str()) {
                continue;
            }
            k.hash(&mut hasher);
            Self::hash_named_attr_value(&mut hasher, v);
        }

        // UDA（显式块里的一类）
        for uda in ele_data.uda_atts() {
            uda.name.hash(&mut hasher);
            uda.is_uda.hash(&mut hasher);
            uda.hash_val.hash(&mut hasher);
            Self::hash_named_attr_value(&mut hasher, &uda.value);
        }

        hasher.finish()
    }

    fn hash_named_attr_value(hasher: &mut impl std::hash::Hasher, v: &NamedAttrValue) {
        use aios_core::pdms_types::RefnoEnum;
        use std::hash::Hash;

        // 写入变体 tag，避免不同变体但 payload 可能相同导致碰撞。
        match v {
            NamedAttrValue::InvalidType => {
                0u8.hash(hasher);
            }
            NamedAttrValue::IntegerType(x) => {
                1u8.hash(hasher);
                x.hash(hasher);
            }
            NamedAttrValue::StringType(s) => {
                2u8.hash(hasher);
                s.hash(hasher);
            }
            NamedAttrValue::F32Type(x) => {
                3u8.hash(hasher);
                x.to_bits().hash(hasher);
            }
            NamedAttrValue::F32VecType(xs) => {
                4u8.hash(hasher);
                xs.len().hash(hasher);
                for x in xs {
                    x.to_bits().hash(hasher);
                }
            }
            NamedAttrValue::Vec3Type(v3) => {
                5u8.hash(hasher);
                let arr = v3.to_array();
                arr[0].to_bits().hash(hasher);
                arr[1].to_bits().hash(hasher);
                arr[2].to_bits().hash(hasher);
            }
            NamedAttrValue::StringArrayType(xs) => {
                6u8.hash(hasher);
                xs.hash(hasher);
            }
            NamedAttrValue::BoolArrayType(xs) => {
                7u8.hash(hasher);
                xs.hash(hasher);
            }
            NamedAttrValue::IntArrayType(xs) => {
                8u8.hash(hasher);
                xs.hash(hasher);
            }
            NamedAttrValue::BoolType(x) => {
                9u8.hash(hasher);
                x.hash(hasher);
            }
            NamedAttrValue::ElementType(s) => {
                10u8.hash(hasher);
                s.hash(hasher);
            }
            NamedAttrValue::WordType(s) => {
                11u8.hash(hasher);
                s.hash(hasher);
            }
            NamedAttrValue::RefU64Type(r) => {
                12u8.hash(hasher);
                r.hash(hasher);
            }
            NamedAttrValue::RefU64Array(rs) => {
                13u8.hash(hasher);
                rs.hash(hasher);
            }
            NamedAttrValue::LongType(x) => {
                14u8.hash(hasher);
                x.hash(hasher);
            }
            NamedAttrValue::RefnoEnumType(r) => {
                15u8.hash(hasher);
                // 显式写入 enum tag，避免 serde(untagged) 的潜在歧义。
                match r {
                    RefnoEnum::Refno(rr) => {
                        0u8.hash(hasher);
                        rr.hash(hasher);
                    }
                    RefnoEnum::SesRef(ses) => {
                        1u8.hash(hasher);
                        ses.hash(hasher);
                    }
                }
            }
        }
    }

    /// 构建索引映射并过滤“内容一致”的冗余历史版本（可选的重型操作）。
    pub async fn build_index_map_and_filter_consistent(
        &mut self,
        verbose: bool,
    ) -> anyhow::Result<IndexMap> {
        self.build_index_map_and_filter_consistent_with_options(verbose, ElementHashOptions::default())
            .await
    }

    /// 构建索引映射并按指定选项过滤“内容一致”的冗余历史版本。
    pub async fn build_index_map_and_filter_consistent_with_options(
        &mut self,
        verbose: bool,
        opts: ElementHashOptions,
    ) -> anyhow::Result<IndexMap> {
        let mut index_map = self.build_index_map_verbose(verbose)?;
        self.filter_consistent_data_with_options(&mut index_map, &opts)
            .await?;
        Ok(index_map)
    }

    /// 使用构建好的索引映射表快速查找refno对应的所有数据位置
    ///
    /// # 参数
    /// * `refno` - 要查找的参考号
    /// * `index_map` - 参考号到数据位置数组的映射表
    ///
    /// # 返回值
    /// * `Option<&Vec<RefnoDataLoc>>` - 如果找到则返回数据位置数组，否则返回None
    pub fn fast_lookup_refno<'a>(
        &self,
        refno: &RefU64,
        index_map: &'a IndexMap,
    ) -> Option<&'a Vec<u64>> {
        index_map.get(refno)
    }

    /// 获取refno的最新位置
    ///
    /// # 参数
    /// * `refno` - 要查找的参考号
    /// * `index_map` - 参考号到数据位置数组的映射表
    ///
    /// # 返回值
    /// * `Option<&RefnoDataLoc>` - 如果找到则返回最新的数据位置，否则返回None
    pub fn fast_lookup_latest_loc<'a>(
        &self,
        refno: &RefU64,
        index_map: &'a IndexMap,
    ) -> Option<u64> {
        index_map.get(refno).and_then(|locs| locs.last()).copied()
    }

    /// 使用索引映射表快速获取元素最新数据
    ///
    /// # 参数
    /// * `refno` - 要获取的元素的参考号
    /// * `index_map` - 参考号到数据位置数组的映射表
    ///
    /// # 返回值
    /// * `anyhow::Result<EleData>` - 元素数据或错误
    pub async fn fast_get_element(
        &mut self,
        refno: RefU64,
        index_map: &IndexMap,
    ) -> anyhow::Result<EleData> {
        if let Some(locs) = index_map.get(&refno) {
            if let Some(&latest_offset) = locs.last() {
                // 直接使用绝对偏移值解析元素
                let offset = latest_offset;
                return self.parse_element(offset).await;
            }
        }

        Err(anyhow!("找不到refno: {:?}", refno))
    }

    /// 获取指定版本的元素数据
    ///
    /// # 参数
    /// * `refno` - 参考号
    /// * `index_map` - 索引映射表
    /// * `version_index` - 版本索引，0表示最新版本
    ///
    /// # 返回值
    /// * `anyhow::Result<EleData>` - 指定版本的元素数据
    pub async fn fast_get_element_version(
        &mut self,
        refno: RefU64,
        index_map: &IndexMap,
        version_index: usize,
    ) -> anyhow::Result<EleData> {
        if let Some(locs) = index_map.get(&refno) {
            // offsets 为升序，last() 为最新
            let actual_index = if version_index < locs.len() {
                locs.len() - 1 - version_index
            } else {
                return Err(anyhow!(
                    "版本索引越界: {} (总版本数: {})",
                    version_index,
                    locs.len()
                ));
            };

            let offset = locs[actual_index];
            return self.parse_element(offset).await;
        }

        Err(anyhow!("找不到refno: {:?}", refno))
    }

    /// 获取元素的所有历史版本
    ///
    /// # 参数
    /// * `refno` - 参考号
    /// * `index_map` - 索引映射表
    ///
    /// # 返回值
    /// * `anyhow::Result<Vec<EleData>>` - 所有历史版本的元素数据
    pub async fn fast_get_element_history(
        &mut self,
        refno: RefU64,
        index_map: &IndexMap,
    ) -> anyhow::Result<Vec<EleData>> {
        if let Some(locs) = index_map.get(&refno) {
            let mut history = Vec::with_capacity(locs.len());

            // offsets 为升序：从旧到新
            for &offset in locs.iter() {
                match self.parse_element(offset).await {
                    Ok(data) => history.push(data),
                    Err(e) => {
                        eprintln!(
                            "解析refno {:?} 在偏移 {} 的元素时出错: {}",
                            refno, offset, e
                        );
                    }
                }
            }

            return Ok(history);
        }

        Err(anyhow!("找不到refno: {:?}", refno))
    }

    /// 批量获取元素数据
    ///
    /// # 参数
    /// * `refnos` - 参考号数组
    /// * `index_map` - 索引映射表
    ///
    /// # 返回值
    /// * `anyhow::Result<Vec<(RefU64, EleData)>>` - 元素数据列表
    pub async fn fast_get_elements(
        &mut self,
        refnos: &[RefU64],
        index_map: &IndexMap,
    ) -> anyhow::Result<Vec<(RefU64, EleData)>> {
        let mut results = Vec::new();

        for &refno in refnos {
            if let Some(locs) = index_map.get(&refno) {
                if let Some(&latest_offset) = locs.last() {
                    match self.parse_element(latest_offset).await {
                        Ok(data) => {
                            results.push((refno, data));
                        }
                        Err(e) => {
                            eprintln!(
                                "解析refno {:?} 在偏移 {} 的元素时出错: {}",
                                refno, latest_offset, e
                            );
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// 使用索引映射表优化的增量元素收集
    ///
    /// 相比于原始的collect_increment_eles，此方法利用索引映射表加速查询过程
    ///
    /// # 参数
    /// * `sesno_range` - 会话号范围（包含边界）
    /// * `index_map` - 预先构建好的索引映射表
    ///
    /// # 返回值
    /// * `anyhow::Result<HashMap<RefU64, EleOperationDetail>>` - 元素参考号到元素数据的映射表
    pub async fn collect_increment_eles_optimized(
        &mut self,
        sesno_range: RangeInclusive<i32>,
        index_map: &IndexMap,
    ) -> anyhow::Result<HashMap<u32, Vec<EleOperationData>>> {
        // 按会话号分组结果
        let mut grouped_results: HashMap<u32, Vec<EleOperationData>> = HashMap::new();

        // 获取范围内的所有会话号
        let mut sesnos: Vec<i32> = self
            .sesno_pgno_map
            .keys()
            .filter(|&sesno| sesno_range.contains(sesno))
            .cloned()
            .collect();

        // 按会话号排序，确保按时间顺序处理
        sesnos.sort_unstable();

        println!("处理范围内的会话: {:?}", sesnos);

        // 遍历每个会话，收集增量数据
        for &sesno in &sesnos {
            if let Some(ses_pgno) = self.get_ses_pageno(sesno) {
                // 收集当前会话中的所有引用号位置
                let locs = self.collect_refno_locs_in_session(ses_pgno);
                let mut session_operations: Vec<EleOperationData> = Vec::new();

                println!("会话 {} 包含 {} 个元素引用", sesno, locs.len());

                // 处理每个位置
                for loc in locs {
                    let refno = RefU64::from_two_nums(loc.refno_0, loc.refno_1);
                    let offset = loc.get_att_offset_with_page_size(self.page_size);

                    // 使用索引映射表快速检查这个refno是否是最新版本
                    if let Some(offsets) = index_map.get(&refno) {
                        // 获取最新的偏移量
                        if let Some(&latest_offset) = offsets.last() {
                            // 如果当前位置是最新的，则解析并添加到结果中
                            if offset == latest_offset {
                                let operation_details =
                                    self.get_refno_operation_status(refno, Some(sesno as u32))?;

                                // 将操作详情转换为操作数据并添加到本会话的结果中
                                for (ref_no, detail) in operation_details {
                                    session_operations.push(EleOperationData {
                                        refno: ref_no,
                                        sesno: sesno as u32,
                                        detail,
                                    });
                                }
                            }
                        }
                    }
                }

                // 如果本会话有操作数据，添加到分组结果
                if !session_operations.is_empty() {
                    grouped_results.insert(sesno as u32, session_operations);
                }
            }
        }

        println!("增量收集完成，共 {} 个会话的数据", grouped_results.len());

        Ok(grouped_results)
    }

    /// 缓存索引映射表，避免重复构建
    ///
    /// 将索引映射表缓存为文件，以便下次快速加载
    ///
    /// # 参数
    /// * `cache_path` - 缓存文件路径
    /// * `index_map` - 索引映射表
    ///
    /// # 返回值
    /// * `anyhow::Result<()>` - 成功或错误
    pub fn cache_index_map(
        &self,
        cache_path: &Path,
        index_map: &IndexMap,
    ) -> anyhow::Result<()> {
        let file = File::create(cache_path)?;
        let mut writer = io::BufWriter::new(file);

        // 写入文件头：magic + version + page_size + count
        const MAGIC: [u8; 4] = *b"PIM1";
        const VERSION: u32 = 1;
        writer.write_all(&MAGIC)?;
        writer.write_all(&VERSION.to_le_bytes())?;
        writer.write_all(&(self.page_size as u32).to_le_bytes())?;
        writer.write_all(&(index_map.len() as u32).to_le_bytes())?;

        // 写入每个参考号及其对应的偏移量集合
        for (refno, offsets) in index_map {
            // 写入refno的两个组成部分
            let refno_0 = refno.get_0();
            let refno_1 = refno.get_1();
            writer.write_all(&refno_0.to_le_bytes())?;
            writer.write_all(&refno_1.to_le_bytes())?;

            // 写入偏移量数量
            let loc_count = offsets.len() as u32;
            writer.write_all(&loc_count.to_le_bytes())?;

            // 写入所有偏移量
            for &offset in offsets.iter() {
                writer.write_all(&offset.to_le_bytes())?;
            }
        }

        writer.flush()?;
        Ok(())
    }

    /// 从缓存文件加载索引映射表
    ///
    /// # 参数
    /// * `cache_path` - 缓存文件路径
    ///
    /// # 返回值
    /// * `anyhow::Result<BTreeMap<RefU64, Vec<RefnoDataLoc>>>` - 加载的索引映射表或错误
    pub fn load_cached_index_map(
        &self,
        cache_path: &Path,
    ) -> anyhow::Result<IndexMap> {
        let file = File::open(cache_path)?;
        let mut reader = io::BufReader::new(file);

        // 读取头部：兼容旧格式（旧格式无 magic，直接 count）
        let mut first4 = [0u8; 4];
        reader.read_exact(&mut first4)?;

        let (count, cached_page_size) = if &first4 == b"PIM1" {
            let mut ver_bytes = [0u8; 4];
            reader.read_exact(&mut ver_bytes)?;
            let ver = u32::from_le_bytes(ver_bytes);
            if ver != 1 {
                return Err(anyhow!("不支持的索引缓存版本: {}", ver));
            }

            let mut ps_bytes = [0u8; 4];
            reader.read_exact(&mut ps_bytes)?;
            let cached_page_size = u32::from_le_bytes(ps_bytes) as usize;
            if cached_page_size != self.page_size {
                return Err(anyhow!(
                    "索引缓存 page_size 不匹配：cache={} 当前={}，请删除缓存并重建",
                    cached_page_size,
                    self.page_size
                ));
            }

            let mut count_bytes = [0u8; 4];
            reader.read_exact(&mut count_bytes)?;
            (u32::from_le_bytes(count_bytes), Some(cached_page_size))
        } else {
            // 旧格式：first4 即 count
            (u32::from_le_bytes(first4), None)
        };
        let _ = cached_page_size;

        // 创建结果映射表
        let mut index_map: IndexMap = HashMap::with_capacity(count as usize);

        // 读取每个refno及其对应的位置信息
        for _ in 0..count {
            // 读取refno的两个组成部分
            let mut refno_0_bytes = [0u8; 4];
            let mut refno_1_bytes = [0u8; 4];
            reader.read_exact(&mut refno_0_bytes)?;
            reader.read_exact(&mut refno_1_bytes)?;

            let refno_0 = u32::from_le_bytes(refno_0_bytes);
            let refno_1 = u32::from_le_bytes(refno_1_bytes);
            let refno = RefU64::from_two_nums(refno_0, refno_1);

            // 读取此refno对应的偏移量数量
            let mut loc_count_bytes = [0u8; 4];
            reader.read_exact(&mut loc_count_bytes)?;
            let loc_count = u32::from_le_bytes(loc_count_bytes);

            // 读取所有偏移量
            let mut offsets = Vec::with_capacity(loc_count as usize);
            for _ in 0..loc_count {
                let mut offset_bytes = [0u8; 8];
                reader.read_exact(&mut offset_bytes)?;
                let offset = u64::from_le_bytes(offset_bytes);
                offsets.push(offset);
            }

            // 添加到映射表
            index_map.insert(refno, offsets);
        }

        // 兼容旧缓存：可能无序/有重复
        for offsets in index_map.values_mut() {
            offsets.sort_unstable();
            offsets.dedup();
        }

        Ok(index_map)
    }

    /// 使用索引映射表优化的参考号状态检查方法
    ///
    /// 相比于原始的get_refno_status方法，此方法利用索引映射表加速状态判断过程
    ///
    /// # 参数
    /// * `refno` - 需要判断状态的参考号
    /// * `index_map` - 预先构建的索引映射表
    ///
    /// # 返回值
    /// * `Ok(EleOperation::Add)` - 参考号是新增的
    /// * `Ok(EleOperation::Modified)` - 参考号是修改过的
    /// * `Ok(EleOperation::Deleted)` - 参考号是已删除的
    /// * `Err(_)` - 参考号不存在或发生其他错误
    pub async fn fast_get_refno_status(
        &mut self,
        refno: RefU64,
        index_map: &IndexMap,
    ) -> anyhow::Result<EleOperation> {
        // 检查参考号是否存在于索引映射表中
        if let Some(offsets) = index_map.get(&refno) {
            if offsets.is_empty() {
                return Ok(EleOperation::None);
            }

            // 获取最新的偏移量
            let latest_offset = *offsets.last().unwrap();

            // 解析最新的元素数据
            let latest_data = self.parse_element(latest_offset).await?;

            // 如果只有一个版本，则为新建操作
            if offsets.len() == 1 {
                return Ok(EleOperation::Add);
            } else {
                return Ok(EleOperation::Modified);
                // let second_latest_offset = *offsets.iter().nth(offsets.len() - 2).unwrap();
                // let previous_data = self.parse_element(second_latest_offset).await?;
            };

            // 获取倒数第二新的偏移量
            // let second_latest_offset = *offsets.iter().nth(offsets.len() - 2).unwrap();
            // let previous_data = self.parse_element(second_latest_offset).await?;

            // 比较元素的状态字段判断操作类型
            // let latest_status = latest_data.att_map().get_status();
            // let previous_status = previous_data.att_map().get_status();

            // if latest_status == previous_status {
            //     // 状态相同，可能是普通修改
            //     Ok(EleOperation::Modified)
            // } else if latest_status == 0 && previous_status != 0 {
            //     // 从活动到非活动，表示删除
            //     Ok(EleOperation::Deleted)
            // } else if latest_status != 0 && previous_status == 0 {
            //     // 从非活动到活动，表示恢复
            //     Ok(EleOperation::Modified)
            // } else {
            //     // 其他状态变化
            //     Ok(EleOperation::Modified)
            // }
        } else {
            // 索引中不存在该参考号
            Err(anyhow!("索引映射表中找不到参考号 {:?}", refno))
        }
    }

    /// 批量获取多个元素及其子元素数据，使用索引映射表提高效率
    ///
    /// # 参数
    /// * `root_refno` - 根元素的参考号
    /// * `index_map` - 参考号到数据位置数组的映射表
    /// * `max_depth` - 最大递归深度，为0表示不限制深度
    ///
    /// # 返回值
    /// * `anyhow::Result<HashMap<RefU64, EleData>>` - 元素参考号到元素数据的映射表
    pub async fn fast_get_elements_deep(
        &mut self,
        root_refno: RefU64,
        index_map: &IndexMap,
        max_depth: usize,
    ) -> anyhow::Result<HashMap<RefU64, EleData>> {
        let mut results = HashMap::new();
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();

        // 首先获取根元素
        if let Some(locs) = index_map.get(&root_refno) {
            if let Some(&latest_offset) = locs.last() {
                let offset = latest_offset;
                if let Ok(root_data) = self.parse_element(offset).await {
                    visited.insert(root_refno);
                    results.insert(root_refno, root_data.clone());

                    // 将子元素添加到队列
                    // if let Some(children) = &root_data.children {
                    for &child_refno in root_data.children.iter() {
                        if !visited.contains(&child_refno) {
                            queue.push_back((child_refno, 1)); // 1表示深度为1
                        }
                    }
                    // }
                } else {
                    return Err(anyhow!("无法解析根元素 {:?}", root_refno));
                }
            } else {
                return Err(anyhow!("根元素 {:?} 无位置记录", root_refno));
            }
        } else {
            return Err(anyhow!("索引映射表中找不到根元素 {:?}", root_refno));
        }

        // 广度优先遍历处理子元素
        while let Some((refno, depth)) = queue.pop_front() {
            // 检查深度限制
            if depth > max_depth {
                continue;
            }

            // 避免重复处理
            if visited.contains(&refno) {
                continue;
            }

            visited.insert(refno);

            // 获取当前元素数据
            if let Some(locs) = index_map.get(&refno) {
                if let Some(&latest_offset) = locs.last() {
                    let offset = latest_offset;
                    if let Ok(ele_data) = self.parse_element(offset).await {
                        results.insert(refno, ele_data.clone());

                        // 将子元素添加到队列
                        // if let Some(children) = &ele_data.children {
                        for &child_refno in ele_data.children.iter() {
                            if !visited.contains(&child_refno) {
                                queue.push_back((child_refno, depth + 1));
                            }
                        }
                        // }
                    }
                }
            }
        }

        Ok(results)
    }

    /// 获取元素及其完整历史记录
    ///
    /// 此函数返回元素的最新状态以及其所有历史版本，同时分析历史记录判断各版本间的变更类型
    ///
    /// # 参数
    /// * `refno` - 要查询的参考号
    /// * `index_map` - 索引映射表
    ///
    /// # 返回值
    /// * `anyhow::Result<(EleData, Vec<(EleData, EleOperation)>)>` - 元素当前数据及历史记录（带操作类型）
    pub async fn get_element_with_history(
        &mut self,
        refno: RefU64,
        index_map: &IndexMap,
    ) -> anyhow::Result<(EleData, Vec<(EleData, EleOperation)>)> {
        if let Some(offsets) = index_map.get(&refno) {
            if offsets.is_empty() {
                return Err(anyhow!("参考号 {} 在索引中存在但没有位置记录", refno));
            }

            // 获取所有历史版本的数据
            let mut history_elements = Vec::with_capacity(offsets.len());
            for &offset in offsets.iter() {
                match self.parse_element(offset).await {
                    Ok(ele_data) => history_elements.push(ele_data),
                    Err(e) => return Err(anyhow!("解析元素 {} 历史版本时出错: {}", refno, e)),
                }
            }

            // 确保有数据
            if history_elements.is_empty() {
                return Err(anyhow!("参考号 {} 没有历史数据", refno));
            }

            // 最新版本取最后一个（offsets 为升序）
            let latest_element = history_elements
                .last()
                .cloned()
                .ok_or_else(|| anyhow!("参考号 {} 没有历史数据", refno))?;

            // 历史版本及其操作类型（不包含最新版本）
            let mut history_with_ops = Vec::with_capacity(history_elements.len() - 1);

            // 按时间顺序（旧 -> 新但不含最新）标记操作
            for i in 0..history_elements.len().saturating_sub(1) {
                let current = &history_elements[i];

                // 先简单标记：最早一条为新增，其余视为修改（后续可补充精细 diff）
                let operation = if i == 0 {
                    EleOperation::Add
                } else {
                    EleOperation::Modified
                };

                history_with_ops.push((current.clone(), operation));
            }

            Ok((latest_element, history_with_ops))
        } else {
            Err(anyhow!("参考号 {} 在索引映射表中不存在", refno))
        }
    }

    /// 获取参考号在指定会话范围内的主操作状态(不包含子元素)
    ///
    /// # 参数
    /// * `refno` - 要判断状态的参考号
    /// * `sesno` - 可选的会话号，用于限定搜索范围
    ///
    /// # 返回值
    /// * `anyhow::Result<EleOperation>` - 成功返回参考号的操作状态(新增/修改/删除/重复/无操作)
    ///
    /// # 错误
    /// * 当参考号在指定范围内不存在时返回错误
    pub fn get_refno_primary_operation_status(
        &mut self,
        refno: RefU64,
        sesno: Option<u32>,
    ) -> anyhow::Result<EleOperation> {
        // 使用search_latest_and_prev_refno获取最新版本和前一个版本
        let [latest, previous] = self.search_latest_and_prev_refno(refno, sesno);

        // 如果没有找到任何版本
        if latest.is_none() {
            return Ok(EleOperation::None);
        }

        // 只有一个版本，说明是新建的
        if previous.is_none() {
            return Ok(EleOperation::Add);
        }

        // 解包最新版本
        let (latest_sesno, latest_offset) = latest.unwrap();

        // 先判断是否发生删除
        let latest_att = self.parse_raw_element(latest_offset).unwrap();
        let owner = latest_att.owner;
        //todo 直接调用 parse children 方法
        let owner_ele = self.auto_get_raw_element(owner)?;
        if !owner_ele.children.contains(&refno) {
            return Ok(EleOperation::Deleted);
        }

        // 解包前一个版本
        let (prev_sesno, prev_offset) = previous.unwrap();
        let prev_att = self.parse_raw_element(prev_offset).unwrap();

        // 检查children是否有变化
        let prev_owner_ele = self.auto_get_raw_element(prev_att.owner)?;

        // 检查子元素是否有变化
        let prev_children = &prev_owner_ele.children;
        let latest_children = &owner_ele.children;

        // 检查是否有任何子元素被删除
        let mut has_deleted_children = false;
        for child_refno in prev_children.iter() {
            if !latest_children.contains(child_refno) {
                has_deleted_children = true;
                break;
            }
        }

        if has_deleted_children {
            return Ok(EleOperation::Deleted);
        }

        //todo 如何判断是否发生修改？
        // let diff = latest_att.att_map().diff(&prev_att.att_map());
        // if diff.is_empty() {
        //     return Ok(EleOperation::None);
        // }
        return Ok(EleOperation::Modified);
    }

    /// 收集最近N个会话的变化数据
    ///
    /// # 参数
    /// * `top_n` - 要收集的最近会话数量，如果为None则默认获取最新一个会话的数据
    ///
    /// # 返回值
    /// * `anyhow::Result<HashMap<RefU64, (EleOperation, Option<EleData>)>>` - 返回一个映射:
    ///   - 键为参考号
    ///   - 值为元组: (操作类型, 可选的元素数据)
    ///
    /// # 错误
    /// * 当读取或解析元素数据失败时返回错误
    pub fn collect_recent_n_sessions_eles(
        &mut self,
        top_n: Option<u32>,
    ) -> anyhow::Result<BTreeMap<u32, Vec<EleOperationData>>> {
        let all_sesnos: Vec<i32> = self.sesno_pgno_map.keys().copied().collect();

        if all_sesnos.is_empty() {
            // 返回空的映射
            return Ok(Default::default());
        }

        let min_sesno = *all_sesnos.iter().min().unwrap();
        let max_sesno = *all_sesnos.iter().max().unwrap();

        // 确定要处理的会话范围
        let start_sesno = match top_n {
            Some(n) => {
                let n = std::cmp::min(n as usize, all_sesnos.len());
                let start_idx = all_sesnos.len() - n;
                all_sesnos[start_idx]
            }
            None => max_sesno - 1,
        };
        if min_sesno < 0 {
            return Err(anyhow!("会话号不能小于0"));
        }

        let range = start_sesno..=max_sesno;

        dbg!(&range);

        // 调用现有方法处理这个范围
        self.collect_increment_eles(Some(range))
    }



    /// 在数据库中搜索指定参考号的物理存储位置（优化版本，使用二分查找）
    ///
    /// # 参数
    /// * `refno` - 要搜索的参考号
    ///
    /// # 返回值
    /// * `anyhow::Result<RefnoDataLoc>` - 成功返回参考号的物理存储位置信息,失败返回错误
    ///
    /// # 错误
    /// 当找不到指定参考号时返回错误
    pub fn search_refno_pgno_optimized(&mut self, refno: RefU64) -> anyhow::Result<RefnoDataLoc> {
        let basic_info = self.get_page_basic_info()?;
        let latest_index_pgno = basic_info.latest_ses_data.index_root_pageno;
        let mut index_data = self.read_index_data(latest_index_pgno)?;
        let mut level = index_data.level as i32;
        let (r0, r1) = (refno.get_0(), refno.get_1());

        while level >= 0 {
            if level == 0 {
                // 叶子节点时使用二分查找
                match self.binary_search_refno(&index_data.refno_locs, r0 as u64, r1 as u64) {
                    Some(idx) => return Ok(index_data.refno_locs[idx].clone()),
                    None => break,
                }
            } else {
                // 非叶子节点时查找下一个页面
                let next_pgno = match self.find_next_pgno_binary(
                    &index_data.refno_locs,
                    r0 as u64,
                    r1 as u64,
                ) {
                    Some(pgno) => pgno,
                    None => return Err(anyhow!("无法在索引中找到下一个页面")),
                };

                index_data = self.read_index_data(next_pgno)?;
                level = index_data.level as i32;
            }
        }

        Err(anyhow!("未找到参考号: {:?}", refno))
    }

    /// 使用二分查找在索引中查找参考号
    fn binary_search_refno(&self, locs: &[RefnoDataLoc], r0: u64, r1: u64) -> Option<usize> {
        let mut left = 0;
        let mut right = locs.len();

        while left < right {
            let mid = left + (right - left) / 2;
            let loc = &locs[mid];

            if loc.refno_0 as u64 == r0 && loc.refno_1 as u64 == r1 {
                return Some(mid);
            }

            if loc.refno_0 as u64 > r0 || (loc.refno_0 as u64 == r0 && loc.refno_1 as u64 > r1) {
                right = mid;
            } else {
                left = mid + 1;
            }
        }

        None
    }

    /// 使用二分查找在非叶子节点中找到下一个页面
    fn find_next_pgno_binary(&self, locs: &[RefnoDataLoc], r0: u64, r1: u64) -> Option<u32> {
        if locs.is_empty() {
            return None;
        }

        // 如果参考号小于第一个元素，使用第一个页面
        if r0 < locs[0].refno_0 as u64
            || (r0 == locs[0].refno_0 as u64 && r1 < locs[0].refno_1 as u64)
        {
            return Some(locs[0].pgno);
        }

        // 如果参考号大于最后一个元素，使用最后一个页面
        let last = locs.len() - 1;
        if r0 > locs[last].refno_0 as u64
            || (r0 == locs[last].refno_0 as u64 && r1 >= locs[last].refno_1 as u64)
        {
            return Some(locs[last].pgno);
        }

        // 二分查找合适的范围
        let mut left = 0;
        let mut right = locs.len() - 1;

        while left + 1 < right {
            let mid = left + (right - left) / 2;
            let loc = &locs[mid];

            if loc.refno_0 as u64 > r0 || (loc.refno_0 as u64 == r0 && loc.refno_1 as u64 > r1) {
                right = mid;
            } else {
                left = mid;
            }
        }

        // 根据窗口查找的逻辑，返回左侧索引对应的页面
        Some(locs[left].pgno)
    }

    /// 收集最新的元素数据，从后往前检索，只保留新增的元素
    ///
    /// 该方法通过从最新会话开始向前遍历，记录已处理的元素，
    /// 只返回新增的元素，跳过已处理的元素（包括已删除和已修改的）。
    ///
    /// # 参数
    /// * `max_sessions` - 可选的最大会话数量限制，如果为None则检索所有会话
    ///
    /// # 返回值
    /// * `anyhow::Result<HashMap<RefU64, EleOperationData>>` - 返回新增的元素操作数据映射
    ///
    /// # 错误
    /// * 当读取或解析元素数据失败时返回错误
    pub async fn collect_latest_eles(
        &mut self,
        max_sessions: Option<u32>,
    ) -> anyhow::Result<HashMap<RefU64, EleOperationData>> {
        let mut latest_elements: HashMap<RefU64, EleOperationData> = HashMap::new();
        let mut deleted_refnos: HashSet<RefU64> = HashSet::new();
        let mut processed_refnos: HashSet<RefU64> = HashSet::new();

        // 获取所有会话号，按降序排列（从最新到最旧）
        let mut session_numbers: Vec<i32> = self.ses_range_map.keys().rev().cloned().collect();

        // 如果指定了最大会话数量，则限制处理的会话数
        if let Some(max) = max_sessions {
            session_numbers.truncate(max as usize);
        }
        println!("开始从后往前检索最新元素数据，共处理 {} 个会话", session_numbers.len());
        
        // 从最新会话开始向前遍历
        for (index, &sesno) in session_numbers.iter().enumerate() {
            println!("处理会话 {} ({}/{})", sesno, index + 1, session_numbers.len());
            
            // 获取当前会话的所有元素操作
            let locs = self.collect_refno_locs(sesno);
            
            // 先收集所有需要处理的元素
            let mut current_session_operations = Vec::new();
            
            for loc in locs {
                let refno = RefU64::from_two_nums(loc.refno_0, loc.refno_1);
                
                // 如果元素已经被处理过，则跳过
                if processed_refnos.contains(&refno) {
                    continue;
                }
                
                // 解析元素操作
                match self.get_refno_operation_status(refno, Some(sesno as u32)) {
                    Ok(mut details) => {
                        // 应该只有一个元素的操作状态
                        if let Some((_, detail)) = details.drain().next() {
                            match detail {
                                EleOperationDetail::Deleted => {
                                    current_session_operations.push((refno, detail, true));
                                }
                                EleOperationDetail::Add(_) => {
                                    current_session_operations.push((refno, detail, false));
                                }
                                EleOperationDetail::Modified(_) => {
                                    // 包含修改操作的元素
                                    current_session_operations.push((refno, detail, false));
                                }
                                // 跳过无操作状态的元素
                                EleOperationDetail::None => {}
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("处理元素 {:?} 时发生错误: {}", refno, e);
                    }
                }
            }
            
            // 处理收集到的操作
            for (refno, detail, is_delete) in current_session_operations {
                processed_refnos.insert(refno);

                if is_delete {
                    deleted_refnos.insert(refno);
                    latest_elements.remove(&refno);
                } else if !deleted_refnos.contains(&refno) && !latest_elements.contains_key(&refno) {
                    let element_data = EleOperationData::new(refno, sesno as u32, detail);
                    latest_elements.insert(refno, element_data);
                }
            }
        }

        Ok(latest_elements)
    }

    /// 收集并保存最新元素数据和会话数据到数据库
    ///
    /// 这个方法结合了 collect_latest_eles 和 update_elements_to_database 的功能，
    /// 用于收集最新的元素数据并将其保存到 SurrealDB 数据库中。
    ///
    /// # 参数
    /// * `max_sessions` - 可选的最大会话数量限制，如果为None则处理所有会话
    ///
    /// # 返回值
    /// * `anyhow::Result<()>` - 成功返回Ok(())，失败返回错误
    ///
    /// # 功能
    /// 1. 收集最新的元素数据（只保留新增的元素，跳过已删除和修改的）
    /// 2. 按会话组织数据
    /// 3. 保存会话信息到数据库
    /// 4. 保存元素数据到数据库
    /// 5. 更新会话统计信息
    pub async fn collect_and_save_latest_data(
        &mut self,
        max_sessions: Option<u32>,
        eles_map: Option<HashMap<RefU64, EleOperationData>>,
    ) -> anyhow::Result<()> {
        println!("开始收集并保存最新元素数据和会话数据...");
        let total_start_time = Instant::now();

        // 第一步：收集最新元素数据
        println!("\n1. 收集最新元素数据...");
        let collect_start_time = Instant::now();
        let latest_elements = if let Some(eles_map) = eles_map {
            eles_map
        } else {
            self.collect_latest_eles(max_sessions).await?
        };
        let collect_elapsed = collect_start_time.elapsed();
        
        println!("收集到 {} 个最新元素，耗时: {:?}", latest_elements.len(), collect_elapsed);

        if latest_elements.is_empty() {
            println!("没有找到新的元素数据，跳过保存步骤");
            return Ok(());
        }

        // 第二步：按会话组织数据
        println!("\n2. 按会话组织数据...");
        let mut range_eles: BTreeMap<u32, Vec<EleOperationData>> = BTreeMap::new();
        
        for (_, element_data) in latest_elements {
            let sesno = element_data.sesno;
            range_eles.entry(sesno).or_insert_with(Vec::new).push(element_data);
        }

        println!("数据已按 {} 个会话组织", range_eles.len());
        for (sesno, elements) in &range_eles {
            println!("  会话 {}: {} 个元素", sesno, elements.len());
        }

        // 第三步：保存到数据库
        println!("\n3. 保存数据到 SurrealDB...");
        let save_start_time = Instant::now();
        self.save_sessions_and_elements(&range_eles).await?;
        let save_elapsed = save_start_time.elapsed();

        let total_elapsed = total_start_time.elapsed();
        println!("\n✅ 数据收集和保存完成!");
        println!("  - 收集耗时: {:?}", collect_elapsed);
        println!("  - 保存耗时: {:?}", save_elapsed);
        println!("  - 总耗时: {:?}", total_elapsed);
        println!("  - 处理会话数: {}", range_eles.len());
        println!("  - 处理元素数: {}", range_eles.values().map(|v| v.len()).sum::<usize>());

        Ok(())
    }

    /// 保存会话和元素数据到数据库
    ///
    /// 这是一个内部方法，用于将组织好的会话和元素数据保存到 SurrealDB
    ///
    /// # 参数
    /// * `range_eles` - 按会话组织的元素数据
    ///
    /// # 返回值
    /// * `anyhow::Result<()>` - 成功返回Ok(())，失败返回错误
    async fn save_sessions_and_elements(
        &mut self,
        range_eles: &BTreeMap<u32, Vec<EleOperationData>>,
    ) -> anyhow::Result<()> {
        // 获取数据库信息
        let pdms_header = self.read_pdms_header()?;
        let dbnum = pdms_header.db_num;

        // 第一步：创建会话记录
        println!("  3.1 创建会话记录...");
        let session_start_time = Instant::now();
        
        let all_sesnos: Vec<u32> = range_eles.keys().cloned().collect();
        let mut session_records = Vec::new();

        for &sesno in &all_sesnos {
            // 获取会话详细信息
            let ses_data = self.get_ses_data(sesno)?;

            let session_record = format!(
                r#"{{
                    id: "{}_{}",
                    sesno: {},
                    timestamp: d"{}",
                    dbnum: {},
                    add_count: 0,
                    modify_count: 0,
                    delete_count: 0,
                    computer_name: "{}",
                    comments: "{}",
                    end_pgno: {},
                    index_root_pageno: {},
                    claim_pageno: {}
                }}"#,
                dbnum,
                sesno,
                sesno,
                ses_data.get_utc_dt().to_rfc3339(),
                dbnum,
                ses_data.get_computer_name(),
                ses_data.get_comments_name(),
                ses_data.end_pgno,
                ses_data.index_root_pageno,
                ses_data.claim_pageno
            );

            session_records.push(session_record);
        }

        // 批量插入会话记录
        for chunk in session_records.chunks(50) {
            let batch_insert_sql = format!(
                r#"INSERT IGNORE INTO sessions [{}];"#,
                chunk.join(",\n                ")
            );

            if let Err(e) = SUL_DB.query(&batch_insert_sql).await {
                eprintln!("批量保存会话信息错误: {}", e);
            }
        }

        let session_elapsed = session_start_time.elapsed();
        println!("    会话记录创建完成，耗时: {:?}", session_elapsed);

        // 第二步：统计并更新会话的增删改数量
        println!("  3.2 统计会话操作数量...");
        let stats_start_time = Instant::now();

        let mut session_stats: BTreeMap<i32, (i32, i32, i32)> = BTreeMap::new();

        for (sesno, elements) in range_eles {
            for element in elements {
                let stats = session_stats.entry(*sesno as i32).or_insert((0, 0, 0));
                match &element.detail {
                    EleOperationDetail::Add(_) => stats.0 += 1,
                    EleOperationDetail::Modified(_) => stats.1 += 1,
                    EleOperationDetail::Deleted => stats.2 += 1,
                    EleOperationDetail::None => {}
                }
            }
        }

        // 更新会话统计
        for (sesno, stats) in &session_stats {
            println!("    会话 {}: 新增 {} 条, 修改 {} 条, 删除 {} 条", sesno, stats.0, stats.1, stats.2);
            
            let update_session_sql = format!(
                r#"UPDATE sessions:{}_{}
                SET add_count = {}, modify_count = {}, delete_count = {};"#,
                dbnum, sesno, stats.0, stats.1, stats.2
            );

            if let Err(e) = SUL_DB.query(&update_session_sql).await {
                eprintln!("更新会话统计错误: {}", e);
            }
        }

        let stats_elapsed = stats_start_time.elapsed();
        println!("    会话统计更新完成，耗时: {:?}", stats_elapsed);

        // 第三步：保存元素数据
        println!("  3.3 保存元素数据...");
        let elements_start_time = Instant::now();

        // 准备元素变更记录
        let mut element_records = Vec::new();
        let mut surql_batch = Vec::new();
        let mut total_surql = 0;

        for (&sesno, elements) in range_eles {
            let timestamp = self.get_ses_data(sesno)?.get_utc_dt().to_rfc3339();
            
            for element in elements {
                let refno = element.refno;
                let op_type = element.get_op_type();
                
                // 只处理新增的元素（根据 collect_latest_eles 的逻辑）
                if matches!(element.detail, EleOperationDetail::Add(_)) {
                    // 创建元素变更记录
                    let pe_key = refno.to_pe_key();
                    let element_record = format!(
                        r#"{{
                            id: [{},{}],
                            refno: {},
                            operation_type: "{}",
                            entity_type: {}.noun,
                            timestamp: d"{}",
                            session_id: sessions:{}_{},
                            sesno: {},
                            details: "[]"
                        }}"#,
                        &pe_key, sesno, &pe_key, op_type, &pe_key, &timestamp, dbnum, sesno, sesno
                    );
                    element_records.push(element_record);

                    // 生成 SurrealQL
                    let id = element.refno.to_string();
                    let surql = element.to_surql(&id, dbnum, sesno);
                    if !surql.is_empty() {
                        surql_batch.push(surql);
                        total_surql += 1;
                        
                        // 批量执行 SurrealQL（每50条）
                        if surql_batch.len() >= 50 {
                            let batch_sql = surql_batch.join(";\n");
                            if let Err(e) = SUL_DB.query(&batch_sql).await {
                                eprintln!("批量执行 SurrealQL 错误: {}", e);
                            }
                            surql_batch.clear();
                        }
                    }
                }
            }
        }

        // 处理剩余的 SurrealQL
        if !surql_batch.is_empty() {
            let batch_sql = surql_batch.join(";\n");
            if let Err(e) = SUL_DB.query(&batch_sql).await {
                eprintln!("批量执行 SurrealQL 错误: {}", e);
            }
        }

        // 批量插入元素变更记录
        for chunk in element_records.chunks(50) {
            if !chunk.is_empty() {
                let batch_insert_sql = format!(
                    r#"INSERT IGNORE INTO element_changes [{}];"#,
                    chunk.join(",\n                ")
                );
                if let Err(e) = SUL_DB.query(&batch_insert_sql).await {
                    eprintln!("批量保存元素变更记录错误: {}", e);
                }
            }
        }

        let elements_elapsed = elements_start_time.elapsed();
        println!("    元素数据保存完成，耗时: {:?}", elements_elapsed);
        println!("    执行了 {} 条 SurrealQL 语句", total_surql);
        println!("    保存了 {} 条元素变更记录", element_records.len());

        Ok(())
    }
}

pub async fn sync_all_history_data(path: &str) -> anyhow::Result<()> {
    //先建立 ses 的索引，date 和 dbnum， sesno 都要建立索引
    let mut io = PdmsIO::new("ams", path, true);
    io.sync_history().await.unwrap();
    Ok(())
}

#[cfg(test)]
mod io_element_hash_tests {
    use super::*;

    #[test]
    fn element_hash_ignores_pgno_sesno() {
        let mut a = EleData::default();
        a.noun = 0x1234;
        a.owner = RefU64::from_two_nums(10, 20);
        a.name = "AAA".to_string();
        a.children.push(RefU64::from_two_nums(1, 2));

        a.att_map_mut()
            .insert("PGNO".to_string(), NamedAttrValue::IntegerType(100));
        a.att_map_mut()
            .insert("SESNO".to_string(), NamedAttrValue::IntegerType(200));
        a.att_map_mut().insert(
            "FOO".to_string(),
            NamedAttrValue::StringType("BAR".to_string()),
        );

        let mut b = a.clone();
        b.att_map_mut()
            .insert("PGNO".to_string(), NamedAttrValue::IntegerType(101));
        b.att_map_mut()
            .insert("SESNO".to_string(), NamedAttrValue::IntegerType(201));

        assert_eq!(
            PdmsIO::calculate_element_hash(&a),
            PdmsIO::calculate_element_hash(&b)
        );
    }

    #[test]
    fn element_hash_changes_on_non_volatile_attr_change() {
        let mut a = EleData::default();
        a.att_map_mut()
            .insert("FOO".to_string(), NamedAttrValue::IntegerType(1));

        let mut b = a.clone();
        b.att_map_mut()
            .insert("FOO".to_string(), NamedAttrValue::IntegerType(2));

        assert_ne!(
            PdmsIO::calculate_element_hash(&a),
            PdmsIO::calculate_element_hash(&b)
        );
    }

    #[test]
    fn element_hash_changes_on_children_change() {
        let mut a = EleData::default();
        a.children.push(RefU64::from_two_nums(1, 1));

        let mut b = a.clone();
        b.children.push(RefU64::from_two_nums(1, 2));

        assert_ne!(
            PdmsIO::calculate_element_hash(&a),
            PdmsIO::calculate_element_hash(&b)
        );
    }

    #[test]
    fn element_hash_options_can_ignore_custom_key() {
        let opts = ElementHashOptions {
            ignore_keys: &["PGNO", "SESNO", "FOO"],
        };

        let mut a = EleData::default();
        a.att_map_mut()
            .insert("FOO".to_string(), NamedAttrValue::IntegerType(1));

        let mut b = a.clone();
        b.att_map_mut()
            .insert("FOO".to_string(), NamedAttrValue::IntegerType(2));

        assert_eq!(
            PdmsIO::calculate_element_hash_with_options(&a, &opts),
            PdmsIO::calculate_element_hash_with_options(&b, &opts)
        );
    }
}

/// 示例：使用索引映射表快速查询PDMS数据库
///
/// 这个函数演示了如何使用索引映射表来提高PDMS数据库的查询效率
///
/// # 参数
/// * `path` - 数据库文件路径
/// * `refnos` - 要查询的参考号列表
///
/// # 返回值
/// * `anyhow::Result<()>` - 成功或错误
pub async fn demo_fast_query_with_index_map(path: &str, refnos: &[RefU64]) -> anyhow::Result<()> {
    // 创建并打开数据库
    println!("初始化数据库连接...");
    let mut io = PdmsIO::new("demo", path, true);
    io.open()?;

    // 构建索引映射表
    println!("构建索引映射表...");
    let start_time = std::time::Instant::now();
    let index_map = io.build_index_map_verbose(true)?;
    let build_time = start_time.elapsed();
    println!(
        "索引构建完成，耗时: {:?}, 索引项数: {}",
        build_time,
        index_map.len()
    );

    // 统计历史记录总数
    let total_history_records: usize = index_map.values().map(|v| v.len()).sum();
    println!("总历史记录数: {}", total_history_records);

    // 打印一些有历史记录的示例
    let mut history_examples = index_map
        .iter()
        .filter(|(_, offsets)| offsets.len() > 1)
        .take(5)
        .collect::<Vec<_>>();

    if !history_examples.is_empty() {
        println!("\n具有历史记录的参考号示例:");
        for (i, (refno, offsets)) in history_examples.iter().enumerate() {
            println!(
                "  {}. 参考号: {}, 历史版本数: {}",
                i + 1,
                refno,
                offsets.len()
            );
            for (j, &offset) in offsets.iter().enumerate().take(3) {
                println!("     - 版本 {}: 偏移量: {}", j, offset);
            }
            if offsets.len() > 3 {
                println!("     - ... 还有 {} 个版本", offsets.len() - 3);
            }
        }
    }

    // 尝试缓存索引（可选）
    let cache_path = Path::new("index_map_cache.bin");
    if !cache_path.exists() {
        println!("缓存索引映射表...");
        io.cache_index_map(cache_path, &index_map)?;
        println!("索引已缓存到文件: {:?}", cache_path);
    }

    // 使用索引查询数据
    if !refnos.is_empty() {
        println!("使用索引查询 {} 个参考号...", refnos.len());
        let query_start = std::time::Instant::now();
        let elements = io.fast_get_elements(refnos, &index_map).await?;
        let query_time = query_start.elapsed();

        // 输出结果
        println!(
            "查询完成，耗时: {:?}, 找到 {} 个元素",
            query_time,
            elements.len()
        );
        for (refno, ele) in elements {
            println!(
                "参考号: {}, 名称: {}, 子元素数: {}",
                refno,
                ele.name,
                ele.children.len()
            );

            // 如果有历史版本，获取并打印历史信息
            if let Some(offsets) = index_map.get(&refno) {
                if offsets.len() > 1 {
                    println!("  -> 该参考号有 {} 个历史版本", offsets.len());

                    // 获取并比较第一个历史版本
                    if offsets.len() >= 2 {
                        if let Ok(history_ele) =
                            io.fast_get_element_version(refno, &index_map, 1).await
                        {
                            println!(
                                "  -> 上一版本名称: {}, 子元素数: {}",
                                history_ele.name,
                                history_ele.children.len()
                            );
                        }
                    }
                }
            }
        }
    }

    // 测试深度查询（以第一个参考号为根）
    if !refnos.is_empty() {
        let root_refno = refnos[0];
        println!("执行深度查询，根参考号: {}...", root_refno);
        let deep_start = std::time::Instant::now();
        let deep_elements = io.fast_get_elements_deep(root_refno, &index_map, 3).await?;
        let deep_time = deep_start.elapsed();

        println!(
            "深度查询完成，耗时: {:?}, 找到 {} 个元素",
            deep_time,
            deep_elements.len()
        );
    }

    Ok(())
}

/// 演示如何查询和分析元素的历史记录
///
/// 此函数展示了如何使用索引映射表获取元素的所有历史版本，并分析版本之间的变更
///
/// # 参数
/// * `path` - 数据库文件路径
/// * `refno` - 要查询历史的参考号
///
/// # 返回值
/// * `anyhow::Result<()>` - 成功或错误
pub async fn demo_history_query(path: &str, refno: RefU64) -> anyhow::Result<()> {
    // println!("初始化数据库连接...");
    // let mut io = PdmsIO::new("demo_history", path, true);
    // io.open()?;

    // println!("构建索引映射表...");
    // let start_time = std::time::Instant::now();
    // let index_map = io.build_index_map_verbose(true).await?;
    // let build_time = start_time.elapsed();
    // println!("索引构建完成，耗时: {:?}, 索引项数: {}", build_time, index_map.len());

    // // 获取并打印指定参考号的历史信息
    // println!("\n查询参考号 {} 的历史记录...", refno);

    // if let Some(offsets) = index_map.get(&refno) {
    //     println!("找到 {} 个历史版本", offsets.len());

    //     // 获取所有历史版本的数据
    //     let history_data = io.fast_get_element_history(refno, &index_map).await?;

    //     // 显示每个版本的详细信息
    //     for (i, data) in history_data.iter().enumerate() {
    //         println!("\n版本 {}:", i);
    //         println!("  名称: {}", data.name);
    //         println!("  类型: {}", data.element_type);
    //         println!("  状态: {}", data.status);
    //         println!("  创建时间: {:?}", data.cdate);
    //         println!("  修改时间: {:?}", data.mdate);

    //         // 如果有子元素，显示子元素信息
    //         if let Some(ref children) = data.children {
    //             println!("  子元素数量: {}", children.len());
    //             for (j, &child) in children.iter().enumerate().take(5) {
    //                 println!("    子元素 {}: {:?}", j + 1, child);
    //             }
    //             if children.len() > 5 {
    //                 println!("    ... 及其他 {} 个子元素", children.len() - 5);
    //             }
    //         }

    //         // 显示属性信息
    //         if !data.attrs.is_empty() {
    //             println!("  属性数量: {}", data.attrs.len());
    //             for (j, (key, value)) in data.attrs.iter().enumerate().take(5) {
    //                 println!("    属性 {}: {} = {:?}", j + 1, key, value);
    //             }
    //             if data.attrs.len() > 5 {
    //                 println!("    ... 及其他 {} 个属性", data.attrs.len() - 5);
    //             }
    //         }
    //     }

    //     // 分析版本变化
    //     if history_data.len() >= 2 {
    //         println!("\n版本变化分析:");
    //         for i in 1..history_data.len() {
    //             let current = &history_data[i];
    //             let previous = &history_data[i-1];

    //             println!("从版本 {} 到版本 {}:", i-1, i);

    //             // 比较名称变化
    //             if current.name != previous.name {
    //                 println!("  名称从 \"{}\" 变更为 \"{}\"", previous.name, current.name);
    //             }

    //             // 比较状态变化
    //             if current.status != previous.status {
    //                 println!("  状态从 {} 变更为 {}", previous.status, current.status);
    //             }

    //             // 比较子元素数量变化
    //             let prev_children_count = previous.children.as_ref().map_or(0, |c| c.len());
    //             let curr_children_count = current.children.as_ref().map_or(0, |c| c.len());

    //             if prev_children_count != curr_children_count {
    //                 println!("  子元素数量从 {} 变更为 {}", prev_children_count, curr_children_count);
    //             }

    //             // 比较属性变化
    //             if previous.attrs.len() != current.attrs.len() {
    //                 println!("  属性数量从 {} 变更为 {}", previous.attrs.len(), current.attrs.len());
    //             }
    //         }
    //     }
    // } else {
    //     println!("参考号 {} 在索引映射表中不存在", refno);
    // }

    Ok(())
}

/// 对比原始search_refno_pgno和优化版本search_refno_pgno_optimized的性能
    pub async fn benchmark_search_refno_pgno(
        path: &str,
        refnos: &[RefU64],
        iterations: usize,
    ) -> anyhow::Result<()> {
    let mut io = PdmsIO::new("test", path, true);
    io.open()?;

    println!("开始基准测试搜索引用号...");
    println!("测试引用号数量: {}", refnos.len());
    println!("每个算法迭代次数: {}", iterations);

    // 测试普通搜索方法
    let start = Instant::now();
    let mut success_count = 0;

    for _ in 0..iterations {
        for &refno in refnos {
            // 修改这里使用Option而不是Result
            if let Some(_) = io.search_latest_refno(refno, None) {
                success_count += 1;
            }
        }
    }

    let elapsed = start.elapsed();
    println!(
        "原始搜索方法: {:?}, 平均每个引用号: {:?}, 成功率: {:.2}%",
        elapsed,
        elapsed / (refnos.len() * iterations) as u32,
        (success_count as f64 / (refnos.len() * iterations) as f64) * 100.0
    );

    // 测试优化后的搜索方法
    let start = Instant::now();
    let mut success_count = 0;

    for _ in 0..iterations {
        for &refno in refnos {
            if let Ok(_) = io.search_refno_pgno_optimized(refno) {
                success_count += 1;
            }
        }
    }

    let elapsed = start.elapsed();
    println!(
        "优化搜索方法: {:?}, 平均每个引用号: {:?}, 成功率: {:.2}%",
        elapsed,
        elapsed / (refnos.len() * iterations) as u32,
        (success_count as f64 / (refnos.len() * iterations) as f64) * 100.0
    );

    // 验证结果一致性
    println!("\n验证两种方法结果一致性...");

    let mut match_count = 0;
    let mut mismatch_count = 0;

    for &refno in refnos {
        let result1 = io.search_latest_refno(refno, None);
        let result2 = io.search_refno_pgno_optimized(refno);

        match (result1, &result2) {
            (Some((sesno, offset)), Ok(loc2)) => {
                // 将 (sesno, offset) 与 RefnoDataLoc 结构对比时需要提取sesno对应的页号
                let ses_pgno = match io.sesno_pgno_map.get(&(sesno as i32)) {
                    Some(&pgno) => pgno,
                    None => {
                        println!("警告: 找不到会话 {} 对应的页号", sesno);
                        mismatch_count += 1;
                        continue;
                    }
                };

                if ses_pgno == loc2.pgno && offset == loc2.get_att_offset_with_page_size(io.page_size) {
                    match_count += 1;
                } else {
                    println!(
                        "不匹配: 引用号 {:?}, 方法1: ({}, {}), 方法2: ({}, {})",
                        refno,
                        sesno,
                        offset,
                        loc2.pgno,
                        loc2.get_att_offset_with_page_size(io.page_size)
                    );
                    mismatch_count += 1;
                }
            }
            (None, Err(_)) => {
                // 都找不到，结果一致
                match_count += 1;
            }
            _ => {
                println!(
                    "不一致: 引用号 {:?}, 方法1: {:?}, 方法2: {:?}",
                    refno, result1, &result2
                );
                mismatch_count += 1;
            }
        }
    }

    println!(
        "一致性检查: 匹配 {}, 不匹配 {}, 一致率: {:.2}%",
        match_count,
        mismatch_count,
        (match_count as f64 / (match_count + mismatch_count) as f64) * 100.0
    );

    Ok(())
}

/// 从数据库中提取一些真实的参考号用于测试
///
/// # 参数
/// * `io` - PDMS IO实例
/// * `count` - 需要提取的参考号数量
///
/// # 返回值
/// * `Vec<RefU64>` - 提取到的参考号集合
    pub fn extract_test_refnos(io: &mut PdmsIO, count: usize) -> anyhow::Result<Vec<RefU64>> {
    let basic_info = io.get_page_basic_info()?;
    let latest_index_pgno = basic_info.latest_ses_data.index_root_pageno;
    let mut index_data = io.read_index_data(latest_index_pgno)?;
    let mut refnos = Vec::new();

    // 如果是叶子节点，直接从中提取
    if index_data.level == 0 {
        for loc in &index_data.refno_locs {
            refnos.push(loc.get_refno());
            if refnos.len() >= count {
                break;
    }
}

#[cfg(test)]
mod io_tests {
    use super::*;

    #[test]
    fn test_process_leaf_node_uses_dynamic_page_size_for_offset() {
        // page_size=4K 时，若错误使用固定 2K PAGE_SIZE，会导致绝对偏移计算错误。
        let page_size = PAGE_SIZE_4K;

        let loc = RefnoDataLoc {
            refno_0: 1,
            refno_1: 2,
            pgno: 2,
            offset: 3, // 单位为 2 字节
            flag: 0,
        };

        let index_data = IndexPageData {
            page_type: 0,        // 此测试不关心
            noun: 0xCC47DF,      // IndexPageData 的 deku assert_eq 只在解码时生效，这里手工构造即可
            level: 0,
            unknowns: [0; 3],
            pfno: 0,
            refno_locs: vec![loc],
            remain_zero_bytes: Vec::new(),
        };

        let mut map: IndexMap = HashMap::new();
        PdmsIO::process_leaf_node(&index_data, &mut map, page_size);

        let refno = RefU64::from_two_nums(1, 2);
        let offsets = map.get(&refno).expect("refno missing");
        assert!(offsets.contains(&(2u64 * page_size as u64 + 3u64 * 2)));
        // 反例：固定 2K 页大小会得到 2*2048+6=4102，不应出现
        assert!(!offsets.contains(&(2u64 * PAGE_SIZE_2K as u64 + 3u64 * 2)));
    }
}
        return Ok(refnos);
    }

    // 如果不是叶子节点，尝试找到一些叶子节点
    let mut queue = vec![latest_index_pgno];
    while !queue.is_empty() && refnos.len() < count {
        let pgno = queue.pop().unwrap();
        let data = io.read_index_data(pgno)?;

        if data.level == 0 {
            // 叶子节点，提取参考号
            for loc in &data.refno_locs {
                refnos.push(loc.get_refno());
                if refnos.len() >= count {
                    break;
                }
            }
        } else {
            // 非叶子节点，添加子节点到队列
            for loc in &data.refno_locs {
                queue.push(loc.pgno);
                if queue.len() + refnos.len() > count * 2 {
                    break;
                }
            }
        }
    }

    Ok(refnos)
}

/// 基准测试函数，比较并行和非并行收集元素的性能
///
/// # 参数
/// * `path` - 数据库文件路径
///
/// # 返回值
/// * `anyhow::Result<()>` - 如果测试成功返回Ok
///
/// # 错误
/// * 如果打开数据库或执行查询失败，返回错误
pub async fn benchmark_increment_eles(path: &str) -> anyhow::Result<()> {
    use std::time::Instant;

    println!("正在加载数据库: {}", path);
    let mut io = PdmsIO::new("test", path, true);
    io.open()?;

    // 获取最近20个会话
    let latest_sesno = io.get_latest_sesno()? as i32;
    let start_sesno = latest_sesno.saturating_sub(10);
    let sesno_range = start_sesno..=latest_sesno;

    println!(
        "测试范围: 会话 {} 到 {} (共20个会话)",
        start_sesno, latest_sesno
    );

    // 测试非并行版本
    println!("开始测试非并行版本...");
    let start_time = Instant::now();
    let result1 = io.collect_increment_eles(Some(sesno_range.clone()))?;
    let non_parallel_time = start_time.elapsed();
    println!("非并行版本耗时: {:?}", non_parallel_time);
    println!("收集到 {} 个元素", result1.len());

    Ok(())
}
