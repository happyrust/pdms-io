use crate::parse::parse_ele_data;
use crate::test_cases::convert_str_to_bytes;
use aios_core::get_default_pdms_db_info;

//分支С隹诠档揽矶W11 & 乱码的情况处理
#[tokio::test]
async fn test_dtit() {
    let data_str = "
00 00 00 19 00 00 53 88 00 06 00 BF 00 08 A1 E7
00 00 53 88 00 06 00 B2 00 00 FD 68 00 03 40 01
00 00 00 00 00 00 00 00 20 0D 40 00 00 08 6E 1C
00 0E 54 BF 00 00 00 04 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 04 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 0D 20 C7
00 00 00 0D 00 01 00 3A 00 00 53 88 00 06 00 BF
00 00 00 00 00 00 00 00 FF F3 2D C0 1C 00 00 12
00 00 00 11 00 00 00 11 00 00 00 01 00 00 00 65
00 00 00 06 00 0A 00 00 00 00 00 00 40 00 04 02
00 00 00 00 00 00 00 00 00 00 00 6A 00 00 00 02
00 0D 20 C7 FF FF FF FF FF FF FF FF 00 00 00 00
00 00 06 41 00 00 06 A5 00 09 D4 C4 0C 00 00 01
00 00 00 01 FF F3 2D CC 1C 00 00 12 00 00 00 11
00 00 00 11 00 00 00 01 00 00 00 65 00 00 00 06
00 0A 00 00 00 00 00 00 40 00 04 02 00 00 00 00
00 00 00 00 00 00 00 6A 00 00 00 02 00 08 9C 41
FF FF FF FF FF FF FF FF 00 00 00 00 00 00 06 41
00 00 06 A5 00 0E 39 6E 28 00 00 08 00 00 00 1A
26 7E 37 56 56 27 27 33 76 3F 5A 39 35 35 40 3F
6D 36 20 26 57 31 31 02 20 26 00 00
 ";
    let data = convert_str_to_bytes(data_str);
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    if let Some(val) = ele_data.whole_attmap.explicit_attmap.get_val("DTIT") {
        let result = val.string_value();
        dbg!(&result);
        // assert_eq!(result, "综合技术廊道");
    }
}

#[tokio::test]
async fn test_znp_17500_5192_description() {
    let data_str = "
00 00 00 1D 00 00 44 5C 00 00 14 48 00 0E A0 01
00 00 44 5C 00 00 14 43 00 00 02 82 00 2B E0 01
00 00 02 82 00 2B 00 01 20 04 80 02 00 08 4B 18
00 00 00 00 00 00 00 00 00 00 00 03 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 03 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 02 00 07 00 00 44 5C 00 00 14 48
00 00 00 00 00 00 00 00 00 00 44 5C 00 00 14 49
00 01 00 17 00 00 44 5C 00 00 14 48 00 00 00 00
00 00 00 00 00 09 39 40 28 00 00 05 00 00 00 10
26 7E 57 5B 3A 4F 3C 3C 4A 75 40 48 35 40 20 26
00 09 2C B5 28 00 00 02 00 00 00 03 42 4F 50 00
00 84 9D 24 0C 00 00 01 00 00 00 02 00 CC 6B 3F
38 00 00 02 00 00 00 01 00 0E A0 01 ";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    if let Some(val) = ele_data.whole_attmap.explicit_attmap.get_val("DESC") {
        let result = val.string_value();
        assert_eq!(result, "综合技术廊道");
    }
}

#[tokio::test]
async fn test_znp_9309_2_description() {
    let data_str = "
00 00 00 1C 00 00 24 5D 00 00 00 02 00 09 D6 5A
00 00 24 5D 00 00 00 00 00 00 00 05 00 08 00 01
00 00 00 00 00 00 00 00 20 01 80 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 03 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 03 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 01 00 14 00 00 24 5D 00 00 00 02 00 00 00 00
00 00 00 00 00 09 C1 8E 3C 00 00 04 00 00 00 09
2F 31 4C 4C 2D 43 49 56 49 00 00 00 00 09 39 40
28 00 00 07 00 00 00 18 26 7E 35 67 46 78 33 27
37 3F 20 26 28 26 7E 40 48 35 40 47 78 20 26 29
";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    if let Some(val) = ele_data.whole_attmap.explicit_attmap.get_val("DESC") {
        let result = val.string_value();
        assert_eq!(result, "电气厂房(廊道区)");
    }
}
