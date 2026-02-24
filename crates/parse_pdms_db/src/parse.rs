use crate::consts::*;
use crate::parse_explict_tools::*;
// 使用新 parser 模块中的基础函数
use crate::parser::attribute::explicit::{get_explicit_attr_type, parse_explicit_header};
use crate::parser::attribute::expression::parse_expression_attr as parse_expression_attr_nom;
use crate::parser::attribute::implicit::{
    parse_implicit_attr_value as parse_implicit_attr_value_new, ImplicitAttrOffset,
};
use crate::parser::attribute::expression_payload::decode_expression_payload;
use crate::parser::combinator::collect_segmented_payload;
use crate::parser::database::header::extract_db_no;
use crate::parser::database::validation::is_valid_db_header;
use crate::parser::element::children::{extract_members, parse_element_children};
use crate::parser::primitives::{parse_members, parse_owner};
use aios_core::basic::info::RefnoInfo;
use aios_core::consts::EXPR_ATT_SET;
use aios_core::db::*;
use aios_core::get_db_option;
use aios_core::get_default_pdms_db_info;
use aios_core::helper::*;
use aios_core::pdms_types::*;
use aios_core::tool::db_tool::*;
use aios_core::types::db_info::PdmsDatabaseInfo;
use aios_core::types::WholeAttMap;
use aios_core::types::*;
use aios_core::AttrVal::*;
use aios_core::SUL_DB;
use anyhow::*;
use core::result::Result::Ok;
#[allow(unused_mut)]
use core::slice::SlicePattern;
use dashmap::{DashMap, DashSet};
use itertools::Itertools;
use memchr::memmem;
use memchr::memmem::rfind_iter;
use nom::bytes::complete::take_until;
use nom::character::complete::alpha1;
use nom::combinator::verify;
use nom::error::ErrorKind;
use nom::multi::{count, many_till};
use nom::number::complete::{be_i16, be_i32, be_u32, be_u64};
use nom::sequence::tuple;
use nom::IResult;
use nom::Parser;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rayon::prelude::IntoParallelIterator;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Debug;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::AsyncReadExt;

//00 00 00 05 00 CC 47 DF 00 00 00 00 00 00 00 02
// const REFNO_ALL_INDEX_PAGE: [u8; 11] = [0x00u8, 0x00, 0x00, 0x05, 0x00, 0xCC, 0x47, 0xDF, 0x00, 0x00, 0x00];
const REFNO_ALL_INDEX_PAGE: [u8; 8] = [0x0u8, 0xCC, 0x47, 0xDF, 0x0, 0x0, 0x0, 0x0];

///一个pdms db的整体数据
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PdmsDbData {
    /// 按noun类型分类的参考号
    pub type_ele_map: DashMap<u32, HashSet<RefU64>>,
    /// 完整属性数据的存储
    pub total_attr_map: DashMap<RefU64, NamedAttrMap>,
    /// 所有包含子节点的map
    pub children_map: HashMap<RefU64, RefU64Vec>,
    ///数据文件名
    pub filename: String,
    ///数据文件的ses pgno
    pub ses_pgno: u32,
    ///数据文件的db type（DESI、CATA、SYS等等）
    pub db_type: String,
    /// 数据文件的db 名称（SYS里用的名称）
    pub db_name: String,
    /// 数据文件的 db number（统一命名为 dbnum）
    #[serde(alias = "dbnum")]
    pub dbnum: u32,
    ///数据文件的field no
    pub field_no: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoomCode {
    pub refno: RefU64,
    pub name_hash: AiosStrHash,
}

///解析pdms的目录
pub async fn parse_pdms_dir(
    dir: &str,
    project: &str,
    config_path: Option<&str>,
    need_parsed_files: &Option<Vec<String>>,
) -> Result<DashMap<String, PdmsDbData>> {
    let dir = PathBuf::from(dir);
    let pdms_project_data_map = DashMap::new();
    let mut children_files = fs::read_dir(dir)?
        .into_iter()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| is_pdms_db_file(p) && check_path_db_header(p))
        .collect::<Vec<PathBuf>>();
    let mut database_info = None;

    if config_path.is_some() {
        if let Ok(mut file) = File::open(config_path.unwrap()) {
            let mut attr_buf: Vec<u8> = Vec::new();
            file.read_to_end(&mut attr_buf).context("read database_info config")?;
            database_info = serde_json::from_slice(&attr_buf).ok();
        }
    }

    let pdms_db_name_map = DashMap::new(); //file_name->db_name
    let mut sys_file = None;
    for path in &children_files {
        let file_name = path.file_name().unwrap().to_str().unwrap();
        //sys file 信息的处理
        if file_name.ends_with("sys") {
            if need_parsed_files.is_some()
                && !need_parsed_files
                    .as_ref()
                    .unwrap()
                    .contains(&file_name.to_string())
            {
                continue;
            }
            let mut pdms_db_data = parse_file(&path, &database_info, file_name, project).await?;
            pdms_db_data
                .total_attr_map
                .iter()
                .try_for_each::<_, Result<()>>(|m| {
                    let map = m.value();
                    let num = map
                        .get_u32("NUMBDB")
                        .ok_or(anyhow!("NUMBDB not exist".to_string()))?;
                    let fnum = map
                        .get_u32("FINO")
                        .ok_or(anyhow!("FINO not exist".to_string()))?;
                    let name = map
                        .get_as_string("NAME")
                        .ok_or(anyhow!("NAME not exist".to_string()))?
                        .to_string();
                    let dbnum = if fnum == 0 { num } else { fnum };
                    pdms_db_name_map.insert(dbnum, name);
                    Ok(())
                })?;
            if pdms_db_name_map.contains_key(&pdms_db_data.dbnum) {
                pdms_db_data.db_name = pdms_db_name_map.get(&pdms_db_data.dbnum).unwrap().clone();
            } else {
                pdms_db_data.db_name = file_name.into();
            }
            pdms_db_data.filename = file_name.into();
            pdms_project_data_map.insert(pdms_db_data.filename.clone(), pdms_db_data);
            sys_file = Some(path);
        }
    }

    if let Some(sys_file) = sys_file {
        children_files.remove(children_files.iter().position(|x| x == sys_file).unwrap());
    }

    for path in children_files {
        let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
        if !file_name.ends_with("com") && !file_name.ends_with("mis") {
            if need_parsed_files.is_none()
                || need_parsed_files.as_ref().unwrap().contains(&file_name)
            {
                let file_name = file_name.as_str();
                println!("path={:?}", file_name);
                if let Ok(mut pdms_db_data) =
                    parse_file(&path, &database_info, file_name, project).await
                {
                    pdms_db_data.filename = file_name.into();
                    let cur_dbnum = pdms_db_data.dbnum.to_string();
                    if pdms_db_data.filename.contains(&cur_dbnum) {
                        if pdms_db_name_map.contains_key(&pdms_db_data.dbnum) {
                            pdms_db_data.db_name =
                                pdms_db_name_map.get(&pdms_db_data.dbnum).unwrap().clone();
                        }
                    } else {
                        if pdms_db_name_map.contains_key(&pdms_db_data.field_no) {
                            pdms_db_data.db_name = pdms_db_name_map
                                .get(&pdms_db_data.field_no)
                                .unwrap()
                                .clone();
                        } else {
                            pdms_db_data.db_name = file_name.into();
                        }
                    }
                    pdms_project_data_map.insert(pdms_db_data.filename.clone(), pdms_db_data);
                }
            }
        }
    }

    return Ok(pdms_project_data_map);
}

///解析db文件的chidlren部分，得到参考号和对应的类型集合
pub fn parse_file_db_basic_data(
    path: &PathBuf,
    file_name: &str,
    project: &str,
) -> Result<DbBasicData> {
    if !is_pdms_db_file(path) || !check_path_db_header(path) {
        return Err(anyhow!("skip non-db file: {:?}", path));
    }
    let time_start = Instant::now();
    let mut file = File::open(path)?;
    let mut buf: Vec<u8> = Vec::new();
    file.read_to_end(&mut buf)?;
    let time = time_start.elapsed();
    println!("read file {:?} finished in {:?}", path, time);
    //使用默认的配置信息
    let basic_data = parse_db_basic_data(buf, file_name, project)?;
    Ok(basic_data)
}

///解析db文件
pub async fn parse_file(
    path: &PathBuf,
    database_info: &Option<PdmsDatabaseInfo>,
    file_name: &str,
    project: &str,
) -> Result<PdmsDbData> {
    if !is_pdms_db_file(path) {
        return Err(anyhow!("skip non-db file: {:?}", path));
    }
    let time_start = Instant::now();
    let mut file = File::open(path)?;
    let mut buf: Vec<u8> = Vec::new();
    file.read_to_end(&mut buf).context("read db file")?;
    if !is_valid_db_header(&buf) {
        return Err(anyhow!("invalid db header: {:?}", path));
    }
    let input = &buf[..];
    let time = time_start.elapsed();
    println!("read file {:?} finished in {:?}", path, time);
    if database_info.is_none() {
        //使用默认的配置信息
        let db_info = get_default_pdms_db_info();
        parse_db(input, &db_info, file_name, project).await
    } else {
        parse_db(input, database_info.as_ref().unwrap(), file_name, project).await
    }
}

///解析db文件
#[inline]
pub async fn parse_file_with_chunk(
    db_basic_data: Arc<DbBasicData>,
    file_name: &str,
    project: &str,
    chunk_refnos: &[RefU64],
    ses_range_map: &BTreeMap<i32, Range<u32>>,
    ignore_world_refno: bool,
) -> Result<PdmsDbData> {
    let db_info = get_default_pdms_db_info();
    parse_db_with_chunk_with_info(
        db_basic_data,
        &db_info,
        file_name,
        project,
        chunk_refnos,
        ses_range_map,
        ignore_world_refno,
    )
    .await
}

/// 解析db文件的chidlren部分，得到参考号和对应的类型集合（显式指定数据库信息）
pub async fn parse_file_with_chunk_with_info(
    db_basic_data: Arc<DbBasicData>,
    database_info: &PdmsDatabaseInfo,
    file_name: &str,
    project: &str,
    chunk_refnos: &[RefU64],
    ses_range_map: &BTreeMap<i32, Range<u32>>,
    ignore_world_refno: bool,
) -> Result<PdmsDbData> {
    parse_db_with_chunk_with_info(
        db_basic_data,
        database_info,
        file_name,
        project,
        chunk_refnos,
        ses_range_map,
        ignore_world_refno,
    )
    .await
}

/// 解析 db 文件（同步并行版本）
#[inline]
pub fn parse_file_with_chunk_parallel_sync(
    db_basic_data: Arc<DbBasicData>,
    file_name: &str,
    project: &str,
    chunk_refnos: &[RefU64],
    ses_range_map: &BTreeMap<i32, Range<u32>>,
    ignore_world_refno: bool,
) -> Result<PdmsDbData> {
    let db_info = get_default_pdms_db_info();
    parse_db_with_chunk_with_info_parallel_sync(
        db_basic_data,
        &db_info,
        file_name,
        project,
        chunk_refnos,
        ses_range_map,
        ignore_world_refno,
    )
}

/// 解析 db 文件（同步并行版本，显式数据库信息）
pub fn parse_file_with_chunk_parallel_sync_with_info(
    db_basic_data: Arc<DbBasicData>,
    database_info: &PdmsDatabaseInfo,
    file_name: &str,
    project: &str,
    chunk_refnos: &[RefU64],
    ses_range_map: &BTreeMap<i32, Range<u32>>,
    ignore_world_refno: bool,
) -> Result<PdmsDbData> {
    parse_db_with_chunk_with_info_parallel_sync(
        db_basic_data,
        database_info,
        file_name,
        project,
        chunk_refnos,
        ses_range_map,
        ignore_world_refno,
    )
}

#[derive(Debug, Clone, Default)]
pub struct EleData {
    pub refno: RefU64,
    pub owner: RefU64,
    pub noun: u32,
    pub whole_attmap: WholeAttMap,
    pub children: RefU64Vec,
    pub name: String,
}

impl EleData {
    #[inline]
    pub fn att_map(&self) -> &NamedAttrMap {
        self.whole_attmap.att_map()
    }

    #[inline]
    pub fn explicit_attmap(&self) -> &NamedAttrMap {
        self.whole_attmap.explicit_attmap()
    }

    #[inline]
    pub fn explicit_attmap_mut(&mut self) -> &mut NamedAttrMap {
        self.whole_attmap.explicit_attmap_mut()
    }

    #[inline]
    pub fn uda_atts(&self) -> &Vec<ExplicitAttr> {
        self.whole_attmap.uda_atts()
    }

    #[inline]
    pub fn uda_atts_mut(&mut self) -> &mut Vec<ExplicitAttr> {
        self.whole_attmap.uda_atts_mut()
    }

    #[inline]
    pub fn att_map_mut(&mut self) -> &mut NamedAttrMap {
        self.whole_attmap.att_map_mut()
    }

    #[inline]
    pub fn refno_enum(&self) -> RefnoEnum {
        self.att_map().ses_refno().into()
    }

    #[inline]
    pub fn refno(&self) -> RefU64 {
        self.att_map().latest_refno()
    }

    #[inline]
    pub fn latest_refno_enum(&self) -> RefnoEnum {
        self.att_map().latest_refno().into()
    }
}
// 以下函数已迁移到 crate::parser::element::children 模块
// parse_ref_u64_nom -> crate::parser::primitives::parse_refno
// parse_members_block -> crate::parser::element::children::parse_members_block
// parse_ele_children_nom -> crate::parser::element::children::parse_element_children

//只是获得RefU64, 用于多线程找到所有需要处理的参考号
pub fn parse_ele_membs(input: &[u8]) -> Vec<RefU64> {
    // 委托给新的 parser::element::children 模块
    extract_members(input)
}

/// 解析元素的子元素
///
/// # 参数
/// * `input` - 输入的字节数组切片
///
/// # 返回值
/// * `(RefU64, RefU64Vec)` - 返回一个元组,包含:
///   - 当前元素的引用号(RefU64)
///   - 子元素引用号的向量(RefU64Vec)
#[inline]
pub fn parse_ele_children(input: &[u8]) -> (RefU64, RefU64Vec) {
    // 委托给新的 parser::element::children 模块
    match parse_element_children(input) {
        Ok((_, res)) => res,
        Err(_) => {
            // 失败时尽量返回可读的 refno 方便上层判别
            let fallback_refno = if input.len() >= 12 {
                RefI32Tuple::from(&input[4..12]).into()
            } else {
                RefU64::default()
            };
            (fallback_refno, RefU64Vec::default())
        }
    }
}

