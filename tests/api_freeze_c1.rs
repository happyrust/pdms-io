//! specs/002 T201 — `PdmsIO` 公共 API 冻结锁（contracts C1）。
//!
//! 本文件**只做编译期核查**：把 C1 清单上每个公共签名以"调用 + 显式类型绑定"
//! 的形式锁死——Phase 2 换芯若改动任一冻结签名（参数个数/类型、返回类型、
//! 接收者可变性、async 性），本测试目标将**编译失败**。
//!
//! 锁定函数永不执行（`never()` 发散），不需要任何数据库文件。
//! 契约规则（C1）：允许内部私有项重写;禁止公共签名变更与公共类型字段删改。

#![allow(dead_code, unreachable_code, unused_variables, clippy::diverging_sub_expression)]

use std::collections::{BTreeMap, HashMap};
use std::ops::RangeInclusive;
use std::path::Path;

use aios_core::pdms_types::EleOperation;
use aios_core::{NamedAttrValue, RefU64};
use chrono::{DateTime, Utc};
use parse_pdms_db::parse::EleData;
use pdms_io::PdmsIO;
use pdms_io::defines::{DbPageBasicInfo, IndexPageData, PdmsHeader, RefnoDataLoc, SessionPageData};
use pdms_io::io::{EleOperationData, EleOperationDetail, IndexMap};

fn never<T>() -> T {
    unreachable!("type-lock only; never executed")
}

fn assert_future<T, F: std::future::Future<Output = T>>(_f: F) {}

/// C1 §生命周期/基础读取
fn _freeze_lifecycle_and_reads(io: &mut PdmsIO) {
    let _: PdmsIO = PdmsIO::new(never::<String>(), never::<&Path>(), never::<bool>());
    let _: anyhow::Result<()> = io.open();
    let _: anyhow::Result<()> = io.init_ses_range_map();
    let _: anyhow::Result<PdmsHeader> = io.read_pdms_header();
    let _: anyhow::Result<DbPageBasicInfo> = io.get_page_basic_info();
    let _: anyhow::Result<Vec<u8>> = io.read_bytes(never::<u64>(), never::<usize>());
    let _: anyhow::Result<Vec<u8>> = io.read_bytes(never::<i64>(), never::<usize>());
    let _: anyhow::Result<Vec<u8>> = io.read_data_cached(never::<u64>(), never::<usize>());
    let _: anyhow::Result<Vec<u8>> = io.read_element_record_cached(never::<u64>());
    let _: f64 = io.cache_hit_rate();
}

/// C1 §会话
fn _freeze_sessions(io: &mut PdmsIO) {
    let _: Option<u32> = io.get_sesno(never::<u32>());
    let _: anyhow::Result<u32> = io.get_latest_sesno();
    let _: anyhow::Result<u32> = io.get_latest_att_pgno();
    let _: anyhow::Result<DateTime<Utc>> = io.get_latest_dt();
    let _: anyhow::Result<DateTime<Utc>> = io.get_sesno_datetime(never::<u32>());
    let _: anyhow::Result<i64> = io.get_sesno_timestamp(never::<u32>());
    let _: anyhow::Result<&SessionPageData> = io.read_ses_data(never::<u32>());
    let _: anyhow::Result<&SessionPageData> = io.get_ses_data(never::<u32>());
    let _: Option<u32> = io.get_ses_pageno(never::<i32>());
    let _: Option<i32> = io.get_nearest_large_sesno(never::<i32>());
    let _: Option<i32> = io.get_nearest_less_sesno(never::<i32>());
}

