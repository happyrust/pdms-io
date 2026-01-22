use std::path::PathBuf;

use crate::parse::parse_file;

/// 针对 test-files/ams7330_0001 的解析冒烟测试
#[tokio::test]
async fn test_parse_ams7330_0001_smoke() {
    let path = PathBuf::from("test-files/ams7330_0001");
    // 项目名前缀用于 file_name 前缀解析 field_no
    let project = "ams7330";
    let file_name = "ams7330_0001";

    let result = parse_file(&path, &None, file_name, project).await;
    match result {
        Ok(db) => {
            assert!(
                !db.total_attr_map.is_empty(),
                "解析成功但未得到任何属性"
            );
        }
        Err(e) => panic!("解析 ams7330_0001 失败: {e:?}"),
    }
}