/// 解析元素的基础数据（同步函数，不进行异步操作）
pub fn parse_raw_ele_data_with_info(
    input: &[u8],
    database_info: &PdmsDatabaseInfo,
) -> Result<EleData> {
    let mut implicit_attmap = NamedAttrMap::default();
    let mut explicit_attmap = NamedAttrMap::default();
    let mut children = RefU64Vec::default();
    let data_len = input.len();
    let impl_len = try_parse_to_i32(&input[0..4])?; //隐含数据长度  0-4
    if impl_len < 0 || (impl_len as usize) > data_len {
        return Err(anyhow!("impl_len < 0 || impl_len > data_len"));
    }
    let origin_impl_len = impl_len as i32 * 4;
    let mut actual_impl_len = origin_impl_len as usize; //隐含数据长度  0-4
    let refno: RefU64 = RefU64::from(&input[4..12]);
    let type_hash = try_parse_to_i32(&input[12..16])?;
    let noun = type_hash as u32;
    let noun_name = db1_dehash(noun); //类型hash  12-16
    let cur_type_info_map = database_info
        .named_attr_info_map
        .get(&noun_name)
        .ok_or(anyhow!("{} not exist in attr_info_map", &noun_name))?;
    let hash_type_info_map = database_info
        .noun_attr_info_map
        .get(&type_hash)
        .ok_or(anyhow!("{} not exist in attr_info_map", &noun_name))?;
    let owner = RefU64::from(&input[16..24]);
    if actual_impl_len + 4 < input.len() {
        let mut tmp_value = parse_to_i32(&input[actual_impl_len..actual_impl_len + 4]);
        while tmp_value == 0 || tmp_value == 7 {
            actual_impl_len += 4;
            tmp_value = parse_to_i32(&input[actual_impl_len..actual_impl_len + 4]);
        }
    }
    if actual_impl_len > data_len {
        return Err(anyhow!("actual_impl_len > data_len"));
    }
    //隐藏属性得数据切片
    let implicit_data = &input[0..actual_impl_len];
    let membs_pos = actual_impl_len;
    let membs_data = &input[membs_pos..];
    let maybe_refno: Option<RefU64> = if membs_data.len() > 12 {
        Some(RefU64::from(&membs_data[4..12]))
    } else {
        None
    };
    let mut memb_bytes_len = 0;

    if maybe_refno == Some(refno) && membs_data.len() >= 4 {
        if &membs_data[0..2] == [0x0, 0x2].as_slice() {
            let declared_bytes = parse_to_u16(&membs_data[2..4]) as usize * 4;
            memb_bytes_len = declared_bytes;
            // 使用新的 collect_segmented_payload 替换 get_merged_data
            let (rest, merged_data) =
                collect_segmented_payload(membs_data, declared_bytes, 0x2)
                    .map_err(|_| anyhow!("parse members segment failed"))?;
            if let Ok((_, c)) = parse_attr_members(&merged_data) {
                children = c;
            }
            // 更新 memb_bytes_len 为实际消耗的字节数
            memb_bytes_len = membs_data.len().saturating_sub(rest.len());
        }
    }

    let explicit_start = actual_impl_len + memb_bytes_len;
    if explicit_start > input.len() {
        return Err(anyhow!("explicit_start > input.len()"));
    }
    let explicit_data = &input[explicit_start..];
    let sorted_noun_hash = sort_offsets(&hash_type_info_map);
    let mut cur_offset: i32 = 0;
    let mut is_f32 = false;

    if sorted_noun_hash.len() > 0 {
        let last_key = sorted_noun_hash.last().unwrap();
        let last_att_info = hash_type_info_map.get(last_key).unwrap();
        let last_step = match last_att_info.att_type {
            DbAttributeType::DIRECTION
            | DbAttributeType::POSITION
            | DbAttributeType::ORIENTATION
            | DbAttributeType::Vec3Type => 3 * 2,
            DbAttributeType::ELEMENT => 2,
            //填的最小的数量，最少有两个数据
            DbAttributeType::INTVEC | DbAttributeType::FLOATVEC | DbAttributeType::DOUBLEVEC => 2,
            _ => 1,
        };
        is_f32 = last_att_info.offset + last_step > (origin_impl_len / 4) as u32;
    }
    //如果发现是f32的数据，就需要重新算偏移
    let mut f32_neg_offset = 0usize;
    for i in 0..sorted_noun_hash.len() {
        let noun_hash = sorted_noun_hash[i];
        let noun_name = db1_dehash(noun_hash as _);
        let attr_info = cur_type_info_map.get(&noun_name).unwrap();
        if cur_offset == 0 {
            cur_offset = (attr_info.offset & 0xFFFFF) as i32;
        }
        let mut step_w = 0;
        if i >= 1 {
            let prev_attr_info = hash_type_info_map.get(&sorted_noun_hash[i - 1]).unwrap();
            let b_expr = check_is_expr(prev_attr_info.hash);
            if is_f32 && !b_expr {
                match prev_attr_info.att_type {
                    DbAttributeType::DOUBLE => {
                        f32_neg_offset += 1;
                    }
                    DbAttributeType::DIRECTION
                    | DbAttributeType::POSITION
                    | DbAttributeType::ORIENTATION
                    | DbAttributeType::Vec3Type => {
                        f32_neg_offset += 3;
                    }
                    _ => {}
                }
            }
        }
        if i < sorted_noun_hash.len() - 1 {
            let next_attr_info = hash_type_info_map.get(&sorted_noun_hash[i + 1]).unwrap();
            step_w =
                (next_attr_info.offset & 0xFFFFF) as i32 - (attr_info.offset & 0xFFFFF) as i32 ;
        } else {
            step_w = origin_impl_len / 4 - (attr_info.offset as i32 & 0xFFFFF);
        }
        let step = step_w.max(1) as usize;

        // ============================================================
        // 新模块集成示例 (未来迁移参考)
        // ============================================================
        // 当启用新解析器时,使用以下代码替换旧的 parse_implicit_attr_value:
        //
        // #[cfg(feature = "new-parser")]
        // {
        //     use crate::parser::attribute::implicit::{ImplicitAttrOffset, parse_implicit_attr_value as parse_new};
        //
        //     // 1. 转换 AttrInfo 为 ImplicitAttrOffset
        //     let implicit_offset = ImplicitAttrOffset {
        //         name: attr_info.name.clone(),
        //         offset: attr_info.offset,
        //         attr_type: attr_info.att_type.clone(),
        //     };
        //
        //     // 2. 调用新的解析函数
        //     if let Ok((_, new_val)) = parse_new(&implicit_data, &implicit_offset, is_f32, f32_neg_offset, step) {
        //         // 3. 转换 NamedAttrValue -> AttrVal
        //         let att_val: AttrVal = new_val.into();
        //
        //         // 4. 后续处理保持不变
        //         match &att_val {
        //             RefU64Type(value) => { ... }
        //             InvalidType => { ... }
        //             _ => {}
        //         }
        //
        //         if attr_info.name != "unset" {
        //             implicit_attmap.insert(attr_info.name.clone(), att_val.into());
        //         }
        //     }
        // }
        //
        // 优势:
        // - 修复了偏移计算 bug (详见 llmdoc/agent/pdms-0x07-segment-offset-investigation.md)
        // - 类型安全 (基于 DbAttributeType 而非 default_val)
        // - 模块化清晰,易于测试和维护
        //
        // 参考文档:
        // - llmdoc/guides/parser-module-integration.md - 完整集成指南
        // - examples/new_parser_usage.rs - 使用示例
        // ============================================================

        let is_expr =
            check_is_expr(attr_info.hash) || is_force_implicit_expr_attr_name(attr_info.name.trim());
        if is_expr {
            if let Ok((_, legacy_val)) = parse_implicit_attr_value(
                &implicit_data,
                &attr_info,
                is_f32,
                f32_neg_offset,
                step,
            ) {
                let att_val = NamedAttrValue::from(&legacy_val);
                if attr_info.name != "unset" {
                    implicit_attmap.insert(attr_info.name.clone(), att_val);
                }
            }
            continue;
        }

        let implicit_offset = ImplicitAttrOffset {
            name: attr_info.name.clone(),
            offset: attr_info.offset,
            attr_type: attr_info.att_type.clone(),
        };
        let mut att_val = None;
        if let Ok((_, new_val)) = parse_implicit_attr_value_new(
            &implicit_data,
            &implicit_offset,
            is_f32,
            f32_neg_offset,
            step,
        ) {
            if !matches!(new_val, NamedAttrValue::InvalidType) {
                att_val = Some(new_val);
            }
        }
        if att_val.is_none() {
            if let Ok((_, legacy_val)) = parse_implicit_attr_value(
                &implicit_data,
                &attr_info,
                is_f32,
                f32_neg_offset,
                step,
            ) {
                att_val = Some(NamedAttrValue::from(&legacy_val));
            }
        }

        if let Some(att_val) = att_val {
            match &att_val {
                NamedAttrValue::RefU64Type(value) => {
                    if attr_info.name.to_lowercase() != "owner" && value.get_0() != 0 {
                        // foreign_refnos.insert(attr_info.name.to_string(), *value);
                    }
                }
                NamedAttrValue::InvalidType => {
                    #[cfg(feature = "debug_parse")]
                    {
                        dbg!(&refno);
                        dbg!(&noun_name);
                    }
                }
                _ => {}
            }
            // unset 是pdms数据中存在info文件里没有的offset数据，手动在info文件里面加的这个 unset 占位
            if attr_info.name != "unset" {
                implicit_attmap.insert(attr_info.name.clone(), att_val);
            }
        }
    }
    let final_explicit_data = collect_explict_data(explicit_data, refno);

    // 临时调试输出（仅 debug_parse）
    if cfg!(feature = "debug_parse") {
        eprintln!(
            "[DEBUG collect_explict_data] refno={}, explicit_data.len()={}, final_explicit_data.len()={}",
            refno,
            explicit_data.len(),
            final_explicit_data.len()
        );
    }

    //添加遗漏的属性
    implicit_attmap.insert("OWNER".into(), NamedAttrValue::RefU64Type(owner));
    implicit_attmap.insert("TYPE".into(), NamedAttrValue::StringType(noun_name));
    implicit_attmap.insert("REFNO".into(), NamedAttrValue::RefU64Type(refno));
    let name = implicit_attmap.get_name_or_default();

    let explicit_attrs =
        match parse_raw_explicit_attrs(&final_explicit_data, &cur_type_info_map, refno) {
            Ok(result) => result.1,
            Err(e) => {
                println!("解析显式属性失败: {:?}, refno={:?}", e, refno);
                Vec::new()
            }
        };

    // 将ExplicitAttr分成UDA属性和普通显式属性
    let mut uda_atts = Vec::new();
    for attr in explicit_attrs {
        if attr.is_uda {
            uda_atts.push(attr);
        } else {
            explicit_attmap.insert(attr.name, attr.value.into());
        }
    }

    // 创建基础 EleData，包含UDA属性列表
    let ele_data = EleData {
        refno,
        owner,
        noun,
        whole_attmap: WholeAttMap {
            attmap: implicit_attmap,
            explicit_attmap,
            uda_atts,
        },
        children,
        name,
    };

    Ok(ele_data)
}

/// 解析元素的基础数据（使用默认数据库配置）
pub fn parse_raw_ele_data(input: &[u8]) -> Result<EleData> {
    let db_info = get_default_pdms_db_info();
    parse_raw_ele_data_with_info(input, &db_info)
}

/// 解析元素数据（同步核心实现）
pub fn parse_ele_data_with_info_sync(
    input: &[u8],
    database_info: &PdmsDatabaseInfo,
) -> Result<EleData> {
    // 使用同步函数解析基础数据
    let mut ele_data = parse_raw_ele_data_with_info(input, database_info)?;

    // 获取需要的信息用于异步调用
    let _refno = ele_data.refno;
    let noun_name = db1_dehash(ele_data.noun);
    let cur_type_info_map = database_info
        .named_attr_info_map
        .get(&noun_name)
        .ok_or(anyhow!("{} not exist in attr_info_map", &noun_name))?;

    // 解析显式属性
    // 异步处理显式属性
    let explicit_attmap = &mut ele_data.whole_attmap.explicit_attmap;

    // 如果存在UDA属性，进行处理（直接从预加载的缓存读取）
    if !ele_data.whole_attmap.uda_atts.is_empty() {
        let _ = process_explicit_attrs(
            std::mem::take(&mut ele_data.whole_attmap.uda_atts),
            explicit_attmap,
        );
    }
    // 精炼属性
    ele_data.whole_attmap = ele_data.whole_attmap.refine(&cur_type_info_map);

    Ok(ele_data)
}

/// 解析元素数据，包含异步处理（兼容包装）
#[deprecated(
    note = "parse_ele_data_with_info 是兼容层，请优先使用 parse_ele_data_with_info_sync"
)]
pub async fn parse_ele_data_with_info(
    input: &[u8],
    database_info: &PdmsDatabaseInfo,
) -> Result<EleData> {
    parse_ele_data_with_info_sync(input, database_info)
}

/// 解析元素数据（同步，使用默认数据库配置）
pub fn parse_ele_data_sync(input: &[u8]) -> Result<EleData> {
    let db_info = get_default_pdms_db_info();
    parse_ele_data_with_info_sync(input, &db_info)
}

/// 解析元素数据，包含异步处理（兼容包装，使用默认数据库配置）
#[deprecated(
    note = "parse_ele_data 是兼容层，请优先使用 parse_ele_data_sync 或 parse_ele_data_with_info_sync"
)]
pub async fn parse_ele_data(input: &[u8]) -> Result<EleData> {
    parse_ele_data_sync(input)
}

