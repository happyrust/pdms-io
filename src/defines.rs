use aios_core::pdms_types::EleOperation;
use aios_core::tool::db_tool::decode_chars_data;
use aios_core::RefU64;
use chrono::{DateTime, Local, MappedLocalTime, TimeZone, Utc};
use deku::bitvec::*;
use deku::ctx::Endian;
use deku::prelude::*;
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::str::FromStr;

pub const PAGE_SIZE: usize = 0x800;

#[derive(Default, Clone, Debug, PartialEq, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct PdmsHeader {
    // 开头两个未知 (0x00 - 0x07)
    pub unknown_0: [i32; 2],
    // 数据库编号 (0x08 - 0x0B)
    pub db_num: i32,
    // 然后是 00 00 00 01 (0x0C - 0x1F)
    pub unknown_1: [i32; 5],
    // 名词 (0x20 - 0x23)
    pub noun: i32,
    // 0xFF FF FF FF (0x24 - 0x27)
    pub unknown_2: i32,
    // 最新会话页号 (0x28 - 0x2B)
    pub latest_ses_pgno: u32,
    // 扩展号 (0x2C - 0x2F)
    pub ext_no: u32,
}

/// 数据库页面基本信息
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DbPageBasicInfo {
    /// PDMS 文件头信息
    pub pdms_header: PdmsHeader,
    /// 最新会话页号
    pub latest_ses_pageno: u32,
    /// 最新会话数据
    pub latest_ses_data: SessionPageData,
    /// 文件大小,用于实现增量更新
    pub file_size: u64,
}

///会话层的定位信息
#[derive(Default, Clone, Debug, PartialEq, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct SessionPageData {
    // 页号，不在文件中存储
    #[deku(skip, default = "0")]
    pub pgno: usize,
    // 页面类型 (0x00 - 0x03)
    pub page_type: i32,
    // 上一个会话页号 (0x04 - 0x07)
    pub last_ses_pageno: i32,
    // 上一个会话扩展号 (0x08 - 0x0B)
    pub last_ses_extno: i32,
    // 会话编号 (0x0C - 0x0F)
    pub sesno: i32,
    // 未知数据，固定为 0xFFFFFFFF (0x10 - 0x13)
    pub unknown_0: i32,

    // 会话最后一页的页号 (0x14 - 0x17)
    pub end_pgno: u32,
    // 会话最后一页的扩展号 (0x18 - 0x1B)
    pub end_extno: u32,

    // 索引根页号 (0x1C - 0x1F)
    pub index_root_pageno: u32,
    // 索引根扩展号 (0x20 - 0x23)
    pub index_root_extno: u32,
    // 声明页号 (0x24 - 0x27)
    pub claim_pageno: u32,
    // 声明扩展号 (0x28 - 0x2B)
    pub claim_extno: u32,

    // 未知数据1 (0x2C - 0x2F)
    pub unknown_1: i32,
    // 未知数据2 (0x30 - 0x33)
    pub unknown_2: i32,

    // 年份 (0x34 - 0x37)
    pub year: u32,
    // 月份 (0x38 - 0x3B)
    pub month: u32,
    // 小时数 (0x3C - 0x3F)
    pub hours: u32,
    // 秒数 (0x40 - 0x43)
    pub seconds: u32,

    // 13个未知的32位整数 (0x44 - 0x77)
    pub unknown_u32: [i32; 13],
    // 计算机名称长度，以4字节为单位 (0x78 - 0x7B)
    pub name_words_len: u32,
    // 计算机名称字节数组 (0x7C - )
    #[deku(count = "name_words_len * 4")]
    pub name_bytes: Vec<u8>,
    // 填充字节，使名称总长度为36字节
    #[deku(count = "(9 - name_words_len) * 4")]
    pub empty_bytes: Vec<u8>,

    // 注释长度，以4字节为单位
    pub comments_words_len: u32,
    // 注释内容字节数组
    #[deku(count = "comments_words_len * 4")]
    pub comments_bytes: Vec<u8>,

    // 剩余的字节数据，每8字节一组
    #[deku(count = "deku::rest.len()/8")]
    pub remain_bytes: Vec<u8>,
}

impl SessionPageData {
    #[inline]
    pub fn get_id(&self, dbnum: i32) -> [i32; 2] {
        [dbnum, self.sesno]
    }

