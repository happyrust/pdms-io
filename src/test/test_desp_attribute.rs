//! 测试 DESP 属性解析
//!
//! 验证 `ams1112_0001` 中 `refno=17496/171603` 的 `DESP`（显式 DOUBLEVEC）不为空。

use crate::io::PdmsIO;
use aios_core::RefU64;
use parse_pdms_db::parse::parse_raw_ele_data;
use std::path::Path;
use crate::test::resolve_test_db_path;

/// DESP 属性的 hash 值（db1_hash("DESP")）
const DESP_HASH: i32 = 0x000D20C7;
