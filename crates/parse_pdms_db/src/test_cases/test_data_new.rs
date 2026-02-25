use crate::parse::parse_ele_data;
use crate::test_cases::convert_str_to_bytes;
use aios_core::tool::db_tool::{db1_dehash, db1_hash};
use aios_core::{PdmsDatabaseInfo, get_default_pdms_db_info};

#[tokio::test]
async fn test_sample_2013286748_1428() {
    // issue :https://gitee.com/happydpc/aios-parse-pdms/issues/I4QBEC
    let data_str = "00 00 00 2D 78 00 51 5C 00 00 05 94 00 0E 55 4B
78 00 51 5C 00 00 05 76 00 00 04 81 00 05 C0 01
00 00 00 00 00 00 00 00 20 06 C0 00 00 00 00 03
92 93 3E 18 C0 CA E3 02 32 FE CE EC 40 91 E5 BF
2B BD 07 40 40 8D 04 5A 00 00 00 03 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
40 66 80 00 00 00 00 0C 00 00 3B 58 00 03 86 75
00 00 3B 58 00 03 80 23 00 00 00 01 00 00 00 02
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 40 56 80 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 02 35 A5 11 77
80 00 00 01 00 01 00 20 78 00 51 5C 00 00 05 94
00 00 00 00 00 00 00 00 00 09 2E A7 0C 00 00 01
FF FF FF FF 00 0B C6 C0 14 00 00 01 00 00 00 01
06 A0 26 04 0C 00 00 01 00 0D F3 17 00 CC 6B 3F
38 00 00 02 00 00 00 01 00 0E 55 4B 00 09 C1 8E
3C 00 00 05 00 00 00 0F 2F 43 6F 70 79 2D 6F 66
2D 46 56 2D 31 31 35 00 29 02 D6 DA 28 00 00 05
00 00 00 0D 31 41 52 2D 52 4D 30 36 2D 41 36 32
35 00 00 00
";
    let _data = convert_str_to_bytes(data_str);
    let pdms_database_info = get_default_pdms_db_info();
    if let Some(map) = pdms_database_info
        .noun_attr_info_map
        .get(&(db1_hash("SECT") as i32))
    {
        dbg!(map.value());
    };
    // let mut lookup = StringLookupTable::default();
    // let ele_data = parse_ele_data(data.as_slice(), &pdms_database_info.noun_attr_info_map,&mut lookup).unwrap();
    // if let Some(value) = lookup.lookup.get(&1433536923){
    //     println!("string={:?}",value.value());
    // }
    // dbg!(&ele_data.whole_attmap.merge());
}

#[tokio::test]
async fn test_24575_228_sample() {
    let data_str = "
00 00 00 0D 00 00 5F FF 00 00 00 E4 00 0C C3 A5
00 00 5F FF 00 00 00 DB 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 20 00 00 00 00 00 00 01
FF FF FF FF 31 41 52 2D 52 4D 30 36 2D 41 36 32
35 00 00 00";
    let data = convert_str_to_bytes(data_str);
    // let pdms_database_info = read_attr_info_config("all_attr_info.bin");
    // let pdms_database_info = get_default_pdms_db_info();
    let _pdms_database_info = get_default_pdms_db_info();
    // if let Some(map) = pdms_database_info.noun_attr_info_map.get(&0xCC3A5) {
    //     dbg!(map.value());
    // }
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    println!("ele_data={:?}", ele_data.whole_attmap);
    // if let Some(value) = lookup.lookup.get(&1433536923){
    //     println!("string={:?}",value.value());
    // }
    // dbg!(&ele_data.whole_attmap.merge());
}

