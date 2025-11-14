#[tokio::test]
async fn test_parse_ele() {
    let refno: RefU64 = "17496/269393".into();
    //首先要根据参考号的索引结构找到这个数据
    // let refno_loc =
    let db_path = "D:/AVEVA/Projects/E3D2.1/AvevaMarineSample/ams000/ams1112_0001";
    let mut io = PdmsIO::new("ams", db_path, true);
    let att = io.auto_get_elements_deep(refno).await;
    dbg!(att);
}

#[tokio::test]
async fn test_read_all_sessions() -> anyhow::Result<()> {
    init_test_surreal().await;
    #[cfg(target_os = "windows")]
    let db_path = "D:/AVEVA/Projects/E3D2.1/AvevaMarineSample/ams000/ams1112_0001";
    #[cfg(target_os = "macos")]
    let db_path =
        "/Users/dongpengcheng/Documents/models/e3d_models/AvevaMarineSample/ams000/ams1112_0001";
    crate::io::sync_all_history_data(db_path).await.unwrap();

    Ok(())
}
