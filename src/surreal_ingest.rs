//! specs/003 —— E3D 增量 → SurrealDB 落库核心（T201 强类型构造 / T202 入口逻辑 / T203 水位）。
//!
//! 契约 `specs/003-e3d-surreal-ingest/contracts/db-contract.md`：
//! - D2 I1：记录 ID 确定性——`ses: {dbnum}_{sesno}`、`pe_ses_h: {dbnum}_{r0}_{r1}_{sesno}`、
//!   `pe: {dbnum}_{r0}_{r1}`（最新态）、`ingest_watermark: {dbnum}`（字符串复合键,实现期回填）。
//! - D2 I2：全部写入为 upsert（同 ID 覆盖同值,重放幂等;区别于存量 `INSERT IGNORE` 的跳过）。
//! - D2 I3：水位单调,`sesno <= 水位` 的会话跳过且计数可观测（[`IngestReport`]）。
//! - D2 I4：强类型构造（`SurrealValue` 派生）,禁止字符串拼接 SQL。
//! - D3 A2：逐记录写,任一失败 `?` 上抛不静默;A3 不隐式连接;A4 只消费 `EleOperationData`。
//!
//! 本模块不依赖 `PdmsIO`（纯函数面,kv-mem 测试可直接驱动）;门面入口在 `io.rs`
//! 组装会话元数据后委托此处。

use std::collections::BTreeMap;

use aios_core::{NamedAttrMap, RefU64, SUL_DB};
use anyhow::Context;
use chrono::Utc;
use surrealdb_types::SurrealValue;

use crate::io::{EleOperationData, EleOperationDetail};

pub const TBL_SES: &str = "ses";
pub const TBL_PE_SES_H: &str = "pe_ses_h";
pub const TBL_PE: &str = "pe";
pub const TBL_WATERMARK: &str = "ingest_watermark";

/// 会话行（rfc3339 时间;入口从 `SessionPageData` 组装,缺元数据时仅 sesno/dbnum）。
#[derive(SurrealValue, Clone, Debug, Default)]
pub struct SesRow {
    pub sesno: u32,
    pub dbnum: i32,
    pub date_time: String,
    pub end_pgno: u32,
}

/// refno × 会话 索引行。
/// 契约 D1 回填注：增量载荷 `EleOperationData` 不含物理 offset（属历史回填路径的字段）,
/// 本行以 `op`（add/modify/delete/none,沿 `get_op_type`）取代。
#[derive(SurrealValue, Clone, Debug)]
pub struct PeSesHRow {
    pub refno: RefU64,
    pub sesno: u32,
    pub dbnum: i32,
    pub op: String,
}

/// 元素主数据行（最新态;`deleted` 标记墓碑）。
#[derive(SurrealValue, Clone, Debug)]
pub struct PeRow {
    pub refno: RefU64,
    pub owner: RefU64,
    pub dbnum: i32,
    pub sesno: u32,
    pub noun: u32,
    pub name: String,
    pub attrs: NamedAttrMap,
    pub explicit_attrs: NamedAttrMap,
    pub children: Vec<RefU64>,
    pub deleted: bool,
}

/// 落库水位（D2 I3:只升不降——调用方按 sesno 升序处理 + 低于水位跳过共同保证）。
#[derive(SurrealValue, Clone, Debug, Default)]
pub struct WatermarkRow {
    pub dbnum: i32,
    pub high_sesno: u32,
    pub updated_at: String,
}

/// 落库回执（跳过计数 = SC-003 的可观测点;门面入口丢弃前记日志,测试直读）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IngestReport {
    pub sessions_written: usize,
    pub sessions_skipped: usize,
    pub ses_rows: usize,
    pub pe_ses_h_rows: usize,
    pub pe_rows: usize,
}

fn pe_row_from(dbnum: i32, sesno: u32, d: &parse_pdms_db::parse::EleData) -> PeRow {
    PeRow {
        refno: d.refno,
        owner: d.owner,
        dbnum,
        sesno,
        noun: d.noun,
        name: d.name.clone(),
        attrs: d.att_map().clone(),
        explicit_attrs: d.explicit_attmap().clone(),
        children: d.children.0.clone(),
        deleted: false,
    }
}