#[tokio::test]
async fn test_sample_mdb() {
    let data_str = "00 00 00 0B 00 00 5F FF 00 00 02 15 00 08 22 1C
00 00 5F FF 00 00 00 07 00 00 02 1F 00 1C 60 01
00 00 02 1F 00 11 80 01 20 2B C0 52 00 02 00 57
00 00 5F FF 00 00 02 15 00 00 00 00 00 00 00 00
00 00 5F FF 00 00 01 E7 00 00 5F FF 00 00 02 09
00 00 5F FF 00 00 01 F9 00 00 5F FF 00 00 01 F8
00 00 5F FF 00 00 02 01 00 00 5F FF 00 00 01 F4
00 00 5F FF 00 00 02 08 00 00 5F FF 00 00 01 E8
00 00 5F FF 00 00 02 0B 00 00 5F FF 00 00 02 0A
00 00 5F FF 00 00 02 0E 00 00 5F FF 00 00 02 0F
00 00 5F FF 00 00 02 10 00 00 5F FF 00 00 02 11
00 00 5F FF 00 00 02 12 00 00 5F FF 00 00 02 13
00 00 5F FF 00 00 02 14 00 00 5F FF 00 00 02 0C
00 00 5F FF 00 00 02 05 00 00 5F FF 00 00 01 E9
00 00 5F FF 00 00 01 D4 00 00 5F FF 00 00 01 E5
00 00 5F FF 00 00 01 D6 00 00 5F FF 00 00 01 D7
00 00 5F FF 00 00 01 D8 00 00 5F FF 00 00 01 DD
00 00 5F FF 00 00 01 DC 00 00 5F FF 00 00 01 D9
00 00 5F FF 00 00 01 DA 00 00 5F FF 00 00 01 E6
00 00 5F FF 00 00 01 DB 00 00 5F FF 00 00 01 DE
00 00 5F FF 00 00 01 E3 00 00 5F FF 00 00 02 0D
00 00 5F FF 00 00 02 07 00 00 5F FF 00 00 01 DF
00 00 5F FF 00 00 02 06 00 00 5F FF 00 00 01 E0
00 00 5F FF 00 00 01 E1 00 00 5F FF 00 00 01 E4
00 00 5F FF 00 00 01 E2 00 01 00 B4 00 00 5F FF
00 00 02 15 00 00 00 00 00 00 00 00 00 09 C1 8E
3C 00 00 03 00 00 00 07 2F 53 41 4D 50 4C 45 00
00 0D F3 30 20 00 00 53 00 00 00 29 00 00 5F FF
00 00 01 E2 00 00 5F FF 00 00 01 E4 00 00 5F FF
00 00 01 E1 00 00 5F FF 00 00 01 E0 00 00 5F FF
00 00 02 06 00 00 5F FF 00 00 01 DF 00 00 5F FF
00 00 02 07 00 00 5F FF 00 00 02 0D 00 00 5F FF
00 00 01 E3 00 00 5F FF 00 00 01 DE 00 00 5F FF
00 00 01 DB 00 00 5F FF 00 00 01 E6 00 00 5F FF
00 00 01 DA 00 00 5F FF 00 00 01 D9 00 00 5F FF
00 00 01 DC 00 00 5F FF 00 00 01 DD 00 00 5F FF
00 00 01 D8 00 00 5F FF 00 00 01 D7 00 00 5F FF
00 00 01 D6 00 00 5F FF 00 00 01 E5 00 00 5F FF
00 00 01 D4 00 00 5F FF 00 00 01 E9 00 00 5F FF
00 00 02 05 00 00 5F FF 00 00 02 0C 00 00 5F FF
00 00 02 14 00 00 5F FF 00 00 02 13 00 00 5F FF
00 00 02 12 00 00 5F FF 00 00 02 11 00 00 5F FF
00 00 02 10 00 00 5F FF 00 00 02 0F 00 00 5F FF
00 00 02 0E 00 00 5F FF 00 00 02 0A 00 00 5F FF
00 00 02 0B 00 00 5F FF 00 00 01 E8 00 00 5F FF
00 00 02 08 00 00 5F FF 00 00 01 F4 00 00 5F FF
00 00 02 01 00 00 5F FF 00 00 01 F8 00 00 5F FF
00 00 01 F9 00 00 5F FF 00 00 02 09 00 00 5F FF
00 00 01 E7 00 09 84 F9 20 00 00 53 00 00 00 29
00 00 5F FF 00 00 01 E7 00 00 5F FF 00 00 02 09
00 00 5F FF 00 00 01 F9 00 00 5F FF 00 00 01 F8
00 00 5F FF 00 00 02 01 00 00 5F FF 00 00 01 F4
00 00 5F FF 00 00 02 08 00 00 5F FF 00 00 01 E8
00 00 5F FF 00 00 02 0B 00 00 5F FF 00 00 02 0A
00 00 5F FF 00 00 02 0E 00 00 5F FF 00 00 02 0F
00 00 5F FF 00 00 02 10 00 00 5F FF 00 00 02 11
00 00 5F FF 00 00 02 12 00 00 5F FF 00 00 02 13
00 00 5F FF 00 00 02 14 00 00 5F FF 00 00 02 0C
00 00 5F FF 00 00 02 05 00 00 5F FF 00 00 01 E9
00 00 5F FF 00 00 01 D4 00 00 5F FF 00 00 01 E5
00 00 5F FF 00 00 01 D6 00 00 5F FF 00 00 01 D7
00 00 5F FF 00 00 01 D8 00 00 5F FF 00 00 01 DD
00 00 5F FF 00 00 01 DC 00 00 5F FF 00 00 01 D9
00 00 5F FF 00 00 01 DA 00 00 5F FF 00 00 01 E6
00 00 5F FF 00 00 01 DB 00 00 5F FF 00 00 01 DE
00 00 5F FF 00 00 01 E3 00 00 5F FF 00 00 02 0D
00 00 5F FF 00 00 02 07 00 00 5F FF 00 00 01 DF
00 00 5F FF 00 00 02 06 00 00 5F FF 00 00 01 E0
00 00 5F FF 00 00 01 E1 00 00 5F FF 00 00 01 E4
00 00 5F FF 00 00 01 E2 ";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    // if let Some(map) = pdms_database_info.noun_attr_info_map.get(&0xCC3A5) {
    //     dbg!(map.value());
    // }
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    println!("ele_data={:?}", ele_data.whole_attmap);
}