    /// 获取指定参考号在当前会话中的操作状态
    ///
    /// 判断参考号在当前会话中的状态是增加、删除还是修改
    pub fn get_refno_status(&self, _refno: RefU64) -> EleOperation {
        // 默认情况下，如果参考号存在于当前会话，我们认为它是被添加的
        // 具体的状态判断需要比较前后会话的数据变化
        // 在实际情况中，我们需要查看这个会话的所有操作来确定

        // 这个方法保留在SessionPageData中，但实际上不会被调用
        // 实际的状态判断逻辑已经转移到了PdmsIO::get_refno_status方法中
        EleOperation::Add
    }

    pub fn gen_sur_json(&self, dbnum: i32) -> String {
        //id 需要拿 sesno 和 dbnum 组合？还是和文件名组合？
        let id = self.get_id(dbnum);
        let json = serde_json::json!({
            "id": id,
            "sesno": self.sesno,
            "pgno": self.pgno,
            "dbnum": dbnum,
            "index_pgno": self.index_root_pageno,
            "claim_pgno": self.claim_pageno,
            "end_pgno": self.end_pgno,
            "computer_name": self.get_computer_name(),
            "comments": self.get_comments_name(),
            "date": self.get_dt().to_rfc3339(),
        });
        json.to_string()
    }

    #[inline]
    pub fn get_dt(&self) -> DateTime<Utc> {
        let year = self.year;
        let month = self.month;
        let days = self.hours / 24;
        let hours = self.hours % 24;
        let minutes = self.seconds / 60;
        let seconds = self.seconds % 60;
        Local
            .with_ymd_and_hms(
                year as i32,
                month as u32,
                days,
                hours as u32,
                minutes,
                seconds,
            )
            .unwrap()
            .into()
    }

    #[inline]
    pub fn get_utc_dt(&self) -> DateTime<Utc> {
        self.get_dt()
    }

    #[inline]
    pub fn get_computer_name(&self) -> String {
        if self.name_words_len == 0 {
            return String::new();
        }
        //去掉后面为 0 的 bytes
        let i = (self.name_words_len as usize - 1) * 4;
        // dbg!(&self.name_bytes[i as usize..]);
        let rpos = self.name_bytes[i..]
            .into_iter()
            .rev()
            .position(|&x| x != 0)
            .unwrap_or(0);
        // dbg!(rpos);
        decode_chars_data(&self.name_bytes[..(i + 4 - rpos)]).0
    }

    #[inline]
    pub fn get_comments_name(&self) -> String {
        if self.comments_words_len == 0 {
            return String::new();
        }
        //去掉后面为 0 的 bytes
        let i = (self.comments_words_len as usize - 1) * 4;
        // dbg!(&self.comments_bytes[i as usize..]);
        let rpos = self.comments_bytes[i..]
            .into_iter()
            .rev()
            .position(|&x| x != 0)
            .unwrap_or(0);
        // dbg!(rpos);
        decode_chars_data(&self.comments_bytes[..(i + 4 - rpos)]).0
    }

    //是否需要要检测有无变化？先拿到最新的数据试试看里面的参考号，和之前的比有无变化
    pub fn get_session_saved_refnos() {}
}

///内含有的几个index part，名称表等等
#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
#[deku(endian = "big")]
pub struct SesIndexesData {
    #[deku(assert_eq = "0x3")]
    pub page_type: i32,
    pub last_ses_pageno: u32,
    pub last_ses_extno: u32,
    pub sesno: i32,
    pub unknown_0: i32, // 0xFF FF FF FF

    pub claim_data_pageno: u32,
    pub claim_data_extno: u32,

    pub index_root_pageno: u32,
    pub index_root_extno: u32,
    pub claim_root_pageno: u32,
    pub claim_root_extno: u32,
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
#[deku(endian = "big")]
pub struct RefnoIndexPgId {
    pub refno_0: u32,
    pub refno_1: u32,
    pub page_no: u32,
    pub ext_no: u32,
}

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
pub struct RootIndexPage {
    #[deku(endian = "big")]
    pub page_type: i32,
    #[deku(endian = "big")]
    pub noun: i32,
    //00 00 00 02 00 00 00 02 00 00 00 02 00 00 00 00
    #[deku(endian = "big")]
    pub unknowns_0: [i32; 4],
    //00 00 01 ED
    #[deku(endian = "big")]
    pub residual_num: u32, //要用0x200 - residual_num 得到剩余的值
    //80 00 00 01 80 00 00 01
    #[deku(endian = "big")]
    pub lock: [i32; 2], //可能是lock

