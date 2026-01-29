use crate::io::PdmsIO;
use crate::test::resolve_test_db_path;

//讲session 数据保存到数据库中，后面版本更新比较的就是会话层的数据
#[tokio::test]
pub async fn test_get_max_att_pgno() {
    let db_path = match resolve_test_db_path("ams1112_0001") {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return;
        }
    };
    let mut io = PdmsIO::new("ams", &db_path, true);
    let max_att_pgno = io.get_latest_att_pgno().unwrap();
    dbg!(max_att_pgno);
    let max_att_version = io.get_latest_sesno().unwrap();
    dbg!(max_att_version);

    let incr_eles = io.collect_increment_eles(Some( 1109..=1111)).unwrap();
    dbg!(&incr_eles.len());
}