#[tokio::test]
async fn test_room_code_sample() {
    let data_str = "00 00 00 29 78 00 51 5C 00 00 21 65 00 08 F3 A6
78 00 51 5C 00 00 21 64 00 00 09 56 00 15 00 01
00 00 00 00 00 00 00 00 20 04 80 00 00 00 00 03
7A E1 47 A0 40 C9 B0 74 47 AE 14 80 C0 CF 71 81
00 00 00 00 C0 A1 F8 00 00 00 00 03 00 00 00 00
40 56 80 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 0C 00 00 3B 6D 00 03 EF F2
00 00 3B 6D 00 03 EF E8 00 00 00 01 00 00 00 02
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 40 9F 40 00 00 00 00 02 11 E3 A0 D7
80 00 00 01 00 01 00 17 78 00 51 5C 00 00 21 65
00 00 00 00 00 00 00 00 00 09 2E A7 0C 00 00 01
FF FF FF FF 00 0B C6 C0 14 00 00 01 00 00 00 01
06 A0 26 04 0C 00 00 01 00 36 7E CC 00 CC 6B 3F
38 00 00 02 00 00 00 01 00 08 F3 A6 29 02 D6 DA
28 00 00 03 00 00 00 05 31 52 31 30 31 00 00 00 ";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    // if let Some(map) = pdms_database_info.noun_attr_info_map.get(&0xCC3A5) {
    //     dbg!(map.value());
    // }
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    println!("ele_data={:?}", ele_data.whole_attmap);
}

