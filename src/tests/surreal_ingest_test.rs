//! specs/003 T204——增量落库:幂等重放(SC-002)+ 水位跳过(SC-003)+ 内容对应(SC-001 合成版)。
//! 真实样本端到端(SC-004)见 T205。

use std::collections::BTreeMap;

use aios_core::{RefU64, SUL_DB};

use crate::io::{EleOperationData, EleOperationDetail, ModifiedElement};
use crate::surreal_ingest::{
    IngestReport, PeRow, TBL_PE, TBL_PE_SES_H, TBL_SES, TBL_WATERMARK, ingest_increments,
};
use crate::tests::surreal_mem::{isolated, rt, table_count};

fn ele(refno: RefU64, name: &str, noun: u32) -> parse_pdms_db::parse::EleData {
    parse_pdms_db::parse::EleData {
        refno,
        name: name.to_string(),
        noun,
        ..Default::default()
    }
}

/// 两会话四操作的合成增量(Add/None + Modified/Deleted)。
fn synth_increments() -> BTreeMap<u32, Vec<EleOperationData>> {
    let r_a = RefU64::from_two_nums(0x5C20, 0x1001);
    let r_b = RefU64::from_two_nums(0x5C20, 0x1002);
    let r_c = RefU64::from_two_nums(0x5C20, 0x1003);
    let r_d = RefU64::from_two_nums(0x5C20, 0x1004);

    let mut m = BTreeMap::new();
    m.insert(
        10,
        vec![
            EleOperationData::new(r_a, 10, EleOperationDetail::Add(ele(r_a, "/SYN-A", 0xCC47))),
            EleOperationData::new(r_b, 10, EleOperationDetail::None),
        ],
    );
    m.insert(
        11,
        vec![
            EleOperationData::new(
                r_c,
                11,
                EleOperationDetail::Modified(ModifiedElement {
                    current_data: ele(r_c, "/SYN-C", 0xCC47),
                    added_attrs: Default::default(),
                    deleted_attrs: Default::default(),
                    modified_attrs: Default::default(),
                    added_explicit_attrs: Default::default(),
                    deleted_explicit_attrs: Default::default(),
                    modified_explicit_attrs: Default::default(),
                    added_uda_attrs: Default::default(),
                    deleted_uda_attrs: Default::default(),
                    modified_uda_attrs: Default::default(),
                    noun: "SYNN".to_string(),
                    children_changed: None,
                }),
            ),
            EleOperationData::new(r_d, 11, EleOperationDetail::Deleted),
        ],
    );
    m
}

async fn counts() -> (i64, i64, i64) {
    (table_count(TBL_SES).await, table_count(TBL_PE_SES_H).await, table_count(TBL_PE).await)
}

#[test]
fn ingest_idempotent_replay_and_watermark() {
    rt().block_on(async {
        let _g = isolated("ingest_replay").await;
        let dbnum = 7200;
        let incs = synth_increments();
        let ses_meta = BTreeMap::new();

        // 第一次:全部写入。pe = Add + Modified + Deleted(墓碑) = 3;None 不写 pe。
        let r1 = ingest_increments(dbnum, &ses_meta, &incs, false).await.unwrap();
        assert_eq!(
            r1,
            IngestReport {
                sessions_written: 2,
                sessions_skipped: 0,
                ses_rows: 2,
                pe_ses_h_rows: 4,
                pe_rows: 3
            }
        );
        let c1 = counts().await;
        assert_eq!(c1, (2, 4, 3));

        // SC-001(合成):内容对应——主数据/墓碑可读回。
        let pe_a: Option<PeRow> =
            SUL_DB.select((TBL_PE, format!("{dbnum}_{}_{}", 0x5C20, 0x1001))).await.unwrap();
        let pe_a = pe_a.expect("pe row for /SYN-A");
        assert_eq!(pe_a.name, "/SYN-A");
        assert_eq!(pe_a.noun, 0xCC47);
        assert!(!pe_a.deleted);
        let pe_d: Option<PeRow> =
            SUL_DB.select((TBL_PE, format!("{dbnum}_{}_{}", 0x5C20, 0x1004))).await.unwrap();
        assert!(pe_d.expect("tombstone for /SYN-D").deleted);

        // SC-003:重放 ⇒ 水位全跳过,库不变。
        let r2 = ingest_increments(dbnum, &ses_meta, &incs, false).await.unwrap();
        assert_eq!(r2.sessions_written, 0);
        assert_eq!(r2.sessions_skipped, 2);
        assert_eq!(counts().await, c1, "watermark replay must not change tables");

        // SC-002:清水位后重放 ⇒ 纯 upsert 幂等,计数仍不变。
        SUL_DB.query(format!("DELETE {TBL_WATERMARK};")).await.unwrap();
        let r3 = ingest_increments(dbnum, &ses_meta, &incs, false).await.unwrap();
        assert_eq!(r3.sessions_written, 2);
        assert_eq!(counts().await, c1, "upsert replay must not duplicate");
    });
}