//移除00 00 00 007，保留后面的数据
pub fn collect_explict_data(mut input: &[u8], refno: RefU64) -> Vec<u8> {
    let mut bytes = Vec::new();
    let original_len = input.len();
    let mut block_count = 0;
    if input.len() < 4 {
        return bytes;
    }
    // 显式块的 payload 在不同样本里存在两种布局：
    // - 直接从 offset=12（flag+len+self_ref）开始就是属性流
    // - offset=12 之后还有 8 字节保留区（旧实现用 offset=20）
    // 这里用“可解析性”做自适应判定，避免误删真实数据。
    let looks_like_attr_stream_start = |buf: &[u8]| -> bool {
        if buf.len() < 4 {
            return false;
        }
        let hash_val = convert_to_hash(&buf[..4]);
        if hash_val == 0 {
            return false;
        }
        // 表达式类显式属性：只有 hash + expression payload（无 type/len 头）
        if check_is_expr(hash_val) {
            if parse_expression_attr_nom(buf, refno.0).is_ok() {
                return true;
            }
        }
        // 普通显式属性：hash + type_code + len_words
        if buf.len() < 8 {
            return false;
        }
        if let Ok((rest, header)) = parse_explicit_header(buf) {
            if header.hash == 0 {
                return false;
            }
            if get_explicit_attr_type(header.type_code).is_none() {
                return false;
            }
            let data_len = header.data_len();
            return data_len <= rest.len();
        }
        false
    };
    while input.len() >= 4 {
        //还可能遇到各种page，需要跳过，暂时假定只有遇到INDEX Page的情况
        let v = parse_to_i32(&input[..4]);
        match v {
            0 | 7 => {
                input = &input[4..];
            }
            5 => {
                if input.len() < 8 {
                    break;
                }
                let page_type = parse_to_i32(&input[4..8]);
                #[cfg(feature = "debug_parse")]
                println!("Found {refno} attr may be in page type {:#4X?}", page_type);
                if page_type == 0xCC47DF && input.len() > 0x2C {
                    //一直找到为0的为止
                    input = &input[0x2C..];
                    while input.len() >= 4 && parse_to_i32(&input[0..4]) != 0 {
                        input = &input[4..];
                    }
                    if input.len() > 4 {
                        input = &input[4..];
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            _ => {
                let flag = parse_to_u16(&input[0..2]) as u8;
                if flag != 1 {
                    if cfg!(feature = "debug_parse") {
                        // 调试：打印更多信息
                        let raw_bytes: Vec<u8> = input[..std::cmp::min(16, input.len())].to_vec();
                        eprintln!(
                            "[DEBUG collect_explict_data] block#{} break: flag={} != 1, pos={}, v={:#X}, raw_bytes={:02X?}",
                            block_count,
                            flag,
                            original_len - input.len(),
                            v,
                            raw_bytes
                        );
                    }
                    // 这里不能直接 break：在某些样本里 explicit 区域后面可能混入其它块/下一条记录开头，
                    // 我们仅需“搜集当前 refno 的显式块”，应尝试按 word 对齐继续向后寻找下一个 0x0001 块头。
                    input = &input[4..];
                    continue;
                }
                let len_words = parse_to_u16(&input[2..4]) as usize;
                // len_words 是该显式块的总 word 数（包含 flag+len 本身的 4 字节）。
                // 某些数据块后面可能紧跟 0/7 填充；这些应由上层循环的 (0|7) 分支跳过，
                // 不能在这里“无条件 +4”，否则会把下一块的 flag+len 吃掉，导致跨块错位。
                let declared_len_bytes = len_words * 4;
                if declared_len_bytes < 12 || declared_len_bytes > input.len() {
                    if cfg!(feature = "debug_parse") {
                        eprintln!(
                            "[DEBUG collect_explict_data] block#{} break: declared_len_bytes={}, input.len()={}",
                            block_count,
                            declared_len_bytes,
                            input.len()
                        );
                    }
                    // 尝试继续向后 resync
                    input = &input[4..];
                    continue;
                }
                let maybe_refno = RefU64::from(&input[4..12]);
                //必须要检查是否跟的是 refno
                if len_words < 5 || maybe_refno != refno {
                    if cfg!(feature = "debug_parse") {
                        eprintln!(
                            "[DEBUG collect_explict_data] block#{} break: len_words={}, maybe_refno={}, expected_refno={}",
                            block_count,
                            len_words,
                            maybe_refno,
                            refno
                        );
                    }
                    input = &input[4..];
                    continue;
                }
                block_count += 1;
                if cfg!(feature = "debug_parse") {
                    eprintln!(
                        "[DEBUG collect_explict_data] block#{} found: len_words={}, declared_len_bytes={}",
                        block_count,
                        len_words,
                        declared_len_bytes
                    );
                }
                match collect_segmented_payload(input, declared_len_bytes, 0x01) {
                    Ok((rest, mut payload)) => {
                        // 兼容：主段可能包含 8 字节保留区（不一定全 0）
                        if payload.len() >= 8 && payload[..8].iter().all(|&b| b == 0) {
                            payload.drain(..8);
                        } else if payload.len() >= 16 {
                            let ok0 = looks_like_attr_stream_start(&payload);
                            let ok8 = looks_like_attr_stream_start(&payload[8..]);
                            if !ok0 && ok8 {
                                payload.drain(..8);
                            }
                        }
                        bytes.extend(payload);
                        input = rest;
                    }
                    Err(e) => {
                        if cfg!(feature = "debug_parse") {
                            eprintln!(
                                "[DEBUG collect_explict_data] block#{} break: collect_segmented_payload error: {:?}",
                                block_count,
                                e
                            );
                        }
                        break;
                    }
                }
            }
        }
    }
    bytes
}

pub fn take_off_007_explicit(mut input: &[u8]) -> &[u8] {
    while input.len() >= 4 {
        let head = &input[..4];
        if head == &[0, 0, 0, 0][..] || head == &[0, 0, 0, 7][..] {
            input = &input[4..];
        }
        if &head[..2] != &[0, 1][..] {
            return input;
        }
        let explicit_len = parse_to_u16(&head[2..4]) as usize;
        let b_007 = &input[explicit_len * 4..explicit_len * 4 + 4];
        if b_007 != &[0, 0, 0, 7] {
            return &input[..explicit_len * 4];
        }
    }
    input
}

///解析db文件的chidlren部分，得到参考号和对应的类型集合
pub fn parse_db_basic_data(input: Vec<u8>, _file_name: &str, _project: &str) -> Result<DbBasicData> {
    let gen_ref_time = Instant::now();
    let (refno_table_map, world_refno) = gen_ref_type_pos_table(&input);
    println!(
        "gen_ref_type_pos_table: {} ms",
        gen_ref_time.elapsed().as_millis()
    );

    let _root_refno = world_refno;

    let memb_time = Instant::now();
    
    // 并行处理：直接遍历所有 refno 解析 children
    let children_map_dash: DashMap<RefU64, Vec<RefU64>> = DashMap::with_capacity(refno_table_map.len());
    
    // 并行处理所有 refno 的 children 解析
    refno_table_map.par_iter().for_each(|entry| {
        let refno = *entry.key();
        let pos = entry.value().pos;
        let d = &input[pos - 4..];
        let membs = parse_ele_membs(&d);
        let children: Vec<RefU64> = membs
            .iter()
            .filter(|&x| refno_table_map.contains_key(x))
            .cloned()
            .collect();
        children_map_dash.insert(refno, children);
    });
    
    // 转换为 HashMap
    let children_map: HashMap<RefU64, Vec<RefU64>> = children_map_dash
        .into_iter()
        .collect();
    
    let all_refnos_count = children_map.len();
    
    println!(
        "Parsing children members cost: {} ms",
        memb_time.elapsed().as_millis()
    );
    println!("All refnos count: {}", all_refnos_count);

    let DbBasicInfo {
        db_type: _,
        ses_pgno,
        dbnum: _,
    } = parse_file_basic_info(&input);

    Ok(DbBasicData {
        ses_pgno,
        bytes: input,
        world_refno,
        refno_table_map,
        children_map,
    })
}

#[inline]
fn get_sesno(ses_range_map: &BTreeMap<i32, Range<u32>>, pgno: u32) -> Option<i32> {
    for (sesno, range) in ses_range_map {
        if range.contains(&pgno) {
            return Some(*sesno);
        }
    }
    None
}

///解析db文件，因为有可能db文件会很大，所以需要做一个分段运行的策略
pub async fn parse_db_with_chunk(
    db_basic_data: Arc<DbBasicData>,
    filename: &str,
    project: &str,
    chunk_refnos: &[RefU64],
    ses_range_map: &BTreeMap<i32, Range<u32>>,
    ignore_world_refno: bool,
) -> Result<PdmsDbData> {
    let db_info = get_default_pdms_db_info();
    parse_db_with_chunk_with_info(
        db_basic_data,
        &db_info,
        filename,
        project,
        chunk_refnos,
        ses_range_map,
        ignore_world_refno,
    )
    .await
}

///解析db文件，因为有可能db文件会很大，所以需要做一个分段运行的策略（显式指定数据库信息）
pub async fn parse_db_with_chunk_with_info(
    db_basic_data: Arc<DbBasicData>,
    database_info: &PdmsDatabaseInfo,
    filename: &str,
    project: &str,
    chunk_refnos: &[RefU64],
    ses_range_map: &BTreeMap<i32, Range<u32>>,
    ignore_world_refno: bool,
) -> Result<PdmsDbData> {
    let input = &db_basic_data.bytes;
    let type_ele_map = Arc::new(DashMap::new());
    let total_att_map: Arc<DashMap<RefU64, NamedAttrMap>> = Arc::new(DashMap::new());
    let mut field_no = 0;
    let test_refno = get_db_option().get_test_refno().map(|x| x.into());

    let DbBasicInfo {
        db_type,
        ses_pgno,
        dbnum,
    } = parse_file_basic_info(input);
    let dbnum_str = dbnum.to_string();
    if db_type.as_str() != "SYST" && !filename.contains(&dbnum_str) {
        let _chars_len = dbnum_str.len();
        let l = filename.len();
        let end = filename.chars().position(|x| x == '_').unwrap_or(l);
        if end < project.len() {
            return Err(anyhow!("Not a valid db file"));
        }
        field_no = filename[project.len()..end]
            .parse::<u32>()
            .unwrap_or_default();
    }

    let root_refno = db_basic_data.world_refno;
    let _refno_info_map = Arc::new(DashMap::new());
    let mut children_map: HashMap<RefU64, RefU64Vec> = db_basic_data
        .children_map
        .iter()
        .map(|(k, v)| (*k, RefU64Vec(v.clone())))
        .collect();
    // 只保留当前分块相关的 children，避免超大 children_map 占用
    if !chunk_refnos.is_empty() {
        let keep_set: HashSet<RefU64> = chunk_refnos.iter().cloned().collect();
        children_map.retain(|k, _| keep_set.contains(k) || (!ignore_world_refno && *k == root_refno));
        for (_, v) in children_map.iter_mut() {
            v.retain(|child| keep_set.contains(child) || (!ignore_world_refno && *child == root_refno));
        }
    }
    //如果忽略world_refno，则不解析world_refno的数据
    // dbg!(&ignore_world_refno);
    if !ignore_world_refno {
        let entry = &*db_basic_data
            .refno_table_map
            .get(&root_refno)
            .ok_or(anyhow!("Not found refno in entry"))?;

        let pgno = entry.pos / 0x800;
        let sesno = get_sesno(&ses_range_map, pgno as _).unwrap_or_default();
        let EleData {
            refno,
            noun,
            whole_attmap,
            ..
        } = parse_ele_data_with_info(&input[entry.pos - 4..], database_info)
            .await
            .unwrap_or_default();
        let mut named_attmap: NamedAttrMap = whole_attmap.merge().into();
        named_attmap.set_sesno(sesno as _);
        // 用文件头解析到的 dbnum，而不是 refno.get_0()
        named_attmap.insert("DBNUM".into(), NamedAttrValue::IntegerType(dbnum as i32));

        #[cfg(feature = "debug_parse")]
        dbg!(&refno);
        total_att_map.insert(refno, named_attmap);
        type_ele_map
            .entry(noun)
            .or_insert(HashSet::default())
            .insert(refno);
        let ref_0 = refno.get_0();
        _refno_info_map
            .entry(ref_0)
            .or_insert(RefnoInfo { ref_0, db_no: dbnum });
    }

    for source_refno in chunk_refnos.iter() {
        let is_debug = test_refno.is_some() && test_refno == Some(*source_refno);
        if let Some(entry) = db_basic_data.refno_table_map.get(source_refno) {
            let pos = entry.pos;
            let total_attmap_clone = total_att_map.clone();
            let type_ele_map = type_ele_map.clone();
            let pgno = entry.pos / 0x800;
            if let Ok(EleData {
                refno,
                noun,
                whole_attmap,
                ..
            }) = parse_ele_data_with_info(&input[pos - 4..], database_info).await
            {
                let sesno = get_sesno(&ses_range_map, pgno as _).unwrap_or_default();
                let mut named_attmap: NamedAttrMap = whole_attmap.merge().into();
                named_attmap.set_sesno(sesno as _);
                named_attmap.insert("DBNUM".into(), NamedAttrValue::IntegerType(dbnum as i32));
                //页数就是所在的位置除以0x800
                total_attmap_clone.insert(refno, named_attmap);
                type_ele_map
                    .entry(noun)
                    .or_insert(HashSet::default())
                    .insert(refno);
            } else {
                if is_debug {
                    println!(
                        "parse ele data failed: {:?}, loc: {:#4X}",
                        source_refno, pos
                    );
                }
            }
        }
    }

    Ok(PdmsDbData {
        type_ele_map: Arc::try_unwrap(type_ele_map).unwrap(),
        total_attr_map: Arc::try_unwrap(total_att_map).unwrap(),
        children_map,
        filename: filename.into(),
        ses_pgno,
        db_type,
        db_name: Default::default(),
        dbnum,
        field_no,
    })
}

/// 解析 db 文件分块（同步并行版本，显式指定数据库信息）
pub fn parse_db_with_chunk_with_info_parallel_sync(
    db_basic_data: Arc<DbBasicData>,
    database_info: &PdmsDatabaseInfo,
    filename: &str,
    project: &str,
    chunk_refnos: &[RefU64],
    ses_range_map: &BTreeMap<i32, Range<u32>>,
    ignore_world_refno: bool,
) -> Result<PdmsDbData> {
    let input = &db_basic_data.bytes;
    let type_ele_map: DashMap<u32, HashSet<RefU64>> = DashMap::new();
    let total_att_map: DashMap<RefU64, NamedAttrMap> = DashMap::new();
    let mut field_no = 0;
    let test_refno = get_db_option().get_test_refno().map(|x| x.into());

    let DbBasicInfo {
        db_type,
        ses_pgno,
        dbnum,
    } = parse_file_basic_info(input);
    let dbnum_str = dbnum.to_string();
    if db_type.as_str() != "SYST" && !filename.contains(&dbnum_str) {
        let l = filename.len();
        let end = filename.chars().position(|x| x == '_').unwrap_or(l);
        if end < project.len() {
            return Err(anyhow!("Not a valid db file"));
        }
        field_no = filename[project.len()..end]
            .parse::<u32>()
            .unwrap_or_default();
    }

    let root_refno = db_basic_data.world_refno;
    let mut children_map: HashMap<RefU64, RefU64Vec> = db_basic_data
        .children_map
        .iter()
        .map(|(k, v)| (*k, RefU64Vec(v.clone())))
        .collect();
    if !chunk_refnos.is_empty() {
        let keep_set: HashSet<RefU64> = chunk_refnos.iter().cloned().collect();
        children_map.retain(|k, _| keep_set.contains(k) || (!ignore_world_refno && *k == root_refno));
        for (_, v) in children_map.iter_mut() {
            v.retain(|child| keep_set.contains(child) || (!ignore_world_refno && *child == root_refno));
        }
    }

    if !ignore_world_refno {
        let entry = &*db_basic_data
            .refno_table_map
            .get(&root_refno)
            .ok_or(anyhow!("Not found refno in entry"))?;

        let pgno = entry.pos / 0x800;
        let sesno = get_sesno(ses_range_map, pgno as _).unwrap_or_default();
        let EleData {
            refno,
            noun,
            whole_attmap,
            ..
        } = parse_ele_data_with_info_sync(&input[entry.pos - 4..], database_info)
            .unwrap_or_default();
        let mut named_attmap: NamedAttrMap = whole_attmap.merge().into();
        named_attmap.set_sesno(sesno as _);
        named_attmap.insert("DBNUM".into(), NamedAttrValue::IntegerType(dbnum as i32));
        total_att_map.insert(refno, named_attmap);
        type_ele_map
            .entry(noun)
            .or_insert(HashSet::default())
            .insert(refno);
    }

    chunk_refnos.par_iter().for_each(|source_refno| {
        let is_debug = test_refno.is_some() && test_refno == Some(*source_refno);
        if let Some(entry) = db_basic_data.refno_table_map.get(source_refno) {
            let pos = entry.pos;
            let pgno = entry.pos / 0x800;
            if let Ok(EleData {
                refno,
                noun,
                whole_attmap,
                ..
            }) = parse_ele_data_with_info_sync(&input[pos - 4..], database_info)
            {
                let sesno = get_sesno(ses_range_map, pgno as _).unwrap_or_default();
                let mut named_attmap: NamedAttrMap = whole_attmap.merge().into();
                named_attmap.set_sesno(sesno as _);
                named_attmap.insert("DBNUM".into(), NamedAttrValue::IntegerType(dbnum as i32));
                total_att_map.insert(refno, named_attmap);
                type_ele_map
                    .entry(noun)
                    .or_insert(HashSet::default())
                    .insert(refno);
            } else if is_debug {
                println!(
                    "parse ele data failed(parallel): {:?}, loc: {:#4X}",
                    source_refno, pos
                );
            }
        }
    });

    Ok(PdmsDbData {
        type_ele_map,
        total_attr_map: total_att_map,
        children_map,
        filename: filename.into(),
        ses_pgno,
        db_type,
        db_name: Default::default(),
        dbnum,
        field_no,
    })
}

///解析db文件，因为有可能db文件会很大，所以需要做一个分段运行的策略
pub async fn parse_db(
    input: &[u8],
    database_info: &PdmsDatabaseInfo,
    file_name: &str,
    project: &str,
) -> Result<PdmsDbData> {
    let type_ele_map = Arc::new(DashMap::new());
    let total_attr_map: Arc<DashMap<RefU64, NamedAttrMap>> = Arc::new(DashMap::new());
    let mut field_no = 0;

    let DbBasicInfo {
        db_type,
        ses_pgno,
        dbnum,
    } = parse_file_basic_info(input);
    #[cfg(feature = "debug_parse")]
    dbg!(&(db_type.as_str(), ses_pgno, dbnum, file_name));
    let dbnum_str = dbnum.to_string();
    if db_type.as_str() != "SYST" && !file_name.contains(&dbnum_str) {
        let _chars_len = dbnum_str.len();
        let l = file_name.len();
        let end = file_name.chars().position(|x| x == '_').unwrap_or(l);
        if end < project.len() {
            return Err(anyhow!("Not a valid db file"));
        }
        field_no = file_name[project.len()..end]
            .parse::<u32>()
            .unwrap_or_default();
    }

    let gen_ref_time = Instant::now();
    let (refno_table_map, world_refno) = gen_ref_type_pos_table(input);
    println!(
        "gen_ref_type_pos_table: {} ms",
        gen_ref_time.elapsed().as_millis()
    );

    let root_refno = world_refno;
    let refno_info_map = Arc::new(DashMap::new());
    let mut children_map = HashMap::new();
    let entry = &*refno_table_map
        .get(&root_refno)
        .ok_or(anyhow!("Not found refno in entry"))?;

    let EleData {
        refno,
        owner: _,
        noun,
        whole_attmap,
        children,
        name: _,
    } = parse_ele_data_with_info(&input[entry.pos - 4..], database_info)
        .await?;

    {
        let mut named_attmap: NamedAttrMap = whole_attmap.merge().into();
        named_attmap.insert("DBNUM".into(), NamedAttrValue::IntegerType(dbnum as i32));
        total_attr_map.insert(refno, named_attmap);
    }
    type_ele_map
        .entry(noun)
        .or_insert(HashSet::default())
        .insert(refno);
    let ref_0 = refno.get_0();
    refno_info_map
        .entry(ref_0)
        .or_insert(RefnoInfo { ref_0, db_no: dbnum });
    if children.len() > 0 {
        children_map.insert(refno, children.clone());
    }

    let memb_time = Instant::now();
    let mut pending_refnos = vec![root_refno.clone()];
    let mut all_refnos = HashSet::new();

    while !pending_refnos.is_empty() {
        let refno = pending_refnos.pop().unwrap();
        if all_refnos.contains(&refno) {
            continue;
        }
        all_refnos.insert(refno);
        if refno_table_map.contains_key(&refno) {
            let entry = &*refno_table_map.get(&refno).unwrap();
            #[cfg(feature = "debug_parse")]
            dbg!(entry);
            let pos = entry.pos;
            //解析到members数据
            let membs = parse_ele_membs(&input[pos - 4..]);
            for memb in &membs {
                if !all_refnos.contains(&memb) {
                    pending_refnos.push(*memb);
                }
            }
            children_map.insert(refno, RefU64Vec(membs));
        }
    }
    println!(
        "Parsing children members cost: {} ms",
        memb_time.elapsed().as_millis()
    );
    println!("All refnos count: {}", all_refnos.len());
    let _eles_time = Instant::now();
    println!("Begin parse attributes");
    // all_refnos.iter().for_each(|refno| {
    for refno in all_refnos {
        if refno_table_map.contains_key(&refno) {
            let entry = &*refno_table_map.get(&refno).unwrap();
            let pos = entry.pos;
            let whole_attr_dashmap = total_attr_map.clone();
            let type_ele_map = type_ele_map.clone();
            if let Ok(EleData {
                refno,
                noun,
                whole_attmap,
                ..
            }) = parse_ele_data_with_info(&input[pos - 4..], database_info).await
            {
                let mut named_attmap: NamedAttrMap = whole_attmap.merge().into();
                named_attmap.insert("DBNUM".into(), NamedAttrValue::IntegerType(dbnum as i32));
                whole_attr_dashmap.insert(refno, named_attmap);
                type_ele_map
                    .entry(noun)
                    .or_insert(HashSet::default())
                    .insert(refno);
            }
        }
    }
    // println!("解析属性所耗时间: {:?} ms", eles_time.elapsed().as_millis());
    // println!("带有外键属性的参考号个数为 {}", foreign_refnos_map.len());
    // println!("DB {} attrs count: {}", file_name, total_attr_map.len());
    // println!(
    //     "解析db: {} 所耗时间: {:?}ms",
    //     file_name,
    //     time_start.elapsed().as_millis()
    // );

    Ok(PdmsDbData {
        type_ele_map: Arc::try_unwrap(type_ele_map).unwrap(),
        total_attr_map: Arc::try_unwrap(total_attr_map).unwrap(),
        children_map,
        filename: file_name.into(),
        ses_pgno,
        db_type,
        db_name: Default::default(),
        dbnum,
        field_no,
    })
}

#[inline]
fn is_force_implicit_expr_attr_name(name: &str) -> bool {
    // 这些字段在 PDMS 中经常以“表达式”形式存储（例如 ATTRIB DESP[1 ]），
    // 直接按 DOUBLE/f64 解析会把表达式 payload 误读为浮点。
    matches!(name, "PX" | "PY" | "DX" | "DY" | "PRAD" | "DRAD")
}

/// 获取隐式属性, input为分段数据，已经限制了长度
///
/// **注意**: 这是旧的实现,存在以下问题:
/// - 偏移计算有误 (应该使用 12 字节而非 20 字节)
/// - 表达式处理和类型解析耦合
/// - 基于 `default_val` 判断类型,不够类型安全
///
/// **新实现**: 参见 [`crate::parser::attribute::implicit::parse_implicit_attr_value`]
/// - 修复了偏移计算 bug
/// - 基于 `DbAttributeType` 判断类型
/// - 职责分离,表达式处理独立
///
/// 迁移指南: [`llmdoc/guides/parser-module-integration.md`](../../llmdoc/guides/parser-module-integration.md)
#[inline]
pub fn parse_implicit_attr_value<'a>(
    origin_bytes: &'a [u8],
    attr_info: &'a AttrInfo,
    f32_flag: bool,
    f32_neg_offset: usize,
    step: usize, //dword 即 4字节数量
) -> IResult<&'a [u8], AttrVal> {
    let mut val = InvalidType;
    let force_expr = is_force_implicit_expr_attr_name(attr_info.name.trim());
    let b_expr = check_is_expr(attr_info.hash) || force_expr;
    // dbg!(attr_info);
    let offset = ((attr_info.offset & 0xFFFF) as usize - f32_neg_offset) * 4;
    // if attr_info.name.as_str() == "BANG" || attr_info.name.as_str() == "DRNS"{
    //     dbg!(offset);
    // }
    if offset > origin_bytes.len() {
        return Err(nom::Err::Error(nom::error::make_error(
            origin_bytes,
            ErrorKind::Eof,
        )));
    }
    let bytes = &origin_bytes[offset..];
    // println!("{}", pretty_hex::pretty_hex(&bytes));
    if b_expr {
        // dbg!(attr_info);
        // 既然当作表达式，而且又在隐含属性里，这里需要把bytes的长度锁定。
        // 注意：某些情况下 step 可能计算为 0（例如 offset 列表里它是最后一个），
        // 此时不能截成空 slice，否则会导致解析结果为空字符串。
        let expr_bytes_len = if step == 0 {
            bytes.len()
        } else {
            (step * 4).min(bytes.len())
        };
        let expr_bytes = &bytes[..expr_bytes_len];

        let (_, string_val) = parse_to_expression(expr_bytes, attr_info.default_val.clone())?;
        val = string_val.clone();

        // PX/PY/DX/DY/PRAD/DRAD 这类隐式“表达式”在不同数据里可能走的是显式表达式编码格式，
        // parse_to_expression 覆盖不全时会返回空字符串；这里再尝试用显式表达式解析器补全。
        if force_expr {
            if let AttrVal::StringType(s) = &val
                && s.trim().is_empty()
            {
                // step 可能不足以覆盖完整表达式：先扩大到剩余隐式数据再试一次旧隐式表达式解析器
                if expr_bytes_len < bytes.len() {
                    if let Ok((_, retry_val)) =
                        parse_to_expression(bytes, attr_info.default_val.clone())
                    {
                        val = retry_val;
                    }
                }

                if let Ok((_, (_, v))) = parse_expression_attr_nom(expr_bytes, 0) {
                    if !v.trim().is_empty() {
                        val = StringType(v.into());
                    }
                }
                if let AttrVal::StringType(s2) = &val
                    && s2.trim().is_empty()
                    && expr_bytes_len < bytes.len()
                {
                    if let Ok((_, (_, v))) = parse_expression_attr_nom(bytes, 0) {
                        if !v.trim().is_empty() {
                            val = StringType(v.into());
                        }
                    }
                }
                if let AttrVal::StringType(s2) = &val
                    && s2.trim().is_empty()
                {
                    if let Ok((_, v)) =
                        crate::parse_explict_tools::parse_expression_func(expr_bytes, RefU64::default())
                    {
                        if !v.trim().is_empty() {
                            val = StringType(v.into());
                        }
                    }
                }
                if let AttrVal::StringType(s2) = &val
                    && s2.trim().is_empty()
                {
                    if let Ok((_, (_, v))) =
                        crate::parse_explict_tools::parse_expression_attr(expr_bytes, RefU64::default())
                    {
                        if !v.trim().is_empty() {
                            val = StringType(v.into());
                        }
                    }
                }
                if let AttrVal::StringType(s2) = &val
                    && s2.trim().is_empty()
                    && expr_bytes_len < bytes.len()
                {
                    if let Ok((_, (_, v))) =
                        crate::parse_explict_tools::parse_expression_attr(bytes, RefU64::default())
                    {
                        if !v.trim().is_empty() {
                            val = StringType(v.into());
                        }
                    }
                }
                if let AttrVal::StringType(s2) = &val
                    && s2.trim().is_empty()
                    && expr_bytes_len < bytes.len()
                {
                    if let Ok((_, v)) =
                        crate::parse_explict_tools::parse_expression_func(bytes, RefU64::default())
                    {
                        if !v.trim().is_empty() {
                            val = StringType(v.into());
                        }
                    }
                }
            }
        }

        // 对坐标/半径类隐式表达式（PX/PY/DX/DY/PRAD/DRAD）保持字符串，
        // 避免把表达式（或带格式的 " 0"）强行转换为数值后丢失语义。
        if !force_expr {
            match attr_info.default_val {
                IntegerType(_) => {
                    if let AttrVal::StringType(s) = string_val
                        && let Ok(v) = s.parse::<i32>()
                    {
                        val = IntegerType(v);
                    }
                }
                DoubleType(_) => {
                    if let AttrVal::StringType(s) = string_val
                        && let Ok(v) = s.parse::<f64>()
                    {
                        val = DoubleType(v);
                    }
                }
                _ => {}
            }
        }
    } else {
        // 隐式属性LEVEL 需要做特殊处理 map给定的是IntegerType 但其实是Vec<Int>
        if attr_info.hash == ATT_LEVE || attr_info.hash == ATT_PTS {
            let (bytes, len) = be_u32(bytes)?;
            let (_, result) = count(be_i32, len as usize).parse(bytes)?;
            val = IntArrayType(result);
        } else {
            match attr_info.default_val {
                IntegerType(_) => {
                    let (_, r) = be_i32(bytes)?;
                    val = IntegerType(r);
                }
                DoubleType(_) => {
                    if f32_flag || step == 1 {
                        if bytes.len() >= 4 {
                            let d = parse_to_f32(&bytes[..4]) as f64;
                            val = DoubleType(d as _);
                        } else {
                            #[cfg(feature = "debug_parse")]
                            {
                                dbg!(step);
                                dbg!(f32_flag);
                                dbg!(f32_neg_offset);
                                dbg!(attr_info);
                                println!("parse double 有问题的数据：{:#04X?}", origin_bytes);
                            }
                        }
                    } else {
                        if bytes.len() >= 8 {
                            let d = parse_to_f64(&bytes[..8]);
                            if d > f32::MAX as f64 {
                                val = DoubleType(0.0);
                            } else {
                                val = DoubleType(d);
                            }
                        } else {
                            #[cfg(feature = "debug_parse")]
                            {
                                dbg!(step);
                                dbg!(f32_flag);
                                dbg!(attr_info);
                            }
                        }
                    }
                }
                BoolType(_) => {
                    //直接在定位的byte上执行
                    let o = (attr_info.offset >> 0x14) as usize;
                    let (_, r) = be_u32(bytes)?;
                    let result = r >> o & 1;
                    val = BoolType(result == 1);
                }
                StringType(_) => {
                    let (_, str_len) = be_i32(bytes)?;
                    let str_len = str_len as usize;
                    //有些应该是没有分清类型的数据？
                    if step == 1 && str_len != 0 {
                        if &bytes[..2] == &[0, 0] || &bytes[..2] == &[0xFF, 0xFF] {
                            let d = parse_to_i32(&bytes[..4]);
                            val = IntegerType(d);
                        } else {
                            let d = parse_to_f32(&bytes[..4]) as f64;
                            val = DoubleType(d);
                        }
                    } else if str_len < bytes.len() && bytes.len() >= 4 {
                        if str_len + 4 <= bytes.len() {
                            let (decode_string, _b_chi) = decode_chars_data(&bytes[4..str_len + 4]);
                            val = StringType(decode_string.into());
                        }
                    } else {
                        val = StringType("".into());
                    }
                }
                ElementType(_) | RefU64Type(_) => {
                    let (_, (ref_0, ref_1)) = tuple((be_u32, be_u32))(bytes)?;
                    if ref_0 == 0 {
                        val = RefU64Type(Default::default());
                    } else {
                        val = RefU64Type(RefU64::from_two_nums(ref_0, ref_1));
                    }
                }
                WordType(_) => {
                    let (_, v) = be_i32(bytes)?;
                    if v > 0x81BF1 {
                        val = WordType(db1_dehash(v as u32).into());
                    } else {
                        val = IntegerType(v);
                    }
                }
                //todo need to find if exist invalid data
                Vec3Type(_) => {
                    // let mut data = [0f64; 3];
                    let (l, _cnt) = be_i32(bytes)?;
                    let data_len = l.len() / 4; //WORD个数
                    if f32_flag {
                        if data_len >= 3 {
                            let data = parse_to_f32_arr(l, 3).try_into().unwrap();
                            val = Vec3Type(data);
                        } else {
                            #[cfg(feature = "debug_parse")]
                            {
                                dbg!(step);
                                dbg!(data_len);
                                dbg!(f32_flag);
                                dbg!(_cnt);
                                dbg!(attr_info);
                                println!("parse vec3 有问题的数据：{:#04X?}", origin_bytes);
                            }
                        }
                    } else {
                        if data_len >= 6 {
                            let data = parse_to_f64_arr(l, 3).try_into().unwrap();
                            val = Vec3Type(data);
                        } else {
                            #[cfg(feature = "debug_parse")]
                            {
                                dbg!(step);
                                dbg!(data_len);
                                dbg!(f32_flag);
                                dbg!(_cnt);
                                dbg!(attr_info);
                            }
                        }
                    }
                }
                DoubleArrayType(_) => {
                    let (l, cnt) = be_i32(bytes)?;
                    let data = parse_to_f32_arr(l, cnt as _);
                    val = DoubleArrayType(data);
                }
                // DbAttributeType::DATETIME => {}
                _ => {}
            }
        }
    }
    // #[cfg(debug_assertions)]
    // dbg!(&val);
    Ok((origin_bytes, val))
}

/// 获得 UDA 名称（从动态收集的 HashMap 中读取）
pub fn get_uda_full_name(hash: i32) -> Option<String> {
    UDA_NAME_CACHE.get(&hash).map(|v| v.clone())
}

/// 注册 UDA 名称到缓存（在解析过程中动态收集）
pub fn register_uda_name(hash: i32, name: String) {
    UDA_NAME_CACHE.insert(hash, format!("UDA_{}", name));
}


lazy_static! {
    static ref UDA_NAME_CACHE: DashMap<i32, String> = DashMap::new();
}

fn resolve_uda_label(hash: i32) -> String {
    if let Some(name) = UDA_NAME_CACHE.get(&hash) {
        return name.value().clone();
    }
    format!("UDA_HASH_{hash}")
}

/// 从数据库批量预加载所有 UDA 名称到缓存
pub async fn preload_uda_name_cache() -> anyhow::Result<()> {
    use aios_core::SurrealQueryExt;
    let sql = "SELECT VALUE [UKEY, UDNA, DYUDNA] FROM UDA WHERE UKEY != none";
    let udas: Vec<(i32, Option<String>, Option<String>)> = SUL_DB.query_take(sql, 0).await?;
    for (ukey, udna, dyudna) in udas {
        let name = udna.filter(|s| !s.is_empty())
            .or(dyudna.filter(|s| !s.is_empty()));
        if let Some(n) = name {
            UDA_NAME_CACHE.insert(ukey, format!("UDA_{}", n));
        }
    }
    Ok(())
}

/// 获取已知显式属性
pub fn process_explicit_attrs(
    uda_attrs: Vec<ExplicitAttr>,
    attr_data_map: &mut NamedAttrMap,
) -> anyhow::Result<()> {
    // 处理解析结果，UDA 名称从缓存中读取（缓存应在 SYST 解析后已预加载）
    for attr in uda_attrs {
        if attr.is_uda {
            let label = resolve_uda_label(attr.hash_val);
            attr_data_map.insert(label, attr.value.into());
        } else {
            //覆盖可能在隐含属性里出现过的数据
            attr_data_map.insert(attr.name, attr.value.into());
        }
    }

    Ok(())
}

/// 获取已知显式属性的原始数据解析（不包含UDA异步处理）
pub fn parse_raw_explicit_attrs<'a>(
    input: &'a [u8],
    attr_info_map: &DashMap<String, AttrInfo>,
    refno: RefU64,
) -> IResult<&'a [u8], Vec<ExplicitAttr>> {
    let mut residual = input;
    let test_refno = get_db_option().get_test_refno().map(|x| x.refno());
    let is_debug = test_refno == Some(refno);
    let mut attr_values = Vec::new();

    while !residual.is_empty() {
        let mut att_value = None;
        let hash_val = convert_to_hash(&residual[..4]);
        if hash_val == 0 {
            break;
        }
        let is_uda = is_uda(hash_val);
        let att_name = if is_uda {
            //UDA 单独处理, 占位不发生分配
            String::new()
        } else {
            db1_dehash(hash_val.abs() as _)
        };
        // 一些坐标/半径字段在不同数据里会以“表达式”编码出现，
        // 但它们的 hash 并不总在 EXPR_ATT_SET 中；若按 DOUBLE 直接解析会把表达式 payload 误读为浮点。
        let force_expr = !is_uda && is_force_implicit_expr_attr_name(att_name.trim());
        // println!("hex value is {:#4X?}, att name is {}", &residual[..4], &att_name);
        if is_debug {
            #[cfg(feature = "debug_parse")]
            {
                if is_uda {
                    dbg!(&att_name);
                }
            }
        }

        // 强制按表达式解析（仅针对 PX/PY/DX/DY/PRAD/DRAD 等字段）
        if force_expr {
            let mut parsed: Option<(&[u8], String)> = None;
            if let Ok((input, (_ty, value))) = parse_expression_attr_nom(residual, refno.0) {
                if !value.trim().is_empty() {
                    parsed = Some((input, value));
                }
            }
            if parsed.is_none() {
                if let Ok((input, (_ty, value))) =
                    crate::parse_explict_tools::parse_expression_attr(residual, refno)
                {
                    if !value.trim().is_empty() {
                        parsed = Some((input, value));
                    }
                }
            }

            if let Some((input, value)) = parsed {
                att_value = Some(StringType(value));
                residual = input;
            }
        }

        if att_value.is_none() && check_is_expr(hash_val) {
            // dbg!(&att_name);
            let mut parsed: Option<(&[u8], String)> = None;
            let mut expr_type: Option<String> = None;
            if let Ok((input, (ty, value))) = parse_expression_attr_nom(residual, refno.0) {
                expr_type = Some(ty);
                parsed = Some((input, value));
            }

            // PTCD/PTCDI 是最复杂的一类表达式：新/旧解析器各自有覆盖盲区。
            // 策略：两边都尝试（若可），再按“更像符号表达式”的结果优先选用。
            let prefer_dual_parse = matches!(expr_type.as_deref(), Some("PTCD") | Some("PTCDI"));
            let need_fallback = parsed
                .as_ref()
                .map(|(_, value)| value.trim().is_empty())
                .unwrap_or(true);
            let legacy = crate::parse_explict_tools::parse_expression_attr(residual, refno)
                .ok()
                .map(|(input, (_ty, value))| (input, value));
            let score = |s: &str| -> i32 {
                let mut sc = 0;
                if s.contains("PARA[") {
                    sc += 10;
                }
                if s.contains("DESP[") || s.contains("DDES[") || s.contains("WDES[") {
                    sc += 8;
                }
                if s.contains('/') || s.contains('*') || s.contains('+') {
                    sc += 3;
                }
                // 纯数值（或几乎纯数值）更可能是误解析：给一个负权重
                if s.trim().parse::<f64>().is_ok() {
                    sc -= 5;
                }
                sc
            };
            if prefer_dual_parse || need_fallback {
                match (parsed.as_ref().map(|(_, v)| v), legacy.as_ref().map(|(_, v)| v)) {
                    (Some(new_v), Some(old_v)) => {
                        if score(old_v) > score(new_v) {
                            parsed = legacy;
                        }
                    }
                    (None, Some(_)) => parsed = legacy,
                    _ => {}
                }
            } else if let (Some(new_v), Some(old_v)) = (parsed.as_ref().map(|(_, v)| v), legacy.as_ref().map(|(_, v)| v)) {
                if !old_v.trim().is_empty() && score(old_v) >= score(new_v) {
                    parsed = legacy;
                }
            }
              if let Some((input, value)) = parsed {
                   if value.is_empty() {
                       att_value = None;
                   } else {
                       let mut parsed_numeric = None;
                       if !force_expr {
                           if let Some(attr_info) = attr_info_map.get(&att_name) {
                               if matches!(&attr_info.default_val, DoubleType(_)) {
                                   if let Ok(num) = value.trim().parse::<f64>() {
                                       parsed_numeric = Some(DoubleType(num));
                                   }
                               }
                           }
                       }
                       att_value = parsed_numeric.or_else(|| Some(StringType(value)));
                   }
                   if is_debug {
                     #[cfg(feature = "debug_parse")]
                     {
                        dbg!(&att_value);
                    }
                }
                residual = input;
            } else {
                println!(
                    "解析{} 表达式属性退出: {:?}, {:#4X?}",
                    refno.to_e3d_id(),
                    &att_name,
                    &residual[..]
                );
                break;
            }
        } else if att_value.is_none() {
            let (l, header) = match parse_explicit_header(&residual[..]) {
                Ok(result) => result,
                Err(_e) => {
                    println!(
                        "解析{} 显式属性退出: {:?}, {:#4X?}",
                        refno.to_e3d_id(),
                        &att_name,
                        &residual[..]
                    );
                    break;
                }
            };
            let attr_type_num = header.type_code;
            let type_len = header.length as usize;
            if type_len * 4 <= l.len() {
                residual = &l[type_len * 4..];
                // 显式属性有可能他给了type但是超了01 后面得长度 所以还要做一层判断
                let tmp_input = &l[..type_len * 4];
                // println!("{:#4X}", explict_hash);
                // dbg!(db1_dehash(explict_hash as u32));
                if attr_info_map.contains_key(&att_name) {
                    let attr_info = attr_info_map.get_mut(&att_name).unwrap();
                    // dbg!(&attr_info.value());
                    // 根据获取到的type hash值，拿到需要的类型
                    match attr_info.default_val {
                        InvalidType => {}
                        IntegerType(_) => {
                            let (_, val) = be_i32(tmp_input)?;
                            att_value = Some(IntegerType(val));
                        }
                        StringType(_) => {
                            let (_, a) = be_u32(tmp_input)?;
                            let len_a = a as usize;
                            if tmp_input.len() > 4 && 4 + len_a <= tmp_input.len() {
                                let (decode_string, _b_chi) =
                                    decode_chars_data(&tmp_input[4..4 + len_a]);
                                att_value = Some(StringType(decode_string.into()));
                            } else {
                                // println!("len_a={:#04X?}", len_a);
                                // println!("error refno={:?}", refno);
                                // println!("error 显示 tmp_input={:#04X?}", tmp_input);
                            }
                        }
                        DoubleType(_) => {
                            let dou_len = tmp_input.len() / 4;
                            if dou_len == 1 {
                                let (_, val) = be_i32(tmp_input)?;
                                att_value = Some(IntegerType(val));
                            } else {
                                let val = parse_to_f64(&tmp_input[..8]);
                                att_value = Some(DoubleType(val));
                            }
                        }
                        DoubleArrayType(_) => {
                            let mut bytes_len = tmp_input.len() / 4;
                            if bytes_len >= 3 {
                                bytes_len -= 1; //去掉一个自身
                                let (tmp_input, data_len) = be_i32(tmp_input)?;
                                let len = data_len as usize;
                                let double_or_float = bytes_len / len;
                                let mut tmp_input = tmp_input;

                                if double_or_float == 2 {
                                    if tmp_input.len() >= 8 {
                                        let mut data = vec![];
                                        for _ in 0..len {
                                            data.push(parse_to_f64(&tmp_input[..8]));
                                            tmp_input = &tmp_input[8..];
                                        }
                                        att_value = Some(DoubleArrayType(data));
                                    } else {
                                        att_value = Some(DoubleArrayType(vec![]));
                                    }
                                } else if double_or_float == 1 {
                                    if tmp_input.len() > 4 {
                                        let mut data = vec![];
                                        for _ in 0..len {
                                            data.push(parse_to_f32(&tmp_input[..4]) as f64);
                                            tmp_input = &tmp_input[4..];
                                        }
                                        att_value = Some(DoubleArrayType(data));
                                    } else {
                                        att_value = Some(DoubleArrayType(vec![]));
                                    }
                                }
                            }
                        }
                        StringArrayType(_) => {
                            let (tmp_input, len) = be_u32(tmp_input)?;
                            let len = len as usize;
                            let mut tmp_input = tmp_input;
                            let mut dehash_strs = vec![];
                            for _ in 0..len {
                                let (remain_input, val) = be_i32(tmp_input)?;
                                dehash_strs.push(db1_dehash(val.abs() as _));
                                tmp_input = remain_input;
                            }
                            att_value = Some(StringArrayType(dehash_strs));
                            // if is_debug && att_name == "ELEL" {
                            //     dbg!(&att_value);
                            // }
                        }
                        BoolArrayType(_) => {}
                        IntArrayType(_) => {
                            let (tmp_input, len) = be_u32(tmp_input)?;
                            let len = len as usize;
                            let mut tmp_input = tmp_input;
                            let mut data = vec![];
                            for _ in 0..len {
                                let (remain_input, val) = be_i32(tmp_input)?;
                                data.push(val);
                                tmp_input = remain_input;
                            }
                            att_value = Some(IntArrayType(data));
                        }
                        BoolType(_) => {
                            let (_, val) = be_u32(tmp_input)?;
                            att_value = Some(BoolType(val != 0));
                        }
                        Vec3Type(_) => {
                            let (l, v) = be_i32(tmp_input)?;
                            let _len = v as usize;
                            let data = parse_to_f64_arr(l, 3).try_into().unwrap();
                            att_value = Some(Vec3Type(data));
                        }
                        ElementType(_) => {
                            let (_, (ref_0, ref_1)) = tuple((be_u32, be_u32))(tmp_input)?;
                            let refno = RefU64::from_two_nums(ref_0, ref_1);
                            att_value = Some(RefU64Type(refno));
                        }
                        WordType(_) => {
                            let (tmp_bytes, val) = be_i32(tmp_input)?;
                            if val >= 0x81BF1 {
                                let val_word = db1_dehash(val as u32);
                                att_value = Some(WordType(val_word.into()));
                            } else if val == 1 {
                                //如果为1时，有个长度信息, 特别是TYPEX
                                let (_, val) = be_i32(tmp_bytes)?;
                                let val_word = db1_dehash(val as u32);
                                att_value = Some(WordType(val_word.into()));
                            }
                        }
                        RefU64Type(_) => {
                            let (_, (ref_0, ref_1)) = tuple((be_u32, be_u32))(tmp_input)?;
                            let refno = RefU64::from_two_nums(ref_0, ref_1);
                            att_value = Some(RefU64Type(refno));
                        }
                        StringHashType(_) => {}
                        RefU64Array(_) => {
                            let (tmp_input, len) = be_u32(tmp_input)?;
                            let len = len as usize;
                            let mut tmp_input = tmp_input;
                            let mut data = vec![];
                            for _ in 0..len {
                                let (remain_input, val) = be_u64(tmp_input)?;
                                data.push(RefU64(val));
                                tmp_input = remain_input;
                            }
                            att_value = Some(RefU64Array(RefU64Vec(data)));
                        }
                    }
                } else {
                    // 这里的逻辑改了一下，先判断是否为表达式，所以之前在这里的表达式判断就注释掉了
                    // 如果DashMap没有对应属性的hash 则调用get_explicit_attr_type进行解析
                    if let Some(attr_type) = get_explicit_attr_type(attr_type_num) {
                        // 根据获取到的type hash值，拿到需要的类型
                        match attr_type {
                            DbAttributeType::INTEGER => {
                                let (_, val) = be_i32(tmp_input)?;
                                att_value = Some(IntegerType(val));
                            }
                            DbAttributeType::DOUBLE => {
                                if tmp_input.len() >= 8 {
                                    let val = parse_to_f64(&tmp_input[..8]);
                                    att_value = Some(DoubleType(val));
                                } else {
                                    let (_, val) = be_i32(tmp_input)?;
                                    att_value = Some(IntegerType(val));
                                }
                            }
                            DbAttributeType::BOOL => {
                                let (_, val) = be_u32(tmp_input)?;
                                att_value = Some(BoolType(val != 0));
                            }
                            DbAttributeType::STRING => {
                                let (_, a) = be_u32(tmp_input)?;
                                let len_a = a as usize;
                                if tmp_input.len() > 4 {
                                    let (decode_string, _b_chi) =
                                        decode_chars_data(&tmp_input[4..4 + len_a]);
                                    // let name_hash = string_lookup.add_str(decode_string.as_str());
                                    // att_value = Some(StringHashType(name_hash));
                                    att_value = Some(StringType(decode_string.into()));
                                } else {
                                    println!("len_a={:#04X?}", len_a);
                                    println!("error refno={:?}", refno);
                                    println!("error 显示 input={:#04X?}", tmp_input);
                                }
                            }
                            DbAttributeType::ELEMENT => {
                                let (_, (ref_0, ref_1)) = tuple((be_u32, be_u32))(tmp_input)?;
                                let refno = RefU64::from_two_nums(ref_0, ref_1);
                                att_value = Some(RefU64Type(refno));
                            }
                            DbAttributeType::WORD => {
                                let (_, val) = be_i32(tmp_input)?;
                                if val >= 0x81BF1 {
                                    let val_word = db1_dehash(val as u32);
                                    att_value = Some(WordType(val_word.into()));
                                } else {
                                    att_value = Some(IntegerType(val));
                                }
                            }
                            DbAttributeType::DIRECTION
                            | DbAttributeType::POSITION
                            | DbAttributeType::ORIENTATION => {
                                let (l, v) = be_i32(tmp_input)?;
                                let _len = v as usize;
                                let data = parse_to_f64_arr(l, 3).try_into().unwrap();
                                att_value = Some(Vec3Type(data));
                            }

                            DbAttributeType::DOUBLEVEC => {
                                let array_len = tmp_input.len() / 4;
                                if array_len >= 2 {
                                    let (mut tmp_input, data_len) = be_i32(tmp_input)?;
                                    let len = data_len as usize;
                                    let double_or_float = (array_len - 1) / len;
                                    if double_or_float == 2 {
                                        let mut data = vec![];
                                        for _ in 0..len {
                                            data.push(parse_to_f64(&tmp_input[..8]));
                                            tmp_input = &tmp_input[8..];
                                        }
                                        att_value = Some(DoubleArrayType(data));
                                    } else if double_or_float == 1 {
                                        let mut data = vec![];
                                        for _ in 0..len {
                                            data.push(parse_to_f32(&tmp_input[..4]) as f64);
                                            tmp_input = &tmp_input[4..];
                                        }
                                        att_value = Some(DoubleArrayType(data));
                                    }
                                }
                            }
                            DbAttributeType::INTVEC => {
                                let (tmp_input, len) = be_u32(tmp_input)?;
                                let len = len as usize;
                                let mut tmp_input = tmp_input;
                                let mut data = vec![];
                                for _ in 0..len {
                                    let (remain_input, val) = be_i32(tmp_input)?;
                                    data.push(val);
                                    tmp_input = remain_input;
                                }
                                att_value = Some(IntArrayType(data));
                            }
                            DbAttributeType::TYPEX => {
                                let (tmp_input, len) = be_u32(tmp_input)?;
                                if len == 1 {
                                    let (_, typex) = be_u32(&tmp_input[..4])?;
                                    // let typex = db1_dehash(typex);
                                    att_value = Some(IntegerType(typex as _));
                                } else {
                                }
                            }
                            DbAttributeType::RefU64Vec => {
                                let (tmp_input, len) = be_u32(tmp_input)?;
                                let len = len as usize;
                                let mut tmp_input = tmp_input;
                                let mut data = vec![];
                                for _ in 0..len {
                                    let (remain_input, val) = be_u64(tmp_input)?;
                                    data.push(RefU64(val));
                                    tmp_input = remain_input;
                                }
                                att_value = Some(RefU64Array(RefU64Vec(data)));
                            }
                            _ => {}
                        }
                    }
                }
            } else {
                break;
            }
        }

        // 如果解析出属性值，添加到结果中，并保存原始hash_val
        if let Some(v) = att_value {
            attr_values.push(ExplicitAttr {
                name: att_name,
                value: v.into(),
                is_uda,
                hash_val,
            });
        }
    }

    Ok((residual, attr_values))
}

