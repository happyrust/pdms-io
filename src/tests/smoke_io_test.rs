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
