use crate::parse::parse_ele_data;
use crate::test_cases::convert_str_to_bytes;
use aios_core::get_default_pdms_db_info;
use aios_core::pdms_types::RefU64;
use aios_core::tool::db_tool::db1_dehash;

#[tokio::test]
async fn test_uda_dehash() {
    let hash = db1_dehash(0x2902D6E0);
    assert_eq!(":CNPEspco".to_string(), hash);
    let hash = db1_dehash(0xE473396C);
    let refno = RefU64(0xE473396C);
    dbg!(refno);
    assert_eq!(":3D_SJZT".to_string(), hash);
    let hash = db1_dehash(642951949);
    assert_eq!(":3D_SJRY".to_string(), hash);
}

// ams desi 24381/48631
// 当前值  :4WO
// 期望值  :3D_SJRY
#[tokio::test]
async fn test_parse_uda_data_24381_48631() {
    let data_str = "
    00 00 00 29 00 00 5C 20 00 00 15 D4 00 08 F3 A6
00 00 5C 20 00 00 15 D1 00 00 26 F5 00 05 40 01
00 00 00 00 00 00 00 00 20 08 C0 00 00 00 00 03
00 00 00 00 40 C1 FA 80 00 00 00 00 40 C8 06 00
00 00 00 00 40 8A 54 00 00 00 00 03 00 00 00 00
00 00 00 00 00 00 00 00 C0 56 80 00 00 00 00 00
00 00 00 00 00 00 00 0C 00 00 3B 58 00 03 80 68
00 00 3B 58 00 03 80 27 00 00 00 01 00 00 00 02
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 02 2F B0 E7 4C
80 00 00 01 00 01 00 28 00 00 5C 20 00 00 15 D4
00 00 00 00 00 00 00 00 00 0A AF CA 14 00 00 01
00 00 00 00 00 09 2E A7 0C 00 00 01 FF FF FF FF
00 0B C6 C0 14 00 00 01 00 00 00 01 06 A0 26 04
0C 00 00 01 00 0D F3 17 10 71 D1 20 08 00 00 02
00 00 00 00 00 00 00 00 10 71 D1 2B 08 00 00 02
00 00 00 00 00 00 00 00 00 0D FD 22 14 00 00 01
00 00 00 00 00 CC 6B 3F 38 00 00 02 00 00 00 01
00 08 F3 A6 00 08 DF C1 1C 00 00 02 00 00 00 01
00 00 00 00 2C F2 AE D5 28 00 00 02 00 00 00 04
74 65 73 74 00 00 00 00 00 00 00 00
    ";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    for (key, value) in ele_data.whole_attmap.explicit_attmap.map {
        dbg!(&key);
        dbg!(&value);
    }
}

#[tokio::test]
async fn test_14194_4_udna() {
    let data_str = "00 00 00 1B 00 00 37 72 00 00 00 04 00 08 1F 4B
00 00 37 72 00 00 00 03 00 00 00 4B 00 03 80 01
00 00 00 00 00 00 00 00 20 14 40 00 29 02 D6 DA
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 08 00 0E 62 A0
00 00 00 78 00 00 00 00 00 00 00 00 00 00 00 00
00 09 C5 E1 00 09 C5 E1 00 00 00 00 00 01 00 56
00 00 37 72 00 00 00 04 00 00 00 00 00 00 00 00
00 0A FA 16 28 00 00 02 00 00 00 04 4E 4F 4E 45
11 A9 8C A9 14 00 00 01 00 00 00 00 00 09 39 40
28 00 00 04 00 00 00 0C E6 88 BF E9 97 B4 E5 90
8D E7 A7 B0 01 56 07 8A 28 00 00 03 00 00 00 08
53 43 52 6F 6F 6D 4E 6F 00 09 C1 8E 3C 00 00 04
00 00 00 09 2F 53 43 52 6F 6F 6D 4E 6F 00 00 00
00 0B C6 1B 1C 00 00 37 00 00 00 36 00 08 A3 E5
0A C6 3D CE 0A 96 7C D1 0A E3 12 EE 00 09 77 E4
00 08 61 E0 00 0C A7 B1 00 08 49 9F 00 0D FC C8
00 0D 27 86 00 0D B0 BD 00 0A F4 BE 00 0D 0F 45
00 0A BA 1B 00 0C A4 39 00 0B 0D 89 00 0D BF 68
00 0A F2 54 00 0E 40 D2 00 0C 54 7E 00 08 F3 A6
00 0B 9F EF 00 0B D2 23 00 09 BF 1B 00 09 07 CD
00 0E 55 4B 00 0A B9 B8 00 0F 7C 34 00 0C D6 91
00 0B FE 25 00 10 36 AC 00 0D CC D6 00 0C D8 26
00 0E 54 79 00 0E 2D 3D 00 08 8B 61 00 0C 2E 90
00 0C 5F EC 00 0F 56 3E 00 08 9E C9 00 0E 76 8D
00 0D B0 CC 00 0C 89 B3 00 09 BF 92 00 0E 53 1E
00 09 A4 5C 00 0D D8 C6 00 08 2A C9 00 09 D0 8E
00 0D FA A2 03 DB 13 9A 10 1D 50 ED 00 0E D9 D0
00 09 72 47";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    let _ele_data = parse_ele_data(data.as_slice()).await.unwrap();
}

