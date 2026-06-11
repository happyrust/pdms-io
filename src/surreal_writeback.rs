//! specs/004 Phase 2 —— `writeback_queue` 队列层(T201 行契约 / T202 出队执行 / T203 幂等)。
//!
//! 契约 `specs/004-surreal-e3d-writeback/contracts/writeback-contract.md` E2:
//! - 确定性批次键 `{dbnum}_{batch_id}`:同批重复入队 = 同 id upsert(覆盖同值)。
//! - 状态机 `pending → applied | failed`;applied 终态——重复 apply **跳过且可观测**,
//!   文件 sesno 不增长(E2-I2/SC-004);failed **不自动重试**(回 pending 由调用方决定)。
//! - 行构造强类型(`SurrealValue`,零 SQL 拼接);`edits` 载荷 = `EditBatch` 的 serde_json
//!   字符串(与内存类型同源单一定义,含 `schema_version`,E1-I2)。
//! - 队列层零格式知识(E2-I3):字节语义全部在 [`crate::writeback_core`] 之下的 e3d_io。
//!
//! 执行顺序:同 dbnum 的 pending 批次按 `(created_at, batch_id)` 升序;**首个失败即停**
//! (该批记 failed + error 后整体上抛,后续批保持 pending——批间可能有顺序依赖,
//! 不越过失败点盲跑;幂等性保证修复后重跑可续)。

use std::path::Path;

use aios_core::{RefU64, SUL_DB};
use anyhow::{Context, anyhow, bail};
use chrono::Utc;
use e3d_io::SchemaSet;
use surrealdb_types::SurrealValue;

use crate::writeback_core::{
    EDITOP_SCHEMA_VERSION, EditBatch, WriteMode, WritebackReport, apply_writeback_file,
};

pub const TBL_WRITEBACK_QUEUE: &str = "writeback_queue";

pub const STATUS_PENDING: &str = "pending";
pub const STATUS_APPLIED: &str = "applied";
pub const STATUS_FAILED: &str = "failed";

/// 队列行(契约 E2;空串/0 = 未设置,避免对 SurrealValue 派生的 Option 字段面冒险)。
#[derive(SurrealValue, Clone, Debug)]
pub struct WritebackQueueRow {
    pub dbnum: i32,
    pub batch_id: String,
    pub db_path_hint: String,
    /// `EditBatch` 的 serde_json(含 schema_version;E1-I2 单一定义)。
    pub edits_json: String,
    pub status: String,
    pub applied_sesno: u32,
    pub new_refnos: Vec<RefU64>,
    pub diff_added: u32,
    pub diff_removed: u32,
    pub diff_modified: u32,
    pub error: String,
    pub created_at: String,
    pub applied_at: String,
}

fn queue_id(dbnum: i32, batch_id: &str) -> String {
    format!("{dbnum}_{batch_id}")
}

/// 入队(确定性 id upsert:同批重复入队覆盖同值,不重复)。
pub async fn enqueue_writeback(
    dbnum: i32,
    batch_id: &str,
    db_path_hint: &str,
    batch: &EditBatch,
) -> anyhow::Result<()> {
    let row = WritebackQueueRow {
        dbnum,
        batch_id: batch_id.to_string(),
        db_path_hint: db_path_hint.to_string(),
        edits_json: serde_json::to_string(batch).context("serialize EditBatch")?,
        status: STATUS_PENDING.to_string(),
        applied_sesno: 0,
        new_refnos: Vec::new(),
        diff_added: 0,
        diff_removed: 0,
        diff_modified: 0,
        error: String::new(),
        created_at: Utc::now().to_rfc3339(),
        applied_at: String::new(),
    };
    let _: Option<WritebackQueueRow> = SUL_DB
        .upsert((TBL_WRITEBACK_QUEUE, queue_id(dbnum, batch_id)))
        .content(row)
        .await
        .with_context(|| format!("enqueue writeback batch {batch_id}"))?;
    Ok(())
}