    #[deku(endian = "big")]
    pub last_pageno: u32,
    #[deku(endian = "big")]
    pub last_extno: u32,

    pub lower_root: RefnoIndexPgId,
    pub upper_root: RefnoIndexPgId,
}

///Index 里的数据条目
/// 参考号数据位置结构体
///
/// 用于存储PDMS数据库中元素的参考号和其对应的物理存储位置信息
#[derive(Debug, PartialEq, DekuRead, DekuWrite, Clone)]
#[deku(endian = "big")]
pub struct RefnoDataLoc {
    /// 参考号的高32位
    pub refno_0: u32,
    /// 参考号的低32位
    pub refno_1: u32,
    /// 页号
    pub pgno: u32,
    /// 页内偏移量,占20位
    #[deku(bits = "20")]
    pub offset: u32,
    /// 标志位,占12位
    #[deku(bits = "12")]
    pub flag: u16,
}

impl RefnoDataLoc {
    /// 是否是起始页
    #[inline]
    pub fn is_start_page(&self) -> bool {
        self.refno_0 == 0x80000001 && self.refno_1 == 0x80000001
    }

    /// 获取完整的参考号
    ///
    /// 将高32位和低32位组合成完整的参考号
    #[inline]
    pub fn get_refno(&self) -> RefU64 {
        RefU64::from_two_nums(self.refno_0, self.refno_1)
    }

    /// 获取属性数据的实际偏移量
    ///
    /// 根据页号和页内偏移量计算出实际的字节偏移量
    #[inline]
    pub fn get_att_offset(&self) -> u64 {
        self.pgno as u64 * 0x800 + self.offset as u64 * 2
    }
}

/// PDMS数据库中的参考号索引页结构
///
/// 用于存储参考号索引的页面数据结构
#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
pub struct RefnoIndexPage {
    /// 页面类型标识
    #[deku(endian = "big")]
    pub page_type: i32,

    /// 页面标识符
    #[deku(endian = "big")]
    pub noun: i32,

    /// 未知用途的固定值数组
    /// 通常为: 00 00 00 02 00 00 00 02 00 00 00 02 00 00 00 00
    #[deku(endian = "big")]
    pub unknowns_0: [i32; 4],

    /// 前一个页面号(Previous Page Number)
    /// 具体用途待确认
    #[deku(endian = "big")]
    pub pfno: u32,

    /// 存储参考号索引页ID的数组
    /// 通过自定义reader函数读取
    #[deku(reader = "read_refno_index_pgid(deku::rest, )")]
    pub data_locs: Vec<RefnoIndexPgId>,
}

//DekuWrite
/// PDMS数据库中的索引页数据结构
///
/// 用于存储参考号和其位置信息的索引页数据
#[derive(Derivative, PartialEq, DekuRead)]
#[derivative(Debug)]
#[deku(endian = "big")]
pub struct IndexPageData {
    /// 页面类型标识
    pub page_type: i32,

    /// 页面标识符,固定值为0xCC47DF
    #[deku(assert_eq = "0xCC47DF")]
    pub noun: i32,

    /// 索引层级
    /// 用于表示当前索引页在B树结构中的层级
    pub level: u32,

    /// 未知用途的固定值数组
    pub unknowns: [u32; 3],

    /// 前一个页面号
    pub pfno: u32,

    /// 参考号位置信息列表
    /// 存储了参考号及其在数据库中的具体位置
    #[deku(reader = "read_refno_data_loc(deku::rest)")]
    pub refno_locs: Vec<RefnoDataLoc>,

    /// 页面剩余的填充字节
    /// 用于填充页面到固定大小
    #[derivative(Debug = "ignore")]
    #[deku(count = "deku::rest.len()/8")]
    pub remain_zero_bytes: Vec<u8>,
}

impl IndexPageData {
    /// 获取起始页
    ///
    /// 返回索引页中的起始页位置信息（如果存在）
    ///
    /// # 返回值
    /// * `Option<&RefnoDataLoc>` - 如果找到起始页则返回Some,否则返回None
    #[inline]
    pub fn get_start_page(&self) -> Option<&RefnoDataLoc> {
        self.refno_locs
            .first()
            .filter(|first| first.is_start_page())
    }