#[tokio::test]
async fn test_udna_1() {
    let data_str = "
00 00 00 1B 00 00 33 EC 00 00 00 B9 00 08 1F 4B
00 00 33 EC 00 00 00 B8 00 00 00 46 00 35 C0 01
00 00 00 00 00 00 00 00 20 05 C0 00 26 52 AB 9E
00 00 00 08 45 2D 57 65 69 67 68 74 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 03 00 0B BA 07
00 00 00 78 00 00 00 00 00 00 00 00 00 00 00 00
00 09 C5 E1 00 09 C5 E1 17 B7 20 9E 00 01 00 1C
00 00 33 EC 00 00 00 B9 00 00 00 00 00 00 00 00
00 0A FA 16 28 00 00 02 00 00 00 04 4E 4F 4E 45
04 1F E8 B1 14 00 00 01 00 00 00 00 00 09 C1 8E
3C 00 00 04 00 00 00 09 2F 45 2D 57 65 69 67 68
74 00 00 00 00 09 39 40 28 00 00 04 00 00 00 0C
45 6D 70 74 79 20 57 65 69 67 68 74 00 0B C6 1B
1C 00 00 02 00 00 00 01 00 AC 03 06";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    dbg!(&ele_data.whole_attmap.explicit_attmap);
}

//15198/530
#[tokio::test]
async fn test_13292_185_udna() {
    let data_str = "
    00 00 00 1B 00 00 3B 5E 00 00 02 12 00 08 1F 4B
00 00 3B 5E 00 00 02 0D 00 00 00 13 00 00 20 01
00 00 00 00 00 00 00 00 20 0B 40 00 2C 00 D5 7D
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 09 00 08 31 81
00 00 00 01 00 00 00 00 00 00 00 00 00 00 00 00
00 08 31 81 00 09 C5 E1 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 07 00 01 00 32 00 00 3B 5E
00 00 02 12 00 00 00 00 00 00 00 00 00 0A FA 16
28 00 00 02 00 00 00 04 4E 4F 4E 45 00 09 39 40
28 00 00 07 00 00 00 18 43 6F 6E 73 69 73 74 65
6E 63 79 20 63 68 65 63 6B 20 72 65 73 75 6C 74
00 0B C6 1B 1C 00 00 02 00 00 00 01 00 0C 55 1C
00 0E 40 7F 28 00 00 03 00 00 00 05 46 41 4C 53
45 00 00 00 01 56 07 8A 28 00 00 04 00 00 00 09
50 46 43 6F 6E 73 43 68 6B 00 00 00 00 09 C1 8E
3C 00 00 08 00 00 00 19 2F 50 46 43 6F 6E 73 69
73 74 65 6E 63 79 43 68 65 63 6B 52 65 73 75 6C
74 00 00 00 00 0E 20 EC 28 00 00 05 00 00 00 10
50 69 70 65 20 66 61 62 72 69 63 61 74 69 6F 6E
";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    dbg!(&ele_data.whole_attmap.explicit_attmap);
}