#[test]
fn ingest_skip_main_data_writes_no_pe() {
    rt().block_on(async {
        let _g = isolated("ingest_skip_main").await;
        let r = ingest_increments(7200, &BTreeMap::new(), &synth_increments(), true)
            .await
            .unwrap();
        assert_eq!(r.pe_rows, 0);
        assert_eq!(counts().await, (2, 4, 0), "skip_main_data must not write pe");
    });
}

/// T205/SC-001:真实样本端到端——sam7200 最新会话增量经**门面入口**入库,
/// 逐表计数与入参对应;门面重放被水位拦截。
#[test]
fn ingest_real_sample_sam7200_end_to_end() {
    const SAM: &str = r"D:\work\plant\pdms-io\pdms-test-data\sam7200_0001";
    if !std::path::Path::new(SAM).exists() {
        println!("数据库文件不存在，跳过测试: sam7200_0001");
        return;
    }
    rt().block_on(async {
        let _g = isolated("ingest_real_sam").await;

        let mut io = crate::io::PdmsIO::new("sam", SAM, false);
        io.open().expect("open sam7200");
        let incs = io.collect_increment_eles(None).expect("collect latest increments");
        assert!(!incs.is_empty(), "latest session must yield increments");
        let total_ops: usize = incs.values().map(|v| v.len()).sum();
        let non_none = incs
            .values()
            .flatten()
            .filter(|op| !matches!(op.detail, EleOperationDetail::None))
            .count();

        io.update_elements_to_database(&incs, false).await.expect("ingest via facade");

        assert_eq!(table_count(TBL_SES).await as usize, incs.len(), "ses rows == sessions");
        assert_eq!(table_count(TBL_PE_SES_H).await as usize, total_ops, "pe_ses_h == ops");
        assert_eq!(table_count(TBL_PE).await as usize, non_none, "pe == non-None ops");

        // 门面重放:水位拦截,库不变。
        let before = counts().await;
        io.update_elements_to_database(&incs, false).await.expect("replay via facade");
        assert_eq!(counts().await, before, "facade replay must be watermark-skipped");
    });
}

/// T205/SC-004:ams1112(103MB/42 万元素级)最新会话增量入库,计数+耗时报告。
#[test]
fn ingest_real_sample_ams1112_timing() {
    let Some(path) = crate::test::resolve_test_db_path("ams1112_0001") else {
        println!("数据库文件不存在，跳过测试: ams1112_0001");
        return;
    };
    rt().block_on(async {
        let _g = isolated("ingest_real_ams").await;

        let mut io = crate::io::PdmsIO::new("ams", &path, false);
        io.open().expect("open ams1112");
        let t0 = std::time::Instant::now();
        let incs = io.collect_increment_eles(None).expect("collect latest increments");
        let collect_elapsed = t0.elapsed();
        let total_ops: usize = incs.values().map(|v| v.len()).sum();

        let t1 = std::time::Instant::now();
        io.update_elements_to_database(&incs, false).await.expect("ingest via facade");
        let ingest_elapsed = t1.elapsed();

        println!(
            "[SC-004] ams1112 latest-session ingest: sessions={} ops={} pe_ses_h={} pe={} collect={collect_elapsed:?} ingest={ingest_elapsed:?}",
            incs.len(),
            total_ops,
            table_count(TBL_PE_SES_H).await,
            table_count(TBL_PE).await,
        );
        assert_eq!(table_count(TBL_PE_SES_H).await as usize, total_ops);
    });
}