/// 队列执行回执(SC-004 的可观测点)。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct QueueApplyReport {
    pub applied: usize,
    pub skipped_applied: usize,
    pub skipped_failed: usize,
    pub new_sesnos: Vec<u32>,
}

async fn put_row(dbnum: i32, batch_id: &str, row: WritebackQueueRow) -> anyhow::Result<()> {
    let _: Option<WritebackQueueRow> = SUL_DB
        .upsert((TBL_WRITEBACK_QUEUE, queue_id(dbnum, batch_id)))
        .content(row)
        .await
        .with_context(|| format!("update queue row {batch_id}"))?;
    Ok(())
}

/// 出队执行(契约 E2/T202):取 `dbnum` 的 pending 批次按 `(created_at, batch_id)` 升序,
/// 逐批经写回核心落盘;成功回执回写,失败记 failed 后上抛(后续批保持 pending)。
pub async fn apply_queue(
    dbnum: i32,
    db_path: impl AsRef<Path>,
    ss: &SchemaSet,
    mode: WriteMode,
) -> anyhow::Result<QueueApplyReport> {
    let db_path = db_path.as_ref();
    let all: Vec<WritebackQueueRow> = match SUL_DB.select(TBL_WRITEBACK_QUEUE).await {
        Ok(rows) => rows,
        Err(e) if e.to_string().contains("does not exist") => Vec::new(),
        Err(e) => return Err(e).context("select writeback_queue"),
    };

    let mut mine: Vec<WritebackQueueRow> =
        all.into_iter().filter(|r| r.dbnum == dbnum).collect();
    mine.sort_by(|a, b| {
        (a.created_at.as_str(), a.batch_id.as_str())
            .cmp(&(b.created_at.as_str(), b.batch_id.as_str()))
    });

    let mut report = QueueApplyReport::default();
    for mut row in mine {
        match row.status.as_str() {
            STATUS_APPLIED => {
                report.skipped_applied += 1; // E2-I2:终态幂等跳过,文件不动
                continue;
            }
            STATUS_FAILED => {
                report.skipped_failed += 1; // 不自动重试
                continue;
            }
            _ => {}
        }

        let attempt: anyhow::Result<(std::path::PathBuf, WritebackReport)> = async {
            let batch: EditBatch =
                serde_json::from_str(&row.edits_json).context("parse edits_json")?;
            if batch.schema_version != EDITOP_SCHEMA_VERSION {
                bail!(
                    "edits schema_version {} != supported {}",
                    batch.schema_version,
                    EDITOP_SCHEMA_VERSION
                );
            }
            apply_writeback_file(db_path, ss, &batch.edits, mode)
        }
        .await;

        match attempt {
            Ok((_out, wb)) => {
                row.status = STATUS_APPLIED.to_string();
                row.applied_sesno = wb.new_sesno;
                row.new_refnos = wb
                    .results
                    .iter()
                    .filter_map(|r| r.new_refno)
                    .map(|(r0, r1)| RefU64::from_two_nums(r0, r1))
                    .collect();
                row.diff_added = wb.diff.added as u32;
                row.diff_removed = wb.diff.removed as u32;
                row.diff_modified = wb.diff.modified as u32;
                row.error = String::new();
                row.applied_at = Utc::now().to_rfc3339();
                let batch_id = row.batch_id.clone();
                put_row(dbnum, &batch_id, row).await?;
                report.applied += 1;
                report.new_sesnos.push(wb.new_sesno);
            }
            Err(e) => {
                let msg = format!("{e:#}");
                row.status = STATUS_FAILED.to_string();
                row.error = msg.clone();
                row.applied_at = Utc::now().to_rfc3339();
                let batch_id = row.batch_id.clone();
                put_row(dbnum, &batch_id, row).await?;
                return Err(anyhow!(
                    "writeback batch {batch_id} failed (recorded; later batches stay pending): {msg}"
                ));
            }
        }
    }
    Ok(report)
}