fn parse_param_with_index(input: i32) -> String {
    let mut val = String::new();
    if input >= 50 && input < 0x65 {
        let value = input - 50;
        val = format!("DESIGN PARAM {}", value);
    } else if input >= 0x65 && input < 0x1F5 {
        let value = ((((input - 0x64) as f32 + 0.005) * 100.0).round() / 100.0) as i32;
        val = format!("IPARAM {}", value);
    } else if input >= 0x1F5 {
        let value = input - 0x1F4;
        match value {
            0..50 => {
                val = format!("TWICE PARAM {}", value);
            }
            50..0x65 => {
                val = format!("TWICE DESIGN PARAM {}", value - 50);
            }
            0x65..0x1F5 => {
                let value = ((((input - 0x64) as f32 + 0.005) * 100.0).round() / 100.0) as i32;
                val = format!("IPARAM {}", value);
            }
            _ => {}
        }
    } else if input <= 0xFFFFFFFFu32 as i32 {
        val = match_angle_or_return_number(input);
    } else {
        val = format!("PARAM {}", input);
    }
    val
}

/// 获取所有的members
/// 
/// 已迁移到 crate::parser::primitives::parse_members
#[inline]
pub fn parse_attr_members(input: &[u8]) -> IResult<&[u8], RefU64Vec> {
    // 委托给新的 parser::primitives 模块
    let (residual, members) = parse_members(input)?;
    Ok((residual, RefU64Vec(members)))
}

