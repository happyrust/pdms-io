use crate::parse::parse_ele_data;

const AMS5054_PTCA_13246_243899: &[u8] = include_bytes!("fixtures/ams5054_ptca_13246_243899.bin");

#[tokio::test]
async fn test_parse_ams5054_ptca_13246_243899_expressions() {
    let ele_data = parse_ele_data(AMS5054_PTCA_13246_243899)
        .await
        .expect("ams5054 PTCA fixture should parse");
    let map = &ele_data.whole_attmap.attmap;

    assert_eq!(map.get_as_string("TYPE").as_deref(), Some("PTCA"));
    assert_eq!(
        map.get_as_string("NAME").as_deref(),
        Some("/AA/NI/VALVE/YJUSSARFG01-P-18")
    );
    assert_eq!(map.get_as_string("REFNO").as_deref(), Some("13246/243899"));
    assert_eq!(map.get_as_string("OWNER").as_deref(), Some("13246/243889"));
    assert_eq!(map.get_i32("NUMB"), Some(18));

    assert_eq!(map.get_as_string("PCON").as_deref(), Some(" 0"));
    assert_eq!(map.get_as_string("PBOR").as_deref(), Some(" 0"));
    assert_eq!(map.get_as_string("PX").as_deref(), Some("411"));
    assert_eq!(
        map.get_as_string("PY").as_deref(),
        Some("- (0.5 * ATTRIB PARA[8 ]) - 53")
    );
    assert_eq!(
        map.get_as_string("PZ").as_deref(),
        Some("ATTRIB PARA[7 ] + 0.22 * ATTRIB PARA[9 ] + 25")
    );

    // 金标准: E3D Q ATT 输出 `Ptcdirection Y`（方向字面量表达式,固定 7-word 编码）
    let merged = ele_data.whole_attmap.merge();
    assert_eq!(merged.get_as_string("PTCD").as_deref(), Some("Y"));
}
