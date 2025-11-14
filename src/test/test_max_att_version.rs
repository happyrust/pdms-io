//讲session 数据保存到数据库中，后面版本更新比较的就是会话层的数据
#[tokio::test]
pub async fn test_get_max_att_pgno() {
    let db_option = get_db_option();
    // let dir = db_option.get_project_path(&db_option.project_name).unwrap();
    let db_path = "D:/AVEVA/Projects/E3D2.1/AvevaMarineSample/ams000/ams1112_0001";
    let mut io = PdmsIO::new("ams", db_path, true);
    let max_att_pgno = io.get_latest_att_pgno().unwrap();
    dbg!(max_att_pgno);
    let max_att_version = io.get_latest_sesno().unwrap();
    dbg!(max_att_version);

    let incr_eles = io.collect_increment_eles(Some(1109..=1111)).unwrap();
    dbg!(&incr_eles.len());
}
