//! specs/004 T203——队列层幂等与状态机(kv-mem + sam7200,缺资源优雅跳过)。

use std::collections::HashMap;

use e3d_io::{Edb, SchemaSet, index_db};

use crate::surreal_writeback::{
    STATUS_APPLIED, STATUS_FAILED, TBL_WRITEBACK_QUEUE, WritebackQueueRow, apply_queue,
    enqueue_writeback,
};
use crate::tests::surreal_mem::{isolated, rt};
use crate::writeback_core::{EditBatch, EditOp, WriteMode};
use aios_core::SUL_DB;

const EXE: &str = r"D:\AVEVA\Everything3D2.10";
const DBF: &str = r"D:\work\plant\pdms-io\pdms-test-data\sam7200_0001";

fn data_present() -> bool {
    std::path::Path::new(&format!(r"{}\desvir.dat", EXE)).exists()
        && std::path::Path::new(DBF).exists()
}

fn temp_db_copy(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("wbq_{}_{}", std::process::id(), tag));
    std::fs::copy(DBF, &p).unwrap();
    p
}

async fn queue_row(dbnum: i32, batch_id: &str) -> WritebackQueueRow {
    let row: Option<WritebackQueueRow> = SUL_DB
        .select((TBL_WRITEBACK_QUEUE, format!("{dbnum}_{batch_id}")))
        .await
        .expect("select queue row");
    row.expect("queue row present")
}

/// SC-004 + E2 状态机:入队 → apply(applied+回执) → 重复 apply 幂等跳过、文件不再生长。
#[test]
fn writeback_queue_apply_then_idempotent_skip() {
    if !data_present() {
        eprintln!("[skip] data absent");
        return;
    }
    rt().block_on(async {
        let _g = isolated("wbq_apply").await;
        let ss = SchemaSet::load(EXE);
        let src = temp_db_copy("apply");
        let out = {
            let mut p = src.as_os_str().to_owned();
            p.push(".e3dout");
            std::path::PathBuf::from(p)
        };

        // 找无名带 POS 元素(写回管道典型目标)。
        let bytes = std::fs::read(&src).unwrap();
        let db0 = Edb::from_bytes(bytes.clone());
        let mut rm = HashMap::new();
        let unnamed = index_db(&db0, &ss, true, &mut rm)
            .iter()
            .find(|e| e.name.is_none() && e.pos().is_some())
            .unwrap()
            .refno;
        let dbnum = unnamed.0 as i32;

        let batch =
            EditBatch::new(vec![EditOp::SetPos { refno: unnamed, pos: [9.0, 8.0, 7.5] }]);
        enqueue_writeback(dbnum, "b001", &src.to_string_lossy(), &batch).await.unwrap();
        // 同批重复入队 = 同 id 覆盖,不重复。
        enqueue_writeback(dbnum, "b001", &src.to_string_lossy(), &batch).await.unwrap();

        let rep = apply_queue(dbnum, &src, &ss, WriteMode::Copy).await.unwrap();
        assert_eq!((rep.applied, rep.skipped_applied), (1, 0));
        assert_eq!(rep.new_sesnos.len(), 1);
        assert!(out.exists(), "copy-mode output produced");
        assert_eq!(std::fs::read(&src).unwrap(), bytes, "source untouched");

        let row = queue_row(dbnum, "b001").await;
        assert_eq!(row.status, STATUS_APPLIED);
        assert_eq!(row.applied_sesno, rep.new_sesnos[0]);
        assert!(row.diff_modified >= 1 && row.error.is_empty() && !row.applied_at.is_empty());

        // 幂等:重复 apply 全跳过,不再产出文件。
        std::fs::remove_file(&out).unwrap();
        let rep2 = apply_queue(dbnum, &src, &ss, WriteMode::Copy).await.unwrap();
        assert_eq!((rep2.applied, rep2.skipped_applied), (0, 1), "applied is terminal");
        assert!(!out.exists(), "no rewrite on idempotent skip");

        let _ = std::fs::remove_file(&src);
    });
}

/// E2 失败语义:坏批 → Err + 行记 failed/error;重 apply 不自动重试(skipped_failed)。
#[test]
fn writeback_queue_failure_recorded_not_retried() {
    if !data_present() {
        eprintln!("[skip] data absent");
        return;
    }
    rt().block_on(async {
        let _g = isolated("wbq_fail").await;
        let ss = SchemaSet::load(EXE);
        let src = temp_db_copy("fail");
        let dbnum = 0x5C20_i32;

        let bad = EditBatch::new(vec![EditOp::Delete {
            refno: (0x5C20, 0xFFFF_FFF0),
            force: true,
        }]);
        enqueue_writeback(dbnum, "bad01", &src.to_string_lossy(), &bad).await.unwrap();

        let err = apply_queue(dbnum, &src, &ss, WriteMode::Copy).await.unwrap_err();
        assert!(format!("{err:#}").contains("bad01"));
        let row = queue_row(dbnum, "bad01").await;
        assert_eq!(row.status, STATUS_FAILED);
        assert!(!row.error.is_empty());

        let rep = apply_queue(dbnum, &src, &ss, WriteMode::Copy).await.unwrap();
        assert_eq!(
            (rep.applied, rep.skipped_failed),
            (0, 1),
            "failed batch must not auto-retry"
        );

        let _ = std::fs::remove_file(&src);
    });
}

/// E1-I2:schema_version 失配 → failed + 错误可读。
#[test]
fn writeback_queue_schema_version_mismatch() {
    if !data_present() {
        eprintln!("[skip] data absent");
        return;
    }
    rt().block_on(async {
        let _g = isolated("wbq_schema").await;
        let ss = SchemaSet::load(EXE);
        let src = temp_db_copy("schema");
        let dbnum = 0x5C20_i32;

        let mut batch = EditBatch::new(vec![EditOp::SetPos {
            refno: (0x5C20, 1),
            pos: [0.0, 0.0, 0.0],
        }]);
        batch.schema_version = 999;
        enqueue_writeback(dbnum, "v999", &src.to_string_lossy(), &batch).await.unwrap();

        let err = apply_queue(dbnum, &src, &ss, WriteMode::Copy).await.unwrap_err();
        assert!(format!("{err:#}").contains("schema_version"));
        assert_eq!(queue_row(dbnum, "v999").await.status, STATUS_FAILED);

        let _ = std::fs::remove_file(&src);
    });
}
