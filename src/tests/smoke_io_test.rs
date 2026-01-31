use crate::io::PdmsIO;
use crate::test::resolve_test_db_path;
use crate::defines::PAGE_SIZE_2K;

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
