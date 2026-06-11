//! specs/004 T203——队列层幂等与状态机(kv-mem + sam7200,缺资源优雅跳过)。

use std::collections::HashMap;

use e3d_io::{Edb, SchemaSet, index_db};

use crate::surreal_writeback::{
    STATUS_APPLIED, STATUS_FAILED, TBL_WRITEBACK_QUEUE, WritebackQueueRow, apply_queue,
    enqueue_writeback,
};
use crate::surreal_ingest::{PeRow, TBL_PE, TBL_PE_SES_H, TBL_SES};
use crate::tests::surreal_mem::{isolated, rt, table_count};
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

/// SC-005 回声收敛(T301;Q4=接受回声的实证):写回副本 → watcher 视角增量提取 →
/// 经门面入口 ingest → `pe` 收敛于写回意图;再 ingest 被水位拦截,库不变。
#[test]
fn writeback_echo_converges_into_pe() {
    if !data_present() {
        eprintln!("[skip] data absent");
        return;
    }
    rt().block_on(async {
        let _g = isolated("wbq_echo").await;
        let ss = SchemaSet::load(EXE);
        let src = temp_db_copy("echo");
        let out = {
            let mut p = src.as_os_str().to_owned();
            p.push(".e3dout");
            std::path::PathBuf::from(p)
        };

        // 写回意图:无名元素改 POS(内联编辑;refno 寻址)。
        //
        // 注:本测试限定**内联属性**编辑面——DA 文本编辑(如 Rename)在文件级
        // 读回正确(001 链式解码),但 v1 增量读取的 EleData 为窗口邻接解析,
        // 看不见 e3d_io 重定位到远页的 DA ⇒ 改名会被定性"无变化"漏出增量。
        // 已作为已知局限入档(research R5 / 005 候选:EleData DA 解析改走链式解码)。
        let bytes = std::fs::read(&src).unwrap();
        let db0 = Edb::from_bytes(bytes);
        let mut rm = HashMap::new();
        let elems = index_db(&db0, &ss, true, &mut rm);
        let unnamed = elems
            .iter()
            .find(|e| e.name.is_none() && e.pos().is_some())
            .unwrap()
            .refno;
        let batch = EditBatch::new(vec![EditOp::SetPos {
            refno: unnamed,
            pos: [111.0, 222.0, 333.5],
        }]);
        let dbnum_q = unnamed.0 as i32;
        enqueue_writeback(dbnum_q, "echo1", &src.to_string_lossy(), &batch).await.unwrap();
        let rep = apply_queue(dbnum_q, &src, &ss, WriteMode::Copy).await.unwrap();
        assert_eq!(rep.applied, 1);
        let new_sesno = rep.new_sesnos[0];

        // 回声:watcher 视角读写回产物,提取最新会话增量,经门面唯一入口入库。
        let mut io = crate::io::PdmsIO::new("sam", &out, false);
        io.open().unwrap();
        let incr = io.collect_increment_eles(None).unwrap();
        assert!(incr.contains_key(&new_sesno), "echo increment carries the writeback session");
        assert!(
            incr[&new_sesno].iter().any(|op| {
                op.refno == aios_core::RefU64::from_two_nums(unnamed.0, unnamed.1)
            }),
            "edited element present in echo increment"
        );
        io.update_elements_to_database(&incr, false).await.unwrap();
        let dbnum = io.dbnum;

        // pe 收敛于写回意图:被编辑的无名元素以新会话号入库、非墓碑,
        // 且属性载荷携带写回的 POS 数值。
        let pe: Option<PeRow> = SUL_DB
            .select((TBL_PE, format!("{dbnum}_{}_{}", unnamed.0, unnamed.1)))
            .await
            .unwrap();
        let pe = pe.expect("edited element echoed into pe");
        assert_eq!(pe.sesno, new_sesno, "pe row carries the writeback session");
        assert!(!pe.deleted);
        let attrs_dbg = format!("{:?}", pe.attrs);
        assert!(
            attrs_dbg.contains("222") && attrs_dbg.contains("333.5"),
            "pe attrs converge on writeback POS intent: {attrs_dbg}"
        );

        // 再 ingest = 水位拦截,逐表计数不变(幂等回声,Q4 语义成立)。
        let counts = (
            table_count(TBL_SES).await,
            table_count(TBL_PE_SES_H).await,
            table_count(TBL_PE).await,
        );
        io.update_elements_to_database(&incr, false).await.unwrap();
        assert_eq!(
            counts,
            (
                table_count(TBL_SES).await,
                table_count(TBL_PE_SES_H).await,
                table_count(TBL_PE).await,
            ),
            "replayed echo must be watermark-skipped"
        );

        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&out);
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