#[tokio::test]
async fn test_sample_23584_2702() {
    let data_str = "00 00 00 21 00 00 5C 20 00 00 0A 8E 00 09 A4 5C
00 00 5C 20 00 00 00 E0 00 00 01 7E 00 31 20 01
00 00 01 7E 00 30 40 01 20 15 00 02 00 0C 7B 76
00 00 00 00 00 00 00 00 00 00 00 03 00 00 00 00
00 00 00 00 00 00 00 00 C0 B2 93 00 00 00 00 00
40 7C 20 00 00 00 00 03 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 02 00 07 00 00 5C 20 00 00 0A 8E
00 00 00 00 00 00 00 00 00 00 5C 20 00 00 0A 8F
00 01 00 59 00 00 5C 20 00 00 0A 8E 00 00 00 00
00 00 00 00 00 CC 6B 3F 38 00 00 02 00 00 00 01
00 09 A4 5C 00 0D 20 C7 18 00 00 03 00 00 00 01
00 00 00 00 40 72 C0 00 0F 7A 2C C8 1C 00 00 02
00 00 00 01 00 09 C5 E1 00 09 D1 65 40 00 00 02
00 00 3B 66 00 00 01 3D FF F7 C8 79 1C 00 00 1A
00 00 00 19 00 00 00 00 00 00 00 08 00 00 00 17
00 00 00 01 00 00 00 6D 00 00 00 14 00 00 00 05
00 00 00 03 00 00 00 2A 00 00 00 02 00 00 00 15
00 00 00 0D 00 00 00 3D 00 00 00 02 00 00 00 15
00 00 00 0D 00 00 00 3D 00 00 00 02 00 00 00 15
00 00 00 0F 00 00 00 3D 00 00 00 02 00 00 00 15
00 00 00 0F 00 00 00 3D FF F7 AC 4F 1C 00 00 25
00 00 00 24 00 00 00 00 00 00 00 06 00 00 00 22
00 00 00 01 00 00 00 6B 00 00 00 1F 00 00 00 05
00 00 00 01 00 00 00 24 00 00 00 22 00 00 00 51
00 00 00 19 00 00 00 02 00 00 00 52 00 00 00 2D
00 00 00 44 00 00 00 61 00 00 00 74 00 00 00 75
00 00 00 6D 00 00 00 2D 00 00 00 50 00 00 00 6F
00 00 00 69 00 00 00 6E 00 00 00 74 00 00 00 0D
00 00 00 25 00 00 00 12 00 0B E5 98 00 00 00 06
00 00 00 01 00 00 00 07 00 00 00 12 00 0B 0D 89
00 00 00 06";
    let data = convert_str_to_bytes(data_str);
    let pdms_database_info: PdmsDatabaseInfo = get_default_pdms_db_info().clone();
    if let Some(map) = pdms_database_info.noun_attr_info_map.get(&0x9A45C) {
        dbg!(map.value());
    }
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    println!("ele_data={:?}", ele_data.whole_attmap);
}

#[tokio::test]
async fn test_sample_23584_5703() {
    let data_str = "00 00 00 2B 00 00 5C 20 00 00 16 47 00 0C 54 7E
00 00 5C 20 00 00 16 3D 00 00 03 2F 00 39 60 01
00 00 00 00 00 00 00 00 20 09 40 00 00 00 00 03
00 00 00 00 40 C2 3E 00 00 00 00 00 40 C7 40 C0
00 00 00 00 40 8B 98 00 00 00 00 03 00 00 00 00
00 00 00 00 00 00 00 00 40 56 80 00 00 00 00 00
00 00 00 00 00 00 00 0C 00 00 3B 58 00 03 83 BC
00 00 3B 58 00 03 80 2D 00 00 00 01 00 00 00 02
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 02 53 72 94 35
80 00 00 01 00 00 00 00 00 00 00 00 00 01 00 2A
00 00 5C 20 00 00 16 47 00 00 00 00 00 00 00 00
00 0A AF CA 14 00 00 01 00 00 00 00 00 09 2E A7
0C 00 00 01 FF FF FF FF 00 0B C6 C0 14 00 00 01
00 00 00 01 06 A0 26 04 0C 00 00 01 00 0D F3 17
10 71 D1 20 08 00 00 02 00 00 00 00 00 00 00 00
10 71 D1 2B 08 00 00 02 00 00 00 00 00 00 00 00
0F B7 7D 24 18 00 00 07 00 00 00 03 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 CC 6B 3F 38 00 00 02 00 00 00 01
00 0C 54 7E 00 08 DF C1 1C 00 00 02 00 00 00 01
00 00 00 00";
    let data = convert_str_to_bytes(data_str);
    let pdms_database_info: PdmsDatabaseInfo = get_default_pdms_db_info().clone();
    if let Some(map) = pdms_database_info
        .noun_attr_info_map
        .get(&(db1_hash("MDB") as i32))
    {
        dbg!(map.value());
    }
    let hash = db1_hash("CURD");
    println!("hash={:?}", hash);
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    println!("ele_data={:?}", ele_data.whole_attmap);
    // if let Some(noll) = ele_data.whole_attmap.implicit_attmap.get(&(835759)) {
    //     let noll = noll.double_value().await.unwrap();
    //     assert_eq!(0.0, noll);
    // }
}