/// 获取该节点的owner
/// 
/// 已迁移到 crate::parser::primitives::parse_owner
pub fn parse_attr_owner(input: &[u8]) -> IResult<&[u8], String> {
    // 委托给新的 parser::primitives 模块
    parse_owner(input)
}

#[inline]
pub fn round_f32(input: f32) -> f32 {
    (input * 100.0).round() / 100.0
}

#[inline]
fn convert_int_to_axis_str(n: i32) -> &'static str {
    match n {
        1 => "X",
        2 => "Y",
        3 => "Z",
        4 => "-X",
        5 => "-Y",
        6 => "-Z",
        _ => "",
    }
}

/// 特殊处理AXIS隐式属性
pub fn parse_to_expression(input: &[u8], default: AttrVal) -> IResult<&[u8], AttrVal> {
    let (res_input, flag) = be_i32(input)?;
    let cnt = input.len() / 4;
    if cnt == 1 {
        let f = flag as f32 / 100.0;
        return Ok((input, StringType(f.to_string())));
    }
    // if cnt < 3 {
    //     return Err(nom::Err::Incomplete(nom::Needed::Unknown));
    // }
    let mut val = default;
    // dbg!(flag);
    if flag == 2 {
        match &res_input[..8] {
            &[0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x1] => val = AttrVal::StringType("X".into()),
            &[0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x2] => val = AttrVal::StringType("Y".into()),
            &[0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x3] => val = AttrVal::StringType("Z".into()),
            &[0x0, 0x0, 0x0, 0x2, 0x0, 0x0, 0x0, 0x1] => val = AttrVal::StringType("-X".into()),
            &[0x0, 0x0, 0x0, 0x2, 0x0, 0x0, 0x0, 0x2] => val = AttrVal::StringType("-Y".into()),
            &[0x0, 0x0, 0x0, 0x2, 0x0, 0x0, 0x0, 0x3] => val = AttrVal::StringType("-Z".into()),

            &[0x0, 0x0, 0x0, 0x3, 0x0, 0x0, 0x0, 0x0] => val = AttrVal::StringType("P0".into()),
            &[0x0, 0x0, 0x0, 0x3, 0x0, 0x0, 0x0, 0x1] => val = AttrVal::StringType("P1".into()),
            &[0x0, 0x0, 0x0, 0x3, 0x0, 0x0, 0x0, 0x2] => val = AttrVal::StringType("P2".into()),
            &[0x0, 0x0, 0x0, 0x3, 0x0, 0x0, 0x3, 0xE8] => val = AttrVal::StringType("-P0".into()),
            &[0x0, 0x0, 0x0, 0x3, 0x0, 0x0, 0x3, 0xE9] => val = AttrVal::StringType("-P1".into()),
            &[0x0, 0x0, 0x0, 0x3, 0x0, 0x0, 0x3, 0xEA] => val = AttrVal::StringType("-P2".into()),

            &_ => {
                match &res_input[..3] {
                    &[0xFF, 0xFF, 0xFF] => {
                        let (_, radius) = be_i32(&res_input[4..8])?;
                        let radius = radius / 100;
                        let mut result = String::new();
                        match &res_input[3..4] {
                            &[0xF4] => {
                                result = format!("X {} Y", radius);
                            }
                            &[0xF3] => {
                                result = format!("X {} Z", radius);
                            }
                            &[0xF1] => {
                                result = format!("X {} -Y", radius);
                            }
                            &[0xF0] => {
                                result = format!("X {} -Z", radius);
                            }
                            &[0xEB] => {
                                result = format!("Y {} X", radius);
                            }
                            &[0xE9] => {
                                result = format!("Y {} Z", radius);
                            }
                            &[0xE8] => {
                                result = format!("Y {} -X", radius);
                            }
                            &[0xE6] => {
                                result = format!("Y {} -Z", radius);
                            }
                            &[0xE1] => {
                                result = format!("Z {} X", radius);
                            }
                            &[0xE0] => {
                                result = format!("Z {} Y", radius);
                            }
                            &[0xDE] => {
                                result = format!("Z {} -X", radius);
                            }
                            &[0xDD] => {
                                result = format!("Z {} -Y", radius);
                            }

                            &[0xD6] => {
                                result = format!("-X {} Y", radius);
                            }
                            &[0xD5] => {
                                result = format!("-X {} Z", radius);
                            }
                            &[0xD3] => {
                                result = format!("-X {} -Y", radius);
                            }
                            &[0xD2] => {
                                result = format!("-X {} -Z", radius);
                            }
                            &[0xCD] => {
                                result = format!("-Y {} X", radius);
                            }
                            &[0xCB] => {
                                result = format!("-Y {} Z", radius);
                            }
                            &[0xCA] => {
                                result = format!("-Y {} -X", radius);
                            }
                            &[0xC8] => {
                                result = format!("-Y {} -Z", radius);
                            }
                            &[0xC3] => {
                                result = format!("-Z {} X", radius);
                            }
                            &[0xC2] => {
                                result = format!("-Z {} Y", radius);
                            }
                            &[0xC0] => {
                                result = format!("-Z {} -X", radius);
                            }
                            &[0xBF] => {
                                result = format!("-Z {} -Y", radius);
                            }
                            &_ => {}
                        }
                        val = StringType(result.into());
                    }
                    &_ => {}
                }
                //todo use dynfmt
                match &res_input[..4] {
                    &[0x0, 0x0, 0x0, 0x3] => {
                        // let (_, value) = be_u8(&tmp_input[7..8])?;
                        let (_, value) = be_u32(&res_input[4..8])?;
                        val = StringType(format!("P{}", value).into());
                        if value > 1000 {
                            let value = value - 1000;
                            val = StringType(format!("-P{}", value).into());
                        }
                    }
                    &[0x0, 0x0, 0x0, 0xC] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("X {} Y", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0xD] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("X {} Z", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0xE] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("X {} -X", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0xF] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("X {} -Y", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x10] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("X {} -Z", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x15] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("Y {} X", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x17] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("Y {} Z", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x18] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("Y {} -X", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x19] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("Y {} -Y", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x1A] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("Y {} -Z", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x1F] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("Z {} X", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x20] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        // dbg!(&value);
                        let result = format!("Z {} Y", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x22] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("Z {} -X", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x23] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("Z {} -Y", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x24] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("Z {} -Z", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x29] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-X {} -X", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x2A] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-X {} Y", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x2B] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-X {} Z", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x2D] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-X {} -Y", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x2E] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-X {} -Z", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x33] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-Y {} X", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x34] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-Y {} Y", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x35] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-Y {} Z", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x36] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-Y {} -X", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x38] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-Y {} -Z", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x3D] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-Z {} X", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x3E] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-Z {} Y", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x3F] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-Z {} Z", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x40] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-Z {} -X", value);
                        val = StringType(result.into());
                    }
                    &[0x0, 0x0, 0x0, 0x41] => {
                        let value = get_expression_angle_or_param(&res_input[4..8])?.1;
                        let result = format!("-Z {} -Y", value);
                        val = StringType(result.into());
                    }
                    _ => {}
                }
            }
        }
    } else if flag == 4 {
        let mut param1 = String::new();
        // 目前的推论是 0x28代表符号部分 ，0x1代表数字部分
        match &res_input[..2] {
            &[0x0, 0x0] => {
                let (_, times) = be_i16(&res_input[2..4])?;
                let times = times_keep_f32_three_decimal_place(times as i32);
                if times == 1.0 {
                    param1 = "PARAM".to_string();
                } else if times == 0.0 {
                    param1 = "".to_string();
                } else {
                    param1 = format!("{} TIMES PARAM", times);
                }
            }
            &[0xFF, 0xFF] => {
                let (_, times) = be_i16(&res_input[2..4])?;
                if times > 0xFFFFu16 as i16 {
                    let mut times = ((0xFFFFu16 as i16) as f32 - times as f32 - 1.0) / 40.0f32;
                    times = (times * 100.0_f32).round() / 100.0;
                    param1 = format!("{} TIMES PARAM", times);
                } else {
                    let mut times = (times as f32 - (0xFFFFu16 as i16) as f32 - 1.0) / 40.0f32;
                    times = (times * 100.0_f32).round() / 100.0;
                    param1 = format!("{} TIMES PARAM", times);
                }
            }
            _ => {}
        }
        match &res_input[4..8] {
            &[0x0, 0x0, 0x0, 0x1] => {
                let (_, value) = be_i32(&res_input[8..12])?;
                if value >= 50 && value < 0x65 {
                    let value = value - 50;
                    param1 = param1.replace("PARAM", ""); // 防止出现两个para
                    param1 = format!("{} DESIGN PARAM {}", param1, value);
                } else if value >= 500 && value < 0x3E9 {
                    let value = value - 500;
                    if value < 50 {
                        param1 = format!("TWICE PARAM {}", value);
                    } else {
                        let value = value - 100;
                        param1 = format!("TWICE IPARAM {}", value);
                    }
                } else if value >= 0x65 && value < 0x3E9 {
                    // PARAM 数值大于 0x65 就是 IPARAM
                    let value = value - 0x64;
                    param1 = format!("IPARAM {}", value);
                } else if value >= 0x3E9 {
                    let value = value - 0x3E8;
                    if value >= 0x65 {
                        let value = value - 0x64;
                        param1 = format!("- IPARAM {}", value);
                    } else {
                        param1 = format!("- {} {}", param1, value);
                    }
                } else if value <= 0xFFFFFFFFu32 as i32 {
                    let (_, t) = be_i32(&res_input[..4])?;
                    // times是除以0x28的倍数
                    if t == 0x28 {
                        param1 = match_angle_or_return_number(parse_to_i32(&res_input[8..12]));
                    } else {
                        let mut value = "".to_string();
                        let v = t as f32 / 40.0;
                        let times = round_f32(v);
                        value = get_implicit_angle_expression(&res_input[8..12]);
                        if value == "" {
                            let v = parse_to_i32(&res_input[8..12]);
                            if v != 0 {
                                value = (-v as f32 / 10.0).to_string();
                            }
                        }
                        param1 = format!("{} TIMES {}", times, value);
                    }
                } else {
                    param1 = format!("{} {}", param1, value);
                }
            }
            &[0x0, 0x0, 0x0, 0x2] => {
                let (_, value) = be_i32(&res_input[8..12])?;
                if value > 1000 {
                    let value = value - 1000;
                    match &res_input[12..16] {
                        &[0xFF, 0xFF, 0xFF, 0xFB] => {
                            param1 = format!("TANF - {} {} DDHEIGHT", param1, value);
                        }
                        &[0xFF, 0xFF, 0xFF, 0xFC] => {
                            param1 = format!("TANF - {} {} DDANGLE", param1, value);
                        }
                        _ => {}
                    }
                } else {
                    let angle = get_implicit_angle_expression(&res_input[8..12]);
                    if angle != "" {
                        match &res_input[12..16] {
                            &[0xFF, 0xFF, 0xFF, 0xFB] => {
                                param1 = format!("TANF {} DDHEIGHT", angle);
                            }
                            &[0xFF, 0xFF, 0xFF, 0xFC] => {
                                param1 = format!("TANF {} DDANGLE", angle);
                            }
                            _ => {}
                        }
                    } else {
                        match &res_input[12..16] {
                            &[0xFF, 0xFF, 0xFF, 0xFB] => {
                                param1 = format!("TANF {} {} DDHEIGHT", param1, value);
                            }
                            &[0xFF, 0xFF, 0xFF, 0xFC] => {
                                param1 = format!("TANF {} {} DDANGLE", param1, value);
                            }
                            _ => {}
                        }
                    }
                }
            }
            &[0x0, 0x0, 0x0, 0x3] => {
                let (_, times) = be_i32(&res_input[..4])?;
                let times = times_keep_f32_three_decimal_place(times);
                let (_, (value1, value2)) = tuple((be_i32, be_i32))(&res_input[8..16])?;
                param1 = parse_param_with_index(value1);
                let result = parse_param_with_index(value2);
                if times != 1.0 {
                    param1 = format!("{} TIMES DIFFERENCE {} {}", times, param1, result);
                } else {
                    param1 = format!("DIFFERENCE {} {}", param1, result);
                }
            }

            &[0x0, 0x0, 0x0, 0x4] => {
                let (_, times) = be_i32(&res_input[..4])?;
                let times = times_keep_f32_three_decimal_place(times);
                let (_, (value1, value2)) = tuple((be_i32, be_i32))(&res_input[8..16])?;

                param1 = parse_param_with_index(value1);
                let param2 = parse_param_with_index(value2);
                if times != 1.0 {
                    param1 = format!("{} TIMES SUM {} {}", times, param1, param2);
                } else {
                    param1 = format!("SUM {} {}", param1, param2);
                }
            }
            //代表是WORD
            &[0x0, 0x0, 0x0, 0x7] => {
                let (_, n0) = be_u32(&res_input[8..12])?;
                let (_, n1) = be_u32(&res_input[12..16])?;
                let hash: u32 = format!("{n0}{n1}").parse().unwrap_or_default();
                param1 = db1_dehash(hash);
            }
            &[0x0, 0x0, 0x0, 0x8] => {
                let (_, times) = be_i32(&res_input[..4])?;
                let times = times_keep_f32_three_decimal_place(times);
                let (_, (value1, value2)) = tuple((be_i32, be_i32))(&res_input[8..16])?;
                param1 = parse_param_with_index(value1);
                let result = parse_param_with_index(value2);
                if times != 1.0 {
                    param1 = format!("{} TIMES SUM {} {}", times, param1, result);
                } else {
                    param1 = format!("MULT {} {}", param1, result);
                }
            }
            &[0x0, 0x0, 0x0, 0x9] => {
                let (_, times) = be_i32(&res_input[..4])?;
                let times = times_keep_f32_three_decimal_place(times);
                let (_, (value1, value2)) = tuple((be_i32, be_i32))(&res_input[8..16])?;
                param1 = parse_param_with_index(value1);
                let result = parse_param_with_index(value2);
                if times != 1.0 {
                    param1 = format!("{} TIMES SUM {} {}", times, param1, result);
                } else {
                    param1 = format!("DIV {} {}", param1, result);
                }
            }

            _ => {}
        }
        if res_input.len() > 24 {
            let value = get_implicit_angle_expression(&res_input[16..20]);
            param1 = format!("{} {}", param1, value);
        }
        return Ok((input, StringType(param1.into())));
    }

    // 🔧 修复：如果现有逻辑无法解析，尝试使用 decode_expression_payload 解析
    // 这可以正确处理 ATTRIB DESP[1] 等表达式
    let is_empty_result = match &val {
        StringType(s) => s.trim().is_empty(),
        _ => false,
    };
    if is_empty_result || matches!(val, InvalidType) {
        if let Ok((consumed, expr_str)) = decode_expression_payload(input) {
            if !expr_str.trim().is_empty() && consumed > 0 {
                return Ok((input, StringType(expr_str.into())));
            }
        }
    }

    Ok((input, val))
}

