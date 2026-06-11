use crate::defines::PAGE_SIZE_2K;
use crate::io::PdmsIO;
use crate::test::resolve_test_db_path;

#[test]
fn test_open_smoke() -> anyhow::Result<()> {
    // 本地若无测试库，则跳过（保证在 CI / 新环境可稳定运行）。
    let db_filepath = match resolve_test_db_path("ams1112_0001") {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return Ok(());
        }
    };

    let mut io = PdmsIO::new("ams", &db_filepath, true);
    io.open()?;
    assert_eq!(io.page_size, PAGE_SIZE_2K, "page_size 探测应命中 2K 页面");
    Ok(())
}

/// specs/002 T205 的 v1↔e3d_io 逐字节 parity 闸已于 Phase 3（T303,2026-06-11）随
/// v1 `ElementRecordReader` 退役删除——迁移期使命完成（ams1112 抽样 430 条全等）;
/// 记录读取的持续等值保障由 `diag_ams1112_full_parse`/`desp`/`bend_angl` 集成测试承接。
/// 此处保留记录定界的自洽 smoke：抽样记录可读、非空、以合法 impl_len 开头。
#[test]
fn test_element_record_smoke() -> anyhow::Result<()> {
    let db_filepath = match resolve_test_db_path("ams1112_0001") {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return Ok(());
        }
    };

    let mut io = PdmsIO::new("ams", &db_filepath, false);
    io.open()?;
    let map = io.build_index_map()?;
    let mut offsets: Vec<u64> = map.values().filter_map(|v| v.last().copied()).collect();
    offsets.sort_unstable();

    let mut checked = 0usize;
    for off in offsets.into_iter().step_by(997) {
        let rec = io.read_element_record_cached(off)?;
        assert!(!rec.is_empty(), "empty record at {off:#X}");
        // 跳过 0/7 前导填充后应以合法 impl_len(低 16 位 8..=512,高 16 位 0)开头。
        let mut p = 0usize;
        while p + 4 <= rec.len() {
            let w = &rec[p..p + 4];
            if w == [0, 0, 0, 0] || w == [0, 0, 0, 7] {
                p += 4;
            } else {
                break;
            }
        }
        if p + 4 <= rec.len() {
            let w0 = u32::from_be_bytes([rec[p], rec[p + 1], rec[p + 2], rec[p + 3]]);
            assert_eq!(w0 >> 16, 0, "impl_len 高位非零 at {off:#X}");
            assert!((8..=512).contains(&(w0 & 0xFFFF)), "impl_len 越界 at {off:#X}");
        }
        checked += 1;
    }
    assert!(checked >= 100, "expected >=100 sampled records, got {checked}");
    Ok(())
}

/// specs/002 T203：会话链委托 e3d_io 后，sesno 映射仍正确建立（增量归属的根基）。
#[test]
fn test_ses_maps_smoke() -> anyhow::Result<()> {
    let db_filepath = match resolve_test_db_path("ams1112_0001") {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return Ok(());
        }
    };

    let mut io = PdmsIO::new("ams", &db_filepath, false);
    io.open()?;
    io.init_ses_range_map()?;
    let latest = io.get_latest_sesno()?;
    assert!(latest > 0, "latest sesno must be positive");
    assert!(
        io.get_ses_pageno(latest as i32).is_some(),
        "sesno_pgno_map must contain the latest session"
    );
    Ok(())
}