fn pe_tombstone(dbnum: i32, sesno: u32, refno: RefU64) -> PeRow {
    PeRow {
        refno,
        owner: RefU64::default(),
        dbnum,
        sesno,
        noun: 0,
        name: String::new(),
        attrs: NamedAttrMap::default(),
        explicit_attrs: NamedAttrMap::default(),
        children: Vec::new(),
        deleted: true,
    }
}

/// 读取库水位(specs/006 同步核心复用:pub(crate),G1-A2 的"初见库/最新判定"依据)。
pub(crate) async fn read_watermark(dbnum: i32) -> anyhow::Result<Option<u32>> {
    // SurrealDB 3.x 对不存在的表/记录 select 报 NotFound(而非 None)——按"无水位"处理。
    match SUL_DB.select::<Option<WatermarkRow>>((TBL_WATERMARK, dbnum.to_string())).await {
        Ok(row) => Ok(row.map(|w| w.high_sesno)),
        Err(e) if e.to_string().contains("does not exist") => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read ingest_watermark for dbnum {dbnum}")),
    }
}

async fn bump_watermark(dbnum: i32, sesno: u32) -> anyhow::Result<()> {
    let row =
        WatermarkRow { dbnum, high_sesno: sesno, updated_at: Utc::now().to_rfc3339() };
    let _: Option<WatermarkRow> = SUL_DB
        .upsert((TBL_WATERMARK, dbnum.to_string()))
        .content(row)
        .await
        .with_context(|| format!("bump ingest_watermark to {sesno}"))?;
    Ok(())
}

/// 增量落库核心（契约 D3 A1~A4）。
///
/// - `ses_meta`:sesno → 会话行元数据（缺项写 minimal 行）。
/// - 会话按升序处理;`sesno <= 水位` 跳过（I3）;每会话完成后升水位。
/// - 全部 upsert,任一失败上抛（A2;幂等重放可补齐）。
pub async fn ingest_increments(
    dbnum: i32,
    ses_meta: &BTreeMap<u32, SesRow>,
    range_eles: &BTreeMap<u32, Vec<EleOperationData>>,
    skip_main_data: bool,
) -> anyhow::Result<IngestReport> {
    let mut report = IngestReport::default();
    let watermark = read_watermark(dbnum).await?;

    for (&sesno, ops) in range_eles {
        if let Some(w) = watermark {
            if sesno <= w {
                report.sessions_skipped += 1;
                continue;
            }
        }

        let ses_row = ses_meta
            .get(&sesno)
            .cloned()
            .unwrap_or(SesRow { sesno, dbnum, ..Default::default() });
        let _: Option<SesRow> = SUL_DB
            .upsert((TBL_SES, format!("{dbnum}_{sesno}")))
            .content(ses_row)
            .await
            .with_context(|| format!("upsert ses {sesno}"))?;
        report.ses_rows += 1;

        for op in ops {
            let (r0, r1) = (op.refno.get_0(), op.refno.get_1());
            let row = PeSesHRow {
                refno: op.refno,
                sesno,
                dbnum,
                op: op.get_op_type().to_string(),
            };
            let _: Option<PeSesHRow> = SUL_DB
                .upsert((TBL_PE_SES_H, format!("{dbnum}_{r0}_{r1}_{sesno}")))
                .content(row)
                .await
                .with_context(|| format!("upsert pe_ses_h ({r0:#x},{r1:#x})@{sesno}"))?;
            report.pe_ses_h_rows += 1;
        }

        if !skip_main_data {
            for op in ops {
                let row = match &op.detail {
                    EleOperationDetail::Add(d) => Some(pe_row_from(dbnum, sesno, d)),
                    EleOperationDetail::Modified(m) => {
                        Some(pe_row_from(dbnum, sesno, &m.current_data))
                    }
                    EleOperationDetail::Deleted => {
                        Some(pe_tombstone(dbnum, sesno, op.refno))
                    }
                    EleOperationDetail::None => None,
                };
                if let Some(row) = row {
                    let (r0, r1) = (row.refno.get_0(), row.refno.get_1());
                    let _: Option<PeRow> = SUL_DB
                        .upsert((TBL_PE, format!("{dbnum}_{r0}_{r1}")))
                        .content(row)
                        .await
                        .with_context(|| format!("upsert pe ({r0:#x},{r1:#x})"))?;
                    report.pe_rows += 1;
                }
            }
        }

        bump_watermark(dbnum, sesno).await?;
        report.sessions_written += 1;
    }

    Ok(report)
}