/// 特殊处理AXIS显式属性
pub fn convert_to_explicit_axis_string(input: &[u8], refno: RefU64) -> IResult<&[u8], AttrVal> {
    let mut result = StringType("".into());
    #[cfg(debug_parse_expr)]
    println!("explicit axis: {}", pretty_hex(&input));
    if input.len() < 20 {
        let (_, val) = parse_to_expression(input, StringType("".to_owned()))?;
        result = val;
    } else {
        // 检测是否以 1A 1A 05 02 17 开头
        let (tmp_input, (_a, _b, _c, d, e)) =
            tuple((be_u32, be_u32, be_u32, be_u32, be_u32))(input)?;
        match [d, e] {
            // 0x16 开头就是 X () Y ... 两个坐标的类型
            // 0x2 0x16 后面第一个就是 X Y Z 这三种坐标
            [0x2, 0x16] => {
                let (tmp_input, mut first_data) = parse_xyz_data(tmp_input, refno, false)?;
                if first_data.starts_with("-") {
                    first_data = format!("AXIS {}", first_data);
                }
                let second = match_axis(parse_to_u32(&tmp_input[..4]));
                result = StringType(format!("{}{}", first_data, second) );
            }
            // 0x17 开头代表是 X () Y () Z 这种类型
            [0x2, 0x17] => {
                let (tmp_input, mut first_data) = parse_xyz_data(tmp_input, refno, false)?;
                if first_data.starts_with("-") {
                    first_data = format!("AXIS {}", first_data);
                }
                let (tmp_input, second_data) = parse_xyz_data(tmp_input, refno, false)?;
                let third = match_axis(parse_to_u32(&tmp_input[..4]));
                result = StringType(format!("{}{}{}", first_data, second_data, third) );
            }
            [0x2, 0x22] => {
                let (_, (a, b, c, d)) = tuple((be_u32, be_u32, be_u32, be_u32))(tmp_input)?;
                // dbg!((a, b, c, d));
                if a == 0x52 && b == 0x3 && c == 0x7 {
                    result = StringType(format!("PP {}", d) );
                }
            }
            [0x2, 0x34] => {
                // 防御：catalogue 等库文件中偶见截断的显式块；避免切片越界导致 panic。
                if tmp_input.len() >= 8 {
                    if let Some((func, count)) = match_to_dir(parse_to_u32(&tmp_input[4..8])) {
                        let mut v = func.to_string();
                        v.push_str(" ");
                        let mut axis_data = &tmp_input[8..];
                        for _i in 0..count {
                            let (residual, coord) = parse_xyz_data(axis_data, refno, true)?;
                            // dbg!(&coord);
                            v.push_str(&coord);
                            axis_data = residual;
                        }
                        // dbg!(&v);
                        result = StringType(v);
                    }
                }
            }
            _ => {
                if tmp_input.len() >= 8 {
                    match &tmp_input[..8] {
                        &[0x0, 0x0, 0x0, 0xB, 0x0, 0x0, 0x0, 0x3D] => {
                            result = StringType("X".into())
                        }
                        &[0x0, 0x0, 0x0, 0xC, 0x0, 0x0, 0x0, 0x3D] => {
                            result = StringType("-X".into())
                        }
                        &[0x0, 0x0, 0x0, 0xD, 0x0, 0x0, 0x0, 0x3D] => {
                            result = StringType("Y".into())
                        }
                        &[0x0, 0x0, 0x0, 0xE, 0x0, 0x0, 0x0, 0x3D] => {
                            result = StringType("-Y".into())
                        }
                        &[0x0, 0x0, 0x0, 0xF, 0x0, 0x0, 0x0, 0x3D] => {
                            result = StringType("Z".into())
                        }
                        &[0x0, 0x0, 0x0, 0x10, 0x0, 0x0, 0x0, 0x3D] => {
                            result = StringType("-Z".into())
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Ok((input, result))
}

/// match ptcdirection 的方法
pub fn match_to_dir(key: u32) -> Option<(&'static str, usize)> {
    match key {
        0x1F => Some(("TO", 1)),
        0x20 => Some(("TO", 2)),
        0x21 => Some(("TO", 3)),
        _ => None,
    }
}

/// match AXIS显式属性对应的值
#[inline]
pub fn match_axis(key: u32) -> String {
    match key {
        0xB => "X".to_string(),
        0xC => "-X".to_string(),
        0xD => "Y".to_string(),
        0xE => "-Y".to_string(),
        0xF => "Z".to_string(),
        0x10 => "-Z".to_string(),
        _ => " ".to_string(),
    }
}

/// 检查是否是Axis属性
#[inline]
pub fn check_is_expr(noun: i32) -> bool {
    //todo make stable
    if EXPR_ATT_SET.contains(&noun) || noun < 0 {
        true
    } else {
        false
    }
}

/// 隐式表达式解析，给一个字符串返回DDHEIGHT这种表达式
#[inline]
pub fn get_implicit_angle_expression(input: &[u8]) -> String {
    let mut val = String::new();
    match input {
        &[0xFF, 0xFF, 0xFF, 0xFB] => {
            val = "DDHEIGHT".to_string();
        }
        &[0xFF, 0xFF, 0xFF, 0xFC] => {
            val = "DDANGLE".to_string();
        }
        &[0xFF, 0xFF, 0xFF, 0xFD] => {
            val = "DDRADIUS".to_string();
        }
        _ => {}
    }
    // db1_dehash(convert_to_hash)
    val
}

#[inline]
pub fn is_desi_noun(bytes: &[u8]) -> bool {
    bytes == [0x0, 0xB, 0x6, 0x92].as_slice()
}

#[inline]
pub fn is_cata_noun(bytes: &[u8]) -> bool {
    bytes == [0x0, 0x8, 0xA1, 0xE6].as_slice()
}

///保存 noun_hash->refno  map
pub fn save_type_hash_file(dir: &str, out_name: &str) -> Result<()> {
    let mut unique_hash_refno_map = DashMap::new();
    let mut path_buf = fs::read_dir(dir)?
        .into_iter()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| is_pdms_db_file(p))
        .collect::<Vec<PathBuf>>();
    path_buf.sort_by(|a, b| {
        fs::metadata(b)
            .map(|m| m.len())
            .unwrap_or_default()
            .partial_cmp(&fs::metadata(a).map(|m| m.len()).unwrap_or_default())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for path in path_buf {
        let meta = fs::metadata(&path)?;
        if meta.len() < 36 {
            println!("skip short file (len < 36): {:?}", path);
            continue;
        }
        let mut file = File::open(&path).context(format!("open db file {:?}", path))?;
        let mut buf = vec![0u8; 36];
        if let Err(e) = file.read_exact(&mut buf) {
            println!("skip unreadable file {:?}, err: {}", path, e);
            continue;
        }
        let _input = &buf[32..36];
        let start = Instant::now();
        println!("path={:?}", path);
        let mut buf: Vec<u8> = Vec::new();
        file.read_to_end(&mut buf).context("read db body")?;
        let time = start.elapsed();
        println!("read {:?} finished in {:?}", path, time);
        process_type_hash(&buf[..], &mut unique_hash_refno_map, &path);
        println!("noun_hash_refnos len = {:?}", unique_hash_refno_map.len());
        let encode = serde_json::to_vec(&unique_hash_refno_map)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(out_name)
            .context(format!("open output file {}", out_name))?;
        file.write(&encode)?;
    }
    Ok(())
}

/// 简单判定是否为 PDMS/E3D 数据文件，避免解析非目标文件
fn is_pdms_db_file(path: &Path) -> bool {
    // 跳过常见的非 db 扩展名
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        let ext = ext.to_ascii_lowercase();
        if matches!(ext.as_str(), "com" | "mis" | "txt" | "log" | "json" | "rs") {
            return false;
        }
        if matches!(ext.as_str(), "db" | "sys" | "mdb" | "pdms") {
            return true;
        }
    }
    // 无扩展名：只要文件足够大（后续再按长度/头部过滤）
    true
}

/// 检查文件头是否包含有效的 PDMS 数据库类型
///
/// 委托给 `validation::is_valid_db_header`，避免代码重复
#[inline]
fn check_path_db_header(path: &Path) -> bool {
    std::fs::File::open(path)
        .and_then(|mut f| {
            let mut header = [0u8; 64];
            f.read(&mut header)?;
            Ok(is_valid_db_header(&header))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests_filters {
    use super::{check_path_db_header, is_pdms_db_file};
    use aios_core::tool::db_tool::db1_hash;
    
    use tempfile::tempdir;

    #[test]
    fn test_is_pdms_db_file_by_extension() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("foo.db");
        std::fs::write(&db_path, b"dummy").unwrap();
        let txt_path = dir.path().join("bar.txt");
        std::fs::write(&txt_path, b"dummy").unwrap();
        assert!(is_pdms_db_file(&db_path));
        assert!(!is_pdms_db_file(&txt_path));
    }

    #[test]
    fn test_check_path_db_header_desi() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("desi.db");
        let mut data = vec![0u8; 64];
        let hash = db1_hash("DESI") as i32;
        data[32..36].copy_from_slice(&hash.to_be_bytes());
        std::fs::write(&path, &data).unwrap();
        assert!(check_path_db_header(&path));
    }

    #[test]
    fn test_check_path_db_header_unknown() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("unknown.db");
        let mut data = vec![0u8; 64];
        data[32..36].copy_from_slice(b"FAKE");
        std::fs::write(&path, &data).unwrap();
        assert!(!check_path_db_header(&path));
    }

    #[test]
    fn test_check_path_db_header_short_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("short.db");
        std::fs::write(&path, b"short").unwrap();
        assert!(!check_path_db_header(&path));
    }
}

#[cfg(test)]
mod tests_attr_members {
    use super::parse_attr_members;
    use nom::error::ErrorKind;

    #[test]
    fn parse_simple_members() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(1u64.to_be_bytes()));
        bytes.extend_from_slice(&(2u64.to_be_bytes()));
        let (residual, members) = parse_attr_members(&bytes).unwrap();
        assert!(residual.is_empty());
        assert_eq!(members.len(), 2);
        assert_eq!(members.get(0).unwrap().0, 1);
        assert_eq!(members.get(1).unwrap().0, 2);
    }

    #[test]
    fn parse_members_reject_partial() {
        let bytes = vec![0xAA, 0xBB, 0xCC]; // not multiple of 8
        let err = parse_attr_members(&bytes).unwrap_err();
        let kind = match err {
            nom::Err::Error(e) | nom::Err::Failure(e) => e.code,
            _ => ErrorKind::Fail,
        };
        assert_eq!(kind, ErrorKind::LengthValue);
    }
}

#[cfg(test)]
mod tests_explicit_segments {
    use super::collect_explict_data;
    use aios_core::types::RefU64;

    fn make_explicit_block(refno: RefU64, payload: &[u8]) -> Vec<u8> {
        assert_eq!(payload.len() % 4, 0);
        let mut data = Vec::new();
        data.extend_from_slice(&1u16.to_be_bytes()); // flag
        let total_bytes = 4 + 8 + payload.len();
        let len_words = total_bytes / 4;
        data.extend_from_slice(&(len_words as u16).to_be_bytes());
        data.extend_from_slice(&(refno.get_0() as i32).to_be_bytes());
        data.extend_from_slice(&(refno.get_1() as i32).to_be_bytes());
        data.extend_from_slice(payload);
        data
    }

    #[test]
    fn test_collect_explict_data_with_segment() {
        let refno = RefU64::from_two_nums(1, 2);
        let base_payload = vec![0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44];
        let mut data = make_explicit_block(refno, &base_payload);

        let seg_payload = vec![0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC];
        let seg_len_words: u16 = 7; // 28 bytes
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x07, 0x00, 0x01]);
        data.extend_from_slice(&seg_len_words.to_be_bytes());
        data.extend_from_slice(&(refno.get_0() as i32).to_be_bytes());
        data.extend_from_slice(&(refno.get_1() as i32).to_be_bytes());
        data.extend_from_slice(&0i32.to_be_bytes());
        data.extend_from_slice(&0i32.to_be_bytes());
        data.extend_from_slice(&seg_payload);

        let result = collect_explict_data(&data, refno);
        let mut expected = base_payload;
        expected.extend_from_slice(&seg_payload);
        assert_eq!(result, expected);
    }
}

///处理type_hash对应的refno位置信息
fn process_type_hash<'a>(
    input: &'a [u8],
    type_hash: &mut DashMap<i32, (RefU64, String)>,
    path: &PathBuf,
) -> bool {
    let refno_0_set = get_total_refno_0s(input);
    let path = Path::new(path);
    let file_name: String = path
        .file_name()
        .unwrap()
        .to_owned()
        .to_string_lossy()
        .to_string()
        .into();
    let _noun_map = get_default_pdms_db_info();
    refno_0_set.par_iter().for_each(|ref_0| {
        let pos_iter = rfind_iter(&input, ref_0);
        for p in pos_iter {
            if let Some(refno_entry) = get_refno_entry(input, p) {
                type_hash
                    .entry(refno_entry.1.noun_hash)
                    .or_insert((refno_entry.0, file_name.clone()));
                break; //if found, just break
            }
        }
    });
    true
}