/// C1 §索引/查找
fn _freeze_index_and_search(io: &mut PdmsIO) {
    let _: anyhow::Result<IndexPageData> = io.read_index_data(never::<u32>());
    let _: anyhow::Result<IndexMap> = io.build_index_map();
    let _: anyhow::Result<IndexMap> = io.build_index_map_default();
    let _: anyhow::Result<IndexMap> = io.build_index_map_verbose(never::<bool>());
    let _: anyhow::Result<()> = io.cache_index_map(never::<&Path>(), never::<&IndexMap>());
    let _: anyhow::Result<IndexMap> = io.load_cached_index_map(never::<&Path>());
    let _: Option<(u32, u64)> = io.search_latest_refno(never::<RefU64>(), never::<Option<u32>>());
    let _: [Option<(u32, u64)>; 2] =
        io.search_latest_and_prev_refno(never::<RefU64>(), never::<Option<u32>>());
    let _: Option<(u32, u64)> =
        io.search_in_leaf_node(never::<&[RefnoDataLoc]>(), never::<u32>(), never::<u32>());
    let _: anyhow::Result<Option<RefnoDataLoc>> =
        io.find_refno_loc(never::<RefU64>(), never::<u32>());
    let _: anyhow::Result<bool> = io.check_refno_exists(never::<RefU64>());
    let _: anyhow::Result<RefnoDataLoc> = io.search_refno_pgno_optimized(never::<RefU64>());
}

/// C1 §索引/查找（生命周期敏感的 fast_lookup 族）
fn _freeze_fast_lookup<'a>(io: &PdmsIO, index_map: &'a IndexMap, refno: &RefU64) {
    let _: Option<&'a Vec<u64>> = io.fast_lookup_refno(refno, index_map);
    let _: Option<u64> = io.fast_lookup_latest_loc(refno, index_map);
}

/// C1 §元素/增量/历史
fn _freeze_elements_and_increments(io: &mut PdmsIO) {
    let _: anyhow::Result<EleData> = io.parse_raw_element(never::<u64>());
    let _: anyhow::Result<EleData> = io.auto_get_raw_element(never::<RefU64>());
    let _: Vec<RefnoDataLoc> = io.collect_refno_locs(never::<i32>());
    let _: Vec<RefnoDataLoc> = io.collect_refno_locs_in_session(never::<u32>());
    let _: Option<bool> = io.filter_index_data(
        never::<&IndexPageData>(),
        never::<&mut Vec<RefnoDataLoc>>(),
        never::<u32>(),
        never::<u32>(),
    );
    let _: anyhow::Result<BTreeMap<u32, Vec<EleOperationData>>> =
        io.collect_increment_eles(never::<Option<RangeInclusive<i32>>>());
    let _: anyhow::Result<BTreeMap<u32, Vec<EleOperationData>>> =
        io.collect_recent_n_sessions_eles(never::<Option<u32>>());
    let _: Vec<EleData> = io.collect_ele_history(never::<RefU64>());
    let _: anyhow::Result<HashMap<RefU64, EleOperationDetail>> =
        io.get_refno_operation_status(never::<RefU64>(), never::<Option<u32>>());
    let _: anyhow::Result<EleOperation> =
        io.get_refno_primary_operation_status(never::<RefU64>(), never::<Option<u32>>());
    let _: anyhow::Result<BTreeMap<u32, u64>> =
        io.search_history_refnos(never::<RefU64>(), never::<Option<u32>>());
    let _: anyhow::Result<NamedAttrValue> = io.get_attribute_value(
        never::<RefU64>(),
        never::<u32>(),
        never::<u32>(),
        never::<u32>(),
        never::<u32>(),
    );
    let _: anyhow::Result<HashMap<u32, Vec<u32>>> =
        PdmsIO::build_noun_attr_map(never::<&Path>());
}

/// C1 §异步项（签名含 async 性一并锁定）
fn _freeze_async(io: &mut PdmsIO) {
    assert_future::<anyhow::Result<()>, _>(io.update_elements_to_database(
        never::<&BTreeMap<u32, Vec<EleOperationData>>>(),
        never::<bool>(),
    ));
    // crate 根再导出（lib.rs: pub use io::{PdmsIO, benchmark_increment_eles}）
    assert_future::<anyhow::Result<()>, _>(pdms_io::benchmark_increment_eles(never::<&str>()));
}

/// 运行时仅证明本测试目标被编译过（真正的断言全部发生在编译期）。
#[test]
fn api_freeze_c1_is_compile_time() {}