#[tokio::test]
async fn test_uda() {
    let data_str = "00 00 00 1B 00 00 3B 78 00 00 3D 5E 00 08 1F 4B
00 00 3B 78 00 00 3D 5C 00 00 01 CC 00 0F 20 01
00 00 00 00 00 00 00 00 20 07 00 00 2C 14 B1 A7
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 03 00 0E 62 A0
00 00 00 0A 00 00 00 00 00 00 00 00 00 00 00 00
00 0E 62 A0 00 09 C5 E1 00 00 00 00 00 01 00 21
00 00 3B 78 00 00 3D 5E 00 00 00 00 00 00 00 00
00 0A FA 16 28 00 00 02 00 00 00 04 4E 4F 4E 45
11 A9 8C A9 14 00 00 01 00 00 00 00 00 09 39 40
28 00 00 06 00 00 00 12 E7 AE A1 E9 81 93 E5 AE
89 E5 85 A8 E7 AD 89 E7 BA A7 00 00 00 0B C6 1B
1C 00 00 02 00 00 00 01 00 09 CA F3 00 0E 40 7F
28 00 00 03 00 00 00 05 55 4E 53 45 54 00 00 00
01 56 07 8A 28 00 00 02 00 00 00 04 41 51 44 4A";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    dbg!(&db1_dehash(641779));
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();

    dbg!(&ele_data.whole_attmap.explicit_attmap);
}

#[tokio::test]
async fn test_detr_15192_232504() {
    let data_str = "00 00 00 19 00 00 3B 58 00 03 8C 38 00 0C A7 8C
00 00 3B 58 00 06 0B 7A 00 00 68 F4 00 2C 00 01
00 00 00 00 00 00 00 00 20 03 80 00 00 00 00 00
40 46 80 00 00 00 00 00 40 46 80 00 00 00 3B 58
00 00 12 15 00 00 3B 58 00 05 C7 70 00 00 3B 58
00 00 D5 38 00 00 3B 60 00 00 89 17 00 00 00 00
00 00 00 00 00 01 00 13 00 00 3B 58 00 03 8C 38
00 00 00 00 00 00 00 00 05 57 C1 73 40 00 00 02
00 00 00 00 00 00 00 00 00 0D F8 D7 28 00 00 02
00 00 00 04 54 52 55 45 00 09 C1 8E 3C 00 00 04
00 00 00 09 2F 46 31 43 2F 45 43 36 35 00 00 00";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();

    dbg!(&ele_data.whole_attmap.explicit_attmap);
}

#[tokio::test]
async fn test_skey_15192_762() {
    let data_str = "00 00 00 11 00 00 3B 58 00 00 02 FA 00 09 D5 D3
00 00 3B 58 00 00 02 F9 00 00 0C 83 00 02 40 01
00 00 00 00 00 00 00 00 20 05 80 00 00 00 00 04
56 43 46 4C 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 01 00 1B 00 00 3B 58 00 00 02 FA
00 00 00 00 00 00 00 00 00 CC 6B 3F 38 00 00 02
00 00 00 01 00 09 D5 D3 00 0F 61 A4 28 00 00 0A
00 00 00 24 57 4F 52 43 45 53 54 45 52 20 45 4C
45 43 20 4F 50 20 43 54 52 4C 20 56 41 4C 56 45
20 23 31 35 30 20 52 46 00 09 C1 8E 3C 00 00 04
00 00 00 0A 2F 57 43 49 46 42 42 45 2D 44 00 00";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();

    dbg!(&ele_data.whole_attmap.explicit_attmap);
}