///获取所有不同的 refno_0
pub fn get_total_refno_0s(input: &[u8]) -> HashSet<&[u8]> {
    let mut refno_0_set = HashSet::new();
    let mut pos_iter = rfind_iter(&input, &REFNO_ALL_INDEX_PAGE[..]);
    while let Some(i) = pos_iter.next() {
        let mut j = i + 0x6 * 4; //偏移6 dword
        let mut d = &input[j..j + 4];
        while d != [0, 0, 0, 0].as_slice() {
            refno_0_set.insert(d);
            j += 0x4 * 4;
            d = &input[j..j + 4];
        }
    }
    refno_0_set
}

pub fn get_expression_angle_or_param(input: &[u8]) -> IResult<&[u8], String> {
    let mut value = get_implicit_angle_expression(input);
    if value == "".to_string() {
        value = parse_param_with_index(parse_to_i32(input));
    }
    Ok((input, value))
}

#[derive(Debug, Default)]
pub struct DbBasicInfo {
    pub db_type: String,
    pub ses_pgno: u32,
    pub dbnum: u32,
}

/// 获取文件的type和ses_pgno, db number
pub fn parse_db_basic_info(path: PathBuf) -> DbBasicInfo {
    let mut file = File::open(&path).unwrap();
    let mut buf = vec![0u8; 60];
    file.read_exact(&mut buf).unwrap();
    parse_file_basic_info(&buf)
}