    /// 获取最大页号
    ///
    /// 遍历所有参考号位置信息,返回最大的页号值
    ///
    /// # 返回值
    /// * `u32` - 最大页号,如果列表为空则返回0
    #[inline]
    pub fn get_max_pgno(&self) -> u32 {
        self.refno_locs
            .iter()
            .map(|x| x.pgno)
            .max()
            .unwrap_or_default()
    }
}

fn read_refno_data_loc(
    mut rest: &BitSlice<u8, Msb0>,
) -> Result<(&BitSlice<u8, Msb0>, Vec<RefnoDataLoc>), DekuError> {
    let mut vec = Vec::new();
    loop {
        let (next_rest, peek) = u32::read(rest, ())?;
        if peek == 0x0 {
            rest = next_rest;
            break;
        }
        let (next_rest, mut d) = RefnoDataLoc::read(rest, ())?;

        vec.push(d);
        rest = next_rest;
    }
    Ok((rest, vec))
}

fn read_refno_index_pgid(
    rest: &BitSlice<u8, Msb0>,
) -> Result<(&BitSlice<u8, Msb0>, Vec<RefnoIndexPgId>), DekuError> {
    let mut pgids = Vec::new();
    let mut rest = rest;
    loop {
        let (next_rest, peek) = u32::read(rest, ())?;
        if peek == 0x0 {
            rest = next_rest;
            break;
        }
        let (next_rest, pgid) = RefnoIndexPgId::read(rest, ())?;
        pgids.push(pgid);
        rest = next_rest;
    }
    Ok((rest, pgids))
}

//todo 需要处理跨页的数据
#[derive(Clone, Debug, PartialEq, Default, DekuRead, DekuWrite)]
#[deku(ctx = "_endian: Endian")]
pub struct EleMembers {
    #[deku(endian = "big")]
    pub flag: u16,
    #[deku(endian = "big")]
    pub len: u16,
    #[deku(endian = "big")]
    pub refno: (u32, u32),
    #[deku(endian = "big")]
    pub unknown_0: (u32, u32),
    #[deku(count = "(len-4)/2")]
    #[deku(endian = "big")]
    pub children: Vec<(u32, u32)>,
}

#[derive(Clone, Debug, PartialEq, DekuRead, DekuWrite)]
// #[deku(endian = "big")]
pub struct ElePageData {
    //0x7
    pub flag: u32,
    #[deku(reader = "read_eles(deku::rest)")]
    pub eles_vec: Vec<EleRawData>,
    #[deku(count = "deku::rest.len()/8")]
    pub remain_bytes: Vec<u8>, //剩余的余量bytes
}

#[derive(Clone, Debug, PartialEq, DekuRead, DekuWrite)]
#[deku(endian = "big")]
pub struct EleRawData {
    //00 00
    pub implicit_flag: u16,
    //00 2E
    pub implicit_count: u16,

    // pub implicit_size: i32,
    //00 00 00 02 00 00 00 02 00 00 00 02 00 00 00 00
    pub ref0: i32,
    pub ref1: i32,
    pub noun: i32, //还需要搞的清楚一点，这个值到底怎么来的

    pub parent_ref0: i32,
    pub parent_ref1: i32,
    pub page_no: u32,

    #[deku(cond = "*implicit_flag == 0", count = "(implicit_count - 7) * 4")]
    pub implicit_data: Vec<u8>,

    #[deku(reader = "read_members(deku::rest)")]
    pub members: Option<EleMembers>,

    pub explicit_flag: u16,
    pub explicit_count: u16,

    #[deku(cond = "*explicit_flag == 1", count = "(explicit_count - 1) * 4")]
    pub explicit_data: Option<Vec<u8>>,
}

impl EleRawData {}

fn read_members(
    rest: &BitSlice<u8, Msb0>,
) -> Result<(&BitSlice<u8, Msb0>, Option<EleMembers>), DekuError> {
    let (_next_rest, peek) = u16::read(rest, Endian::Big)?;
    if peek != 0x2 {
        return Ok((rest, None));
    }
    let (next_rest, membs) = EleMembers::read(rest, Endian::Big)?;
    Ok((next_rest, Some(membs)))
}

fn read_eles(
    rest: &BitSlice<u8, Msb0>,
) -> Result<(&BitSlice<u8, Msb0>, Vec<EleRawData>), DekuError> {
    let mut vec = Vec::new();
    let mut rest = rest;
    loop {
        let (next_rest, peek) = u32::read(rest, ())?;
        if peek == 0x0 {
            rest = next_rest;
            break;
        }
        let (next_rest, d) = EleRawData::read(rest, ())?;
        vec.push(d);
        rest = next_rest;
    }
    Ok((rest, vec))
}