#[tokio::test]
async fn test_skey_15192_464() {
    let data_str = "00 00 00 11 00 00 3B 58 00 00 01 D0 00 09 D5 D3
00 00 3B 58 00 00 01 CF 00 00 2F B2 00 09 20 01
00 00 00 00 00 00 00 00 20 05 00 00 00 00 00 02
4F 50 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 01 00 19 00 00 3B 58 00 00 01 D0
00 00 00 00 00 00 00 00 00 CC 6B 3F 38 00 00 02
00 00 00 01 00 09 D5 D3 00 0F 61 A4 28 00 00 08
00 00 00 1C 4F 52 49 46 49 43 45 20 50 4C 41 54
45 20 33 4D 4D 20 26 20 36 4D 4D 20 23 33 30 30
00 09 C1 8E 3C 00 00 04 00 00 00 0A 2F 41 30 49
51 42 44 30 2D 44 00 00 ";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();

    dbg!(&ele_data.whole_attmap.explicit_attmap);
}

#[tokio::test]
async fn test_anci_17496_167490() {
    let data_str = "00 00 00 3A 00 00 44 58 00 02 8E 42 00 0A D9 F2
00 00 44 58 00 02 8E 41 00 00 29 E4 00 32 C0 01
00 00 00 00 00 00 00 00 20 0F 40 00 00 00 00 00
00 00 00 03 00 00 00 00 C0 D6 B7 00 00 00 00 00
C0 C8 18 00 00 00 00 00 C0 94 50 00 00 00 00 03
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 C0 56 80 00 00 00 00 0C 00 00 5C 98
00 12 CC 7B 00 00 00 01 00 00 00 02 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 03 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 02 4D 7C 3A 35 80 00 00 01
00 00 00 00 40 66 80 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 01 00 42 00 00 44 58
00 02 8E 42 00 00 00 00 00 00 00 00 00 0A AF CA
14 00 00 01 00 00 00 00 06 A0 26 04 0C 00 00 01
00 0D F3 17 00 09 FB FA 14 00 00 01 00 00 00 01
10 71 D1 20 08 00 00 02 00 00 00 00 00 00 00 00
10 71 D1 2B 08 00 00 02 00 00 00 00 00 00 00 00
00 0D FD 22 14 00 00 01 00 00 00 00 00 CC 6B 3F
38 00 00 02 00 00 00 01 00 0A D9 F2 00 09 C1 8E
3C 00 00 06 00 00 00 13 2F 36 4B 41 30 32 2D 4D
53 55 50 2D 45 30 30 34 35 2E 36 00 00 0D 20 C7
18 00 00 05 00 00 00 02 00 00 00 00 40 7F 40 00
00 00 00 00 00 00 00 00 0F 7A 2C C8 1C 00 00 03
00 00 00 02 00 0E 54 BF 00 09 C5 E1 00 09 2F 7A
0C 00 00 01 00 08 2D B8 00 0F 61 A5 28 00 00 05
00 00 00 10 36 4B 41 30 32 2D 4D 53 55 50 2D 45
30 30 34 35 0A 03 38 58 14 00 00 01 00 00 00 01
04 E5 C3 C7 40 00 00 02 00 00 44 58 00 02 7C B6";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();

    dbg!(&ele_data.whole_attmap.explicit_attmap);
    assert_eq!(
        ele_data
            .whole_attmap
            .explicit_attmap
            .get_as_string("STEX")
            .unwrap(),
        "6KA02-MSUP-E0045"
    );
}