/// 获取文件的type和ses_pgno, db number
pub fn parse_file_basic_info(input: &[u8]) -> DbBasicInfo {
    let t = parse_to_u32(&input[32..36]);
    let mut file_type = "".to_string();
    if t >= 0x81BF1 {
        file_type = db1_dehash(t);
    }
    let dbnum = extract_db_no(input)
        .map(|v| v as u32)
        .unwrap_or_else(|| {
            if input.len() >= 12 {
                parse_to_u32(&input[8..12])
            } else {
                0
            }
        });
    let ses_pgno = if input.len() >= 44 {
        parse_to_u32(&input[40..44])
    } else {
        0
    };
    DbBasicInfo {
        db_type: file_type,
        ses_pgno,
        dbnum,
    }
}

//直接使用session的会话，读取所有的EleDataEntry, 靠这个搜索出来的不

///获得参考号对应的Entry
#[inline]
fn get_refno_entry(input: &[u8], offset: usize) -> Option<(RefU64, EleDataEntry)> {
    let input = &input[offset - 4..];
    let noun_hash = parse_to_i32(&input[12..16]);
    let mut refno_entry = None;
    let mut is_ok = true;
    if !db1_dehash(noun_hash as u32).is_empty() {
        let (_, (_len, _refno)) = tuple::<_, _, nom::error::Error<&[u8]>, _>((
            be_i32, //len
            be_u64,
        ))(&input[0..12])
        .ok()?;
        let len = parse_to_u32(&input[0..4]);
        let refno = RefU64::from(&input[4..12]);
        let test_refno = get_db_option().get_test_refno().map(|x| x.refno());
        let is_debug = test_refno == Some(refno);
        if len != 0 && (len & 0xFFFF000 == 0) {
            let tmp_pos = len as usize * 4; //隐含属性理论结束点
            let found_0_7 = memmem::find(
                &input[tmp_pos..tmp_pos + 20],
                &[0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x7],
            );
            if is_debug {
                #[cfg(feature = "debug_parse")]
                {
                    dbg!(found_0_7);
                }
            }

            if found_0_7.is_some() {
                //允许一定范围去查找
                if let Some(next_pos) = memmem::find(&input[12..tmp_pos + 20], &input[4..12]) {
                    let end_pos = next_pos + 12 ; //隐含属性实际结束点
                    if end_pos >= tmp_pos + 4 {
                        let diff_len = end_pos - tmp_pos - 4;
                        if is_debug {
                            #[cfg(feature = "debug_parse")]
                            {
                                dbg!(diff_len);
                            }
                        }
                        is_ok = diff_len == 0;
                        if diff_len > 0 && diff_len % 4 == 0 && end_pos > tmp_pos {
                            let s: IResult<&[u8], (Vec<i32>, i32)> = many_till(
                                verify(be_i32, |&x| x == 0),
                                verify(be_i32, |&x| x == 7),
                            ).parse(
                                &input[tmp_pos..end_pos]
                            );
                            if is_debug {
                                #[cfg(feature = "debug_parse")]
                                {
                                    dbg!(&s);
                                }
                            }
                            if s.is_ok() {
                                is_ok = (diff_len / 4) == (s.unwrap().1 .0.len() + 1);
                            }
                        }
                    }
                }
            } else {
                let mem_flag = parse_to_u16(&input[tmp_pos..tmp_pos + 2]);
                if mem_flag == 2 || mem_flag == 1 {
                    is_ok = (&input[tmp_pos + 4..tmp_pos + 12]) == &input[4..12];
                }
            }
        }
        if is_ok {
            refno_entry = Some((
                refno,
                EleDataEntry {
                    pos: offset,
                    noun_hash,
                },
            ));
        }
    }
    refno_entry
}

/// map中将所有offset不为0的值进行排序, 返回Noun hash 的排序
pub fn sort_offsets(map: &DashMap<i32, AttrInfo>) -> Vec<i32> {
    let mut off_map = BTreeMap::new();
    for kv in map {
        let v = kv.value();
        let h = *kv.key();
        if v.offset > 0xFFFF {
            let o = (v.offset >> 0x14) as u32;
            off_map.insert((v.offset & 0xFFFFF) * 100 + o, h);
        } else if v.offset != 0 {
            off_map.insert(v.offset * 100, h);
        }
    }
    off_map.values().cloned().collect()
}

pub fn get_offset_map(map: DashMap<i32, AttrInfo>) -> HashMap<u32, (u32, DbAttributeType)> {
    let mut offset_map = HashMap::new();
    for (_, a) in map {
        if a.offset != 0 {
            offset_map.insert(a.offset, (a.offset, a.att_type));
        }
    }
    offset_map
}

///通过offset获取某个隐式属性的长度
pub fn get_implicit_len_by_offset(count: &Vec<u32>, offset: u32) -> usize {
    if let Some(index) = count.iter().position(|o| *o == offset) {
        return (count[index + 1] - count[index]) as usize;
    }
    0
}

/// 根据i32数据match DDHEIGHT这种表达式，若没有则返回数据
pub fn match_angle_or_return_number(input: i32) -> String {
    let mut result = get_implicit_angle_expression(&input.to_be_bytes());
    if result == "" {
        let value =
            (((0xFFFFFFFFu32 as i32 - input) as f32 / 0xA as f32 + 0.1) * 10.0).round() / 10.0;
        result = value.to_string();
    }
    result
}

pub fn parse_pdms_project_name(input: &str) -> IResult<&str, &str> {
    let (_, name) = take_until("sys")(input)?;
    Ok((input, name))
}

pub const WORLD_NOUN: i32 = 0xBEB83;

pub fn gen_ref_type_pos_table(input: &[u8]) -> (DashMap<RefU64, EleDataEntry>, RefU64) {
    let refno_0_set = get_total_refno_0s(input);
    let refno_table = DashMap::new();
    let word_refno_hashset = DashSet::new();
    refno_0_set.par_iter().for_each(|ref_0| {
        let pos_iter = rfind_iter(&input, ref_0);
        for p in pos_iter {
            //需要检查是否满足要求，前面基本是 0x 00 00 00 xx
            let t = &input[p - 4..p];
            if !(t[0] == 0 && t[1] == 0 && t[2] == 0 && t[3] >= 0x8) {
                continue;
            }
            if let Some((refno, entry)) = get_refno_entry(input, p) {
                //判断是否是World
                if entry.noun_hash == WORLD_NOUN {
                    word_refno_hashset.insert(refno);
                }
                refno_table.entry(refno).or_insert(entry);
            }
        }
    });
    let world_refno = word_refno_hashset.into_iter().next().unwrap_or_default();
    (refno_table, world_refno)
}

/// 获取 ref_no + type 的索引位置表  和 world的参考号
/// 根据get_last_index_position返回的hashset获取所有的ref_no + type的位置
/// 返回值是hashmap k:所有的ref_no v:(ref_no的position,type的hash)
/// 利用这个层级关系去解析数据，加快速度
pub fn gen_ref_type_pos_table_parallel(
    input: &[u8],
    _noun_attr_info_map: &DashMap<String, DashMap<String, AttrInfo>>,
) -> (DashMap<RefU64, EleDataEntry>, RefU64) {
    let get_total_timer = Instant::now();
    let refno_0_set = get_total_refno_0s(input);
    println!(
        "Get total refnos {} ms",
        get_total_timer.elapsed().as_millis()
    );

    let refno_0_set_timer = Instant::now();
    // let mut world_refno = Arc::new(Mutex::new(RefU64::default()));
    let refno_table: DashMap<RefU64, EleDataEntry> = DashMap::new();
    let word_refno_hashset = DashSet::new();
    //todo 需要根据文件大小去优化
    let segs = 4;
    let step_size = input.len() / segs; //处理分段正好落在分割的地方的情况， 前后扩展多 20个 bytes吧
    refno_0_set.par_iter().for_each(|ref_0| {
        //分成32段
        (0..segs).into_par_iter().for_each(|x| {
            let start = if x != 0 { step_size * x - 20 } else { 100 };
            if start < input.len() {
                let data = if x == segs - 1 {
                    &input[start..input.len()] //最后一段
                } else {
                    &input[start..start + step_size]
                };
                if data.len() > 100 {
                    let pos_iter = rfind_iter(data, ref_0);
                    for p in pos_iter {
                        if p < start {
                            continue;
                        }
                        if let Some(refno_entry) = get_refno_entry(data, p - start) {
                            //判断是否是World
                            if refno_entry.1.noun_hash == 0xBEB83 {
                                word_refno_hashset.insert(refno_entry.0);
                            }
                            if refno_table.contains_key(&refno_entry.0) {
                                let d = &*refno_table.get(&refno_entry.0).unwrap();
                                if d.pos < refno_entry.1.pos {
                                    refno_table.insert(refno_entry.0, refno_entry.1);
                                }
                            } else {
                                refno_table.insert(refno_entry.0, refno_entry.1);
                            }
                            // refno_table.entry(refno_entry.0).or_insert(refno_entry.1);
                        }
                    }
                }
            }
        });
    });

    println!(
        "refno_0_set.par_iter costs {} ms",
        refno_0_set_timer.elapsed().as_millis()
    );
    let world_refno = word_refno_hashset.into_iter().next().unwrap_or_default();
    // dbg!(world_refno.to_refno_str());
    (refno_table, world_refno)
}

pub fn get_project_name_from_filename(filename: &str) -> IResult<&str, &str> {
    let (input, p) = alpha1(filename)?;
    Ok((input, p))
}



#[cfg(test)]
mod sync_api_tests {
    use super::*;

    #[tokio::test]
    async fn parse_ele_sync_and_async_wrapper_should_be_consistent() {
        // 32 字节可避免切片越界；该输入会在类型信息阶段报错，适合比较错误一致性。
        let input = [0u8; 32];
        let db_info = get_default_pdms_db_info();

        let sync_err = parse_ele_data_with_info_sync(&input, &db_info)
            .err()
            .map(|e| e.to_string());
        #[allow(deprecated)]
        let async_err = parse_ele_data_with_info(&input, &db_info)
            .await
            .err()
            .map(|e| e.to_string());

        assert_eq!(sync_err, async_err);
    }
}
