use aios_core::{init_test_surreal, RefU64};
use crate::io::PdmsIO;
use crate::test::resolve_test_db_path;



#[tokio::test]
#[ignore] // 需要实际的数据库文件才能运行
async fn test_get_sesno_timestamp() -> anyhow::Result<()> {
    // 测试通过 sesno 获取时间戳的新功能
    // 注意：这个测试需要实际的 PDMS 数据库文件
    let db_path = match resolve_test_db_path("ams000/ams1112_0001") {
        Some(path) => path,
        None => {
            println!("跳过测试：数据库文件不存在 ams000/ams1112_0001");
            return Ok(());
        }
    };

    // 检查文件是否存在
    if !std::path::Path::new(&db_path).exists() {
        println!("跳过测试：数据库文件不存在 {}", db_path.display());
        return Ok(());
    }

    let mut io = crate::io::PdmsIO::new("ams", &db_path, true);
    io.open()?;
    io.init_ses_range_map()?;

    // 获取最新的会话号进行测试
    let latest_sesno = io.get_latest_sesno()?;
    println!("最新会话号: {}", latest_sesno);

    // 测试获取 DateTime<Utc>
    let datetime = io.get_sesno_datetime(latest_sesno)?;
    println!("会话 {} 的保存时间: {}", latest_sesno, datetime);

    // 测试获取 Unix 时间戳
    let timestamp = io.get_sesno_timestamp(latest_sesno)?;
    println!("会话 {} 的时间戳: {}", latest_sesno, timestamp);

    // 验证两种方法返回的时间是一致的
    assert_eq!(datetime.timestamp(), timestamp);

    // 测试一个较早的会话号（如果存在）
    if latest_sesno > 1 {
        let earlier_sesno = latest_sesno - 1;
        if let Ok(earlier_datetime) = io.get_sesno_datetime(earlier_sesno) {
            let earlier_timestamp = io.get_sesno_timestamp(earlier_sesno)?;
            println!("会话 {} 的保存时间: {}", earlier_sesno, earlier_datetime);
            println!("会话 {} 的时间戳: {}", earlier_sesno, earlier_timestamp);

            // 验证时间一致性
            assert_eq!(earlier_datetime.timestamp(), earlier_timestamp);

            // 验证时间顺序（较早的会话应该有较早的时间）
            assert!(earlier_timestamp <= timestamp);
        }
    }

    Ok(())
}

