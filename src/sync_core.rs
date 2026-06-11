//! specs/006 —— 同步核心(T101 `scan_targets` / T102 `sync_db`)。
//!
//! 契约 `specs/006-e3d-sync-daemon/contracts/sync-daemon-contract.md` G1:
//! - A1:目录扫描识别 `*_0001` 库文件,读头取 dbnum(读失败记跳过,不报错);
//! - A2:逐库**纯水位驱动**——latest ≤ 水位 ⇒ Skipped;latest > 水位 ⇒ 范围增量经
//!   唯一入口 ingest;初见库(无水位)⇒ 仅以最新会话立基线,不回灌历史;
//! - A3:无本地状态;同一文件状态重复执行 = 幂等(003 D2 保证);
//! - A4:初灌走显式 bootstrap(`collect_and_save_latest_data`,既有唯一入口)。
//!
//! 故障隔离(G3):单库错误折叠为 `Failed(String)`,进程层面由调用方(守护壳)记日志继续。
//! 本模块零新落库构造点(G4 红线):全部写库经 `update_elements_to_database`。

use std::path::{Path, PathBuf};

use crate::io::PdmsIO;
use walkdir::WalkDir;

/// 扫描产物:一个待同步的库目标(G1-A1)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbTarget {
    pub path: PathBuf,
    pub dbnum: i32,
}

/// 单库同步结果(G1-A2/G3-A1 的可观测单元)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbSyncOutcome {
    /// 新增会话已入库(sessions = 提取到的增量会话数)。
    Synced { from_sesno: u32, to_sesno: u32, sessions: usize },
    /// 文件最新会话 ≤ 水位,零写库。
    SkippedUpToDate { sesno: u32 },
    /// 初见库:以最新会话立基线(不回灌历史)。
    Baseline { sesno: u32, ops: usize },
    /// 该库本轮失败(隔离;下轮重试)。
    Failed(String),
}

/// 头部 db_num 字段偏移(`PdmsHeader` 0x08,大端 i32;defines.rs 同源)。
const HDR_DBNUM_OFF: usize = 0x08;

/// G1-A1:识别 `dirs` 下全部 `*_0001` 库文件并读头取 dbnum;
/// `whitelist` 给定时仅保留命中的 dbnum。读头失败的文件静默跳过。
pub fn scan_targets(dirs: &[PathBuf], whitelist: Option<&[i32]>) -> Vec<DbTarget> {
    let mut out = Vec::new();
    for dir in dirs {
        for entry in WalkDir::new(dir).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.ends_with("_0001") {
                continue;
            }
            let Some(dbnum) = read_dbnum(path) else {
                continue;
            };
            if let Some(wl) = whitelist {
                if !wl.contains(&dbnum) {
                    continue;
                }
            }
            out.push(DbTarget { path: path.to_path_buf(), dbnum });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn read_dbnum(path: &Path) -> Option<i32> {
    let mut buf = [0u8; 64];
    let mut f = std::fs::File::open(path).ok()?;
    std::io::Read::read_exact(&mut f, &mut buf).ok()?;
    Some(i32::from_be_bytes([
        buf[HDR_DBNUM_OFF],
        buf[HDR_DBNUM_OFF + 1],
        buf[HDR_DBNUM_OFF + 2],
        buf[HDR_DBNUM_OFF + 3],
    ]))
}

/// G1-A2:单库同步步(纯水位驱动;错误折叠为 `Failed` 以便守护壳隔离)。
pub async fn sync_db(target: &DbTarget) -> DbSyncOutcome {
    match sync_db_inner(target).await {
        Ok(outcome) => outcome,
        Err(e) => DbSyncOutcome::Failed(format!("{e:#}")),
    }
}

async fn sync_db_inner(target: &DbTarget) -> anyhow::Result<DbSyncOutcome> {
    let mut io = PdmsIO::new("syncd", &target.path, false);
    io.open()?;
    io.init_ses_range_map()?;
    let latest = io.get_latest_sesno()?;
    let watermark = crate::surreal_ingest::read_watermark(io.dbnum).await?;

    match watermark {
        Some(w) if latest <= w => Ok(DbSyncOutcome::SkippedUpToDate { sesno: latest }),
        Some(w) => {
            let range = (w as i32 + 1)..=(latest as i32);
            let incr = io.collect_increment_eles(Some(range))?;
            let sessions = incr.len();
            io.update_elements_to_database(&incr, false).await?;
            Ok(DbSyncOutcome::Synced { from_sesno: w + 1, to_sesno: latest, sessions })
        }
        None => {
            // 初见库:仅最新会话立基线(G1-A2;历史回灌走显式 bootstrap)。
            let incr = io.collect_increment_eles(None)?;
            let ops = incr.values().map(|v| v.len()).sum();
            io.update_elements_to_database(&incr, false).await?;
            Ok(DbSyncOutcome::Baseline { sesno: latest, ops })
        }
    }
}

/// G1-A4:显式初灌(bootstrap)——委托既有唯一入口 `collect_and_save_latest_data`。
pub async fn bootstrap_db(target: &DbTarget, max_sessions: u32) -> anyhow::Result<()> {
    let mut io = PdmsIO::new("syncd", &target.path, false);
    io.open()?;
    io.init_ses_range_map()?;
    io.collect_and_save_latest_data(Some(max_sessions), None).await
}
