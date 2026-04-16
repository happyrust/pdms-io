use crate::io::PdmsIO;
use crate::test::resolve_test_db_path;
use aios_core::tool::db_tool::db1_dehash;
use aios_core::{RefU64, init_test_surreal};

#[tokio::test]
async fn test_parse_ele() {
    let refno: RefU64 = "17496/269393".into();
    //首先要根据参考号的索引结构找到这个数据
    // let refno_loc =
    let db_path = match resolve_test_db_path("ams1112_0001") {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return;
        }
    };
    let mut io = PdmsIO::new("ams", &db_path, true);
    let att = io.auto_get_elements_deep(refno).await;
    dbg!(att);
}

#[cfg(feature = "surrealdb")]
#[tokio::test]
async fn test_read_all_sessions() -> anyhow::Result<()> {
    init_test_surreal().await;
    let db_path = match resolve_test_db_path("ams1112_0001") {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return Ok(());
        }
    };
    crate::io::sync_all_history_data(db_path.to_string_lossy().as_ref())
        .await
        .unwrap();

    Ok(())
}
