//! Adapter 层：将旧 PdmsIO 接口代理到 engine_v2
//!
//! 提供与旧 API 签名兼容的桥接函数，使上游调用方
//! 可以逐步从 PdmsIO 迁移到 engine_v2::Database。

use std::path::Path;

use crate::defines::PdmsHeader;
use crate::engine_v2::db5::database::Database;
use crate::engine_v2::types::*;

/// 将旧 PdmsHeader 转换为新 DbHeader
pub fn from_legacy_header(h: &PdmsHeader) -> DbHeader {
    DbHeader {
        version: h.version,
        db_num: h.db_num,
        flags: h.flags,
        creation_time: h.creation_time,
        latest_ses_pgno: h.latest_ses_pgno,
        ext_no: h.ext_no,
        session_page_no: h.session_page_no,
        page_size: h.page_size,
        stored_page_count: h.stored_page_count,
    }
}

/// 将新 DbHeader 转换回旧 PdmsHeader
pub fn to_legacy_header(h: &DbHeader) -> PdmsHeader {
    PdmsHeader {
        unknown_0_0: 0,
        version: h.version,
        db_num: h.db_num,
        unknown_1_0: 1,
        unknown_1_1: 1,
        unknown_1_2: 0,
        flags: h.flags,
        unknown_1_4: 0,
        creation_time: h.creation_time,
        unknown_2: -1,
        latest_ses_pgno: h.latest_ses_pgno,
        ext_no: h.ext_no,
        session_page_no: h.session_page_no,
        page_size: h.page_size,
        stored_page_count: h.stored_page_count,
        unknown_3: 2,
    }
}

/// 将旧 RefU64 (u64) 转换为新 RefNo
pub fn refno_from_u64(v: u64) -> RefNo {
    RefNo::from_u64(v)
}

/// 将新 RefNo 转换回 u64
pub fn refno_to_u64(r: RefNo) -> u64 {
    r.to_u64()
}

/// V2 兼容接口：只读打开数据库
pub fn open_read(path: impl AsRef<Path>) -> DbResult<Database> {
    Database::open_read(path, 256)
}

/// V2 兼容接口：读写打开数据库
pub fn open_write(path: impl AsRef<Path>) -> DbResult<Database> {
    Database::open_write(path, 256)
}
