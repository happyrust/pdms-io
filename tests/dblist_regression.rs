use pdms_io::dblist::parse_dblist_file;
use std::fs;

const SAMPLE_TXT: &str = "tests/data/-CCV-S-2-H-1105_7997.txt";
const SNAPSHOT_JSON: &str = "tests/data/-CCV-S-2-H-1105_7997.json";

#[test]
fn dblist_snapshot_regression() {
    let doc = parse_dblist_file(SAMPLE_TXT).expect("解析 DBLIST 失败");
    assert!(doc.warnings.is_empty(), "解析产生警告: {:?}", doc.warnings);

    let actual = serde_json::to_value(&doc).expect("序列化失败");

    if std::env::var("UPDATE_DBLIST_SNAPSHOT").as_deref() == Ok("1") {
        let pretty = serde_json::to_string_pretty(&actual).expect("序列化失败");
        fs::write(SNAPSHOT_JSON, pretty).expect("写入快照失败");
        return;
    }

    let expected_str = fs::read_to_string(SNAPSHOT_JSON).expect("读取快照失败");
    let expected: serde_json::Value =
        serde_json::from_str(&expected_str).expect("快照 JSON 无法解析");
    assert_eq!(actual, expected);
}
