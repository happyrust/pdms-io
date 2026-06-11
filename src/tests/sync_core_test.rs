//! specs/006 T103——同步核心:扫描/基线/捕获/幂等/重启/坏库隔离(kv-mem + 临时目录)。

use std::path::PathBuf;

use crate::sync_core::{DbSyncOutcome, scan_targets, sync_db};
use crate::tests::surreal_mem::{isolated, rt};
use crate::writeback_core::{EditOp, WriteMode, apply_writeback_file};

const EXE: &str = r"D:\AVEVA\Everything3D2.10";
const SAM: &str = r"D:\work\plant\pdms-io\pdms-test-data\sam7200_0001";

fn data_present() -> bool {
    std::path::Path::new(&format!(r"{}\desvir.dat", EXE)).exists()
        && std::path::Path::new(SAM).exists()
}

/// 建临时"工程目录":sam7200 副本 + 干扰文件(非库名/坏头库名)。
fn temp_project(tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("syncd_{}_{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("tst7200_0001");
    std::fs::copy(SAM, &db).unwrap();
    std::fs::write(dir.join("notes.txt"), b"not a db").unwrap();
    std::fs::write(dir.join("bad_0001"), b"short").unwrap(); // <64B,读头失败 ⇒ scan 跳过
    (dir, db)
}

/// T101:扫描识别 + 干扰过滤 + dbno 白名单。
#[test]
fn sync_scan_targets_filters() {
    if !data_present() {
        eprintln!("[skip] data absent");
        return;
    }
    let (dir, db) = temp_project("scan");
    let targets = scan_targets(&[dir.clone()], None);
    assert_eq!(targets.len(), 1, "only the real db is recognized");
    assert_eq!(targets[0].path, db);
    let dbnum = targets[0].dbnum;
    assert!(dbnum > 0);

    assert!(scan_targets(&[dir.clone()], Some(&[dbnum])).len() == 1, "whitelist hit");
    assert!(scan_targets(&[dir.clone()], Some(&[dbnum + 1])).is_empty(), "whitelist miss");

    let _ = std::fs::remove_dir_all(&dir);
}

/// T103 全链(SC-001/002):基线 → 幂等跳过 → 新会话捕获 → 水位前进 → 重启幂等。
#[test]
fn sync_db_baseline_capture_idempotent_restart() {
    if !data_present() {
        eprintln!("[skip] data absent");
        return;
    }
    rt().block_on(async {
        let _g = isolated("sync_core_e2e").await;
        let (dir, db) = temp_project("e2e");
        let ss = e3d_io::SchemaSet::load(EXE);
        let target = scan_targets(&[dir.clone()], None).remove(0);

        // ① 初见库:基线(最新会话,不回灌历史)。
        let o1 = sync_db(&target).await;
        let base_sesno = match o1 {
            DbSyncOutcome::Baseline { sesno, .. } => sesno,
            other => panic!("expected Baseline, got {other:?}"),
        };

        // ② 同状态重跑 = 幂等跳过(零写库)。
        assert_eq!(
            sync_db(&target).await,
            DbSyncOutcome::SkippedUpToDate { sesno: base_sesno },
            "watermark must short-circuit"
        );

        // ③ 设计员保存:e3d_io 写一笔新会话生成新版本文件,替换原文件。
        let edits = {
            let bytes = std::fs::read(&db).unwrap();
            let dbv = e3d_io::Edb::from_bytes(bytes);
            let mut rm = std::collections::HashMap::new();
            let unnamed = e3d_io::index_db(&dbv, &ss, true, &mut rm)
                .iter()
                .find(|e| e.name.is_none() && e.pos().is_some())
                .unwrap()
                .refno;
            vec![EditOp::SetPos { refno: unnamed, pos: [42.0, 43.0, 44.5] }]
        };
        let (out_path, rep) = apply_writeback_file(&db, &ss, &edits, WriteMode::Copy).unwrap();
        std::fs::rename(&out_path, &db).unwrap(); // 新版本顶替(模拟保存完成)
        assert_eq!(rep.new_sesno, base_sesno + 1);

        // ④ 守护轮捕获新会话,水位前进。
        match sync_db(&target).await {
            DbSyncOutcome::Synced { from_sesno, to_sesno, sessions } => {
                assert_eq!((from_sesno, to_sesno), (base_sesno + 1, base_sesno + 1));
                assert_eq!(sessions, 1);
            }
            other => panic!("expected Synced, got {other:?}"),
        }

        // ⑤ "重启"(无本地状态,新调用即重启语义):幂等跳过。
        assert_eq!(
            sync_db(&target).await,
            DbSyncOutcome::SkippedUpToDate { sesno: base_sesno + 1 }
        );

        let _ = std::fs::remove_dir_all(&dir);
    });
}

/// SC-003 坏库隔离:垃圾头库文件 Failed,健康库同轮照常。
#[test]
fn sync_db_bad_file_isolated() {
    if !data_present() {
        eprintln!("[skip] data absent");
        return;
    }
    rt().block_on(async {
        let _g = isolated("sync_core_isolation").await;
        let (dir, _db) = temp_project("isolation");
        // 64B 垃圾头:能被 scan 读头(dbnum 垃圾值),sync_db open 时失败 ⇒ Failed 隔离。
        std::fs::write(dir.join("junk_0001"), vec![0xAAu8; 4096]).unwrap();

        let targets = scan_targets(&[dir.clone()], None);
        assert_eq!(targets.len(), 2, "junk db with readable header is scanned");

        let mut ok = 0;
        let mut failed = 0;
        for t in &targets {
            match sync_db(t).await {
                DbSyncOutcome::Failed(e) => {
                    failed += 1;
                    assert!(!e.is_empty());
                }
                DbSyncOutcome::Baseline { .. } => ok += 1,
                other => panic!("unexpected outcome {other:?}"),
            }
        }
        assert_eq!((ok, failed), (1, 1), "healthy db syncs while junk db fails in isolation");

        let _ = std::fs::remove_dir_all(&dir);
    });
}
