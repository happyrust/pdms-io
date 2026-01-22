use crate::parse::parse_ele_data;
use crate::pdms_types::{AttrVal, StringLookupTable};
use crate::test_cases::convert_str_to_bytes;
use crate::{db1_dehash, read_attr_info_config};
use smol_str::SmolStr;

#[test]
fn test_ahu_sample_15392_7313() {
    // issue :https://gitee.com/happydpc/aios-parse-pdms/issues/I4QBEC
    let data_str = "
00 00 00 2D 00 00 3C 20 00 00 1C 91 00 08 58 97
00 00 3C 20 00 00 1C 90 00 00 03 D5 00 0A 40 01
00 00 00 00 00 00 00 00 20 2F 00 00 00 00 00 03
00 00 00 00 40 CE 8C 00 00 00 00 00 40 D0 C7 00
00 00 00 00 40 7F 40 00 00 00 00 03 FF FA CD 20
C0 56 7F FF 00 00 00 00 00 00 00 00 FF FA CD 20
40 56 7F FF 00 00 00 0C 00 00 3B 5A 00 00 18 05
00 00 3B 5A 00 00 17 42 00 00 00 01 00 00 00 02
00 00 3B 5A 00 00 18 06 00 00 00 00 00 00 00 00
00 00 00 00 40 56 80 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 02 5F 5E 37 10
80 00 00 01 00 01 00 C1 00 00 3C 20 00 00 1C 91
00 00 00 00 00 00 00 00 00 09 2E A7 0C 00 00 01
FF FF FF FF 00 0B C6 C0 14 00 00 01 00 00 00 01
06 A0 26 04 0C 00 00 01 00 0D F3 17 00 09 C1 8E
3C 00 00 05 00 00 00 0F 2F 48 56 41 43 32 2D 42
31 2F 49 54 45 4D 31 00 00 0D 20 C7 18 00 00 A1
00 00 00 50 00 00 00 00 41 2D 03 0C 00 00 00 00
40 8F 40 00 00 00 00 00 40 7F 40 00 00 00 00 00
40 8F 40 00 00 00 00 00 40 7F 40 00 00 00 00 00
40 A7 70 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
40 6F 40 00 00 00 00 00 40 6F 40 00 00 00 00 00
00 00 00 00 00 00 00 00 40 62 C0 00 00 00 00 00
00 00 00 00 00 00 00 00 40 62 C0 00 00 00 00 00
00 00 00 00 00 00 00 00 40 62 C0 00 00 00 00 00
00 00 00 00 00 00 00 00 40 52 C0 00 00 00 00 00
40 49 00 00 00 00 00 00 40 18 00 00 00 00 00 00
40 52 C0 00 00 00 00 00 40 49 00 00 00 00 00 00
40 18 00 00 00 00 00 00 40 52 C0 00 00 00 00 00
40 49 00 00 00 00 00 00 40 18 00 00 00 00 00 00
41 2C 4D A2 00 00 00 00 3F F0 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
41 2C 4D A2 00 00 00 00 40 8F 40 00 00 00 00 00
40 7F 40 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
40 8A 90 00 00 00 00 00 40 92 C0 00 00 00 00 00
40 62 C0 00 00 00 00 00 40 62 C0 00 00 00 00 00
41 20 3A 0A 00 00 00 00 41 20 3A 0A 00 00 00 00
41 20 3A 0A 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
41 20 B1 2E 00 00 00 00 41 20 53 08 00 00 00 00
40 BD 7E 00 00 00 00 00 40 BD 7E 00 00 00 00 00
40 BD 7E 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 08 7B D3 20 00 00 07 00 00 00 03
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 3C 20 00 00 1C B6 ";
    let data = convert_str_to_bytes(data_str);
    let pdms_database_info = read_attr_info_config("all_attr_info.bin");
    // if let Some(map) = pdms_database_info.noun_attr_info_map.get(&0x85897i32) {
    //     //dbg!(map.value());
    // }
    let mut lookup = StringLookupTable::default();
    let ele_data = parse_ele_data(
        data.as_slice(),
        &pdms_database_info.noun_attr_info_map,
        &mut lookup,
        0,
    )
    .unwrap();
    //dbg!(&ele_data);
}

#[test]
fn test_aba_14352_102824() {
    // issue : https://gitee.com/happydpc/aios-parse-pdms/issues/I4QDGE
    let data_str = "
00 00 00 17 00 00 38 10 00 01 91 A8 00 0C A7 8C
00 00 38 10 00 01 91 A7 00 00 64 6A 00 03 00 01
00 00 00 00 00 00 00 00 00 03 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 75 E0
00 00 18 F8 00 00 75 E0 00 00 19 02 00 00 00 00
00 00 00 00 00 00 75 E0 00 00 15 44 00 01 00 11
00 00 38 10 00 01 91 A8 00 00 00 00 00 00 00 00
00 09 C1 8E 3C 00 00 06 00 00 00 11 2F 47 4E 5F
43 53 5F 32 2F 4D 31 36 78 3A 53 31 36 00 00 00
00 0D F8 D7 28 00 00 02 00 00 00 04 53 54 55 44
";
    let data = convert_str_to_bytes(data_str);
    let pdms_database_info = read_attr_info_config("all_attr_info.bin");
    // if let Some(map) = pdms_database_info.noun_attr_info_map.get(&0xCA78Ci32) {
    //     //dbg!(map.value());
    // }
    let mut lookup = StringLookupTable::default();
    let ele_data = parse_ele_data(
        data.as_slice(),
        &pdms_database_info.noun_attr_info_map,
        &mut lookup,
        0,
    )
    .unwrap();
    let mut value = SmolStr::new("");
    if let Ok(m) = ele_data.whole_attmap.merge().get_val("DETR") {
        match m {
            AttrVal::ElementType(v) => {
                value = v.clone();
            }
            _ => {}
        }
    }
    assert_eq!(value, "30176/6392");
}

#[test]
fn test_aba_8193_90707() {
    // issue : https://gitee.com/happydpc/aios-parse-pdms/issues/I4Q8BO
    // 这个是节点后是后面新增的，数据为800h里面的一部分
    // 这个数据不好截断，最好还是直接解析文件 数据在文件pos：10CDCD8h
    let data_str = "
00 00 00 1D 00 00 20 01 00 01 62 53 00 0D B0 CC
00 00 20 01 00 01 62 4B 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 03
00 00 00 00 00 00 00 00 43 8C 00 00 00 00 00 03
00 00 00 00 00 00 00 00 C2 DC 00 00 00 00 00 02
00 00 00 00 00 00 00 0A 00 00 00 02 46 87 50 00
46 88 18 00 44 A0 00 00 42 20 00 00 00 00 00 01
3B 98 21 88 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 05 00 CC 47 DF 00 00 00 01
00 00 00 02";
    let data = convert_str_to_bytes(data_str);
    let pdms_database_info = read_attr_info_config("all_attr_info.bin");
    let mut lookup = StringLookupTable::default();
    let ele_data = parse_ele_data(
        data.as_slice(),
        &pdms_database_info.noun_attr_info_map,
        &mut lookup,
        0,
    )
    .unwrap();
    //dbg!(&ele_data);
}

#[test]
fn test_sample_23984_1064() {
    let data_str = "
00 00 00 29 00 00 5D B0 00 00 04 28 00 0C 6B 50
00 00 5D B0 00 00 04 27 00 00 03 4F 00 13 60 01
00 00 00 00 00 00 00 00 20 0B 00 00 00 00 00 00
00 00 00 04 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 04 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 04 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 04
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 02 00 00 00 01 00 00 00 02 00 00 00 02
00 00 00 00 00 00 00 0A 00 00 00 00 00 00 00 00
00 00 00 00 00 01 00 31 00 00 5D B0 00 00 04 28
00 00 00 00 00 00 00 00 FF F7 E1 83 1C 00 00 2A
00 00 00 29 00 00 00 29 00 00 00 01 00 00 00 65
00 00 00 06 00 09 99 99 99 99 99 9A 40 00 03 FA
00 00 00 00 00 00 00 06 00 00 00 65 00 00 00 06
00 18 00 00 00 00 00 00 40 00 04 01 00 00 00 00
00 00 00 06 00 00 00 6A 00 00 00 02 00 08 9C 41
FF FF FF FF FF FF FF FF 00 00 00 00 00 00 06 41
00 00 06 A5 00 00 00 65 00 00 00 06 00 1C 00 00
00 00 00 00 40 00 04 03 00 00 00 00 00 00 00 06
00 00 00 6A 00 00 00 02 00 08 9C 41 FF FF FF FF
FF FF FF FF 00 00 00 00 00 00 06 41 00 00 06 A5
00 00 03 22 00 00 03 24 ";
    let data = convert_str_to_bytes(data_str);
    let pdms_database_info = read_attr_info_config("all_attr_info.bin");
    let mut lookup = StringLookupTable::default();
    let ele_data = parse_ele_data(
        data.as_slice(),
        &pdms_database_info.noun_attr_info_map,
        &mut lookup,
        0,
    )
    .unwrap();
    let mut value = SmolStr::new("");
    if let Ok(m) = ele_data.whole_attmap.merge().get_val("DX") {
        match m {
            AttrVal::StringType(v) => {
                value = v.clone();
            }
            _ => {}
        }
    }
    assert_eq!(value, "( 0.05 * ( ATTRIB PARA[6] + ATTRIB PARA[28] ) )")
}

#[test]
fn test_sann() {
    // issue : https://gitee.com/happydpc/aios-parse-pdms/issues/I4QDGE
    let data_str = "
00 00 00 40 00 00 3B 59 00 00 23 E2 00 0C 78 67
00 00 3B 59 00 00 23 E0 00 00 0D D6 00 16 C0 01
00 00 00 00 00 00 00 00 20 0C 00 00 00 00 00 04
00 00 00 00 00 00 00 01 00 00 00 00 00 00 00 00
00 00 00 04 00 00 00 00 00 00 00 01 00 00 00 00
00 00 00 00 00 00 00 02 00 00 00 02 00 00 00 02
00 00 00 04 00 00 00 28 00 00 00 01 FF FF F8 F8
00 00 00 00 00 00 00 04 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 04 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 04
00 00 00 00 00 00 00 01 00 00 00 00 00 00 00 00
00 00 00 04 00 00 00 00 00 00 00 01 00 00 00 00
00 00 00 00 00 00 00 04 00 00 00 00 00 00 00 01
00 00 00 00 00 00 00 00 00 00 00 04 00 00 00 00
00 00 00 01 00 00 00 00 00 00 00 00 00 00 00 02
00 00 00 02 00 00 00 00 00 00 00 0A 00 00 00 03
00 01 00 35 00 00 3B 59 00 00 23 E2 00 00 00 00
00 00 00 00 FF F6 AB B4 1C 00 00 1A 00 00 00 19
00 00 00 19 00 00 00 01 00 00 00 65 00 00 00 06
00 00 40 00 00 00 00 00 00 00 00 01 00 00 00 00
00 00 00 06 00 00 00 6A 00 00 00 02 00 0D 20 C7
FF FF FF FF FF FF FF FF 00 00 00 00 00 00 06 41
00 00 06 A5 00 00 00 65 00 00 00 06 00 00 40 00
00 00 00 00 00 00 00 02 00 00 00 00 00 00 00 06
00 00 03 25 FF F6 94 65 1C 00 00 12 00 00 00 11
00 00 00 11 00 00 00 01 00 00 00 65 00 00 00 06
00 00 40 00 00 00 00 00 00 00 00 02 00 00 00 00
00 00 00 06 00 00 00 6A 00 00 00 02 00 0D 20 C7
FF FF FF FF FF FF FF FF 00 00 00 00 00 00 06 41
00 00 06 A5";
    let data = convert_str_to_bytes(data_str);
    let pdms_database_info = read_attr_info_config("all_attr_info.bin");
    // if let Some(map) = pdms_database_info.noun_attr_info_map.get(&0xCA78Ci32) {
    //     //dbg!(map.value());
    // }
    let mut lookup = StringLookupTable::default();
    let ele_data = parse_ele_data(
        data.as_slice(),
        &pdms_database_info.noun_attr_info_map,
        &mut lookup,
        0,
    )
    .unwrap();
    let mut value = SmolStr::new("");
    //dbg!(&ele_data);
}

#[test]
fn test_atta() {
    // issue : https://gitee.com/happydpc/aios-parse-pdms/issues/I4QDGE
    let data_str = "
00 00 00 33 00 00 5C 20 00 00 15 8E 00 08 A3 E5
00 00 5C 20 00 00 15 8B 00 00 03 0F 00 06 80 01
00 00 00 00 00 00 00 00 20 0A 00 00 00 00 00 03
00 00 00 00 40 B4 F0 00 00 00 00 00 40 C4 50 00
00 00 00 00 40 93 74 00 00 00 00 03 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 08 00 00 3B 59 00 00 37 CA
00 00 3B 58 00 03 80 2D 00 00 00 01 00 00 00 02
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 03 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 0A F8 61 00 00 5C 20 00 00 1B 41
00 00 00 02 29 BA BD 40 80 00 00 01 00 01 00 2D
00 00 5C 20 00 00 15 8E 00 00 00 00 00 00 00 00
00 0A AF CA 14 00 00 01 00 00 00 00 00 09 2E A7
0C 00 00 01 FF FF FF FF 00 0B C6 C0 14 00 00 01
00 00 00 01 06 A0 26 04 0C 00 00 01 00 0D F3 17
00 0B CB FF 08 00 00 02 00 00 00 00 00 00 00 00
10 71 D1 20 08 00 00 02 00 00 00 00 00 00 00 00
10 71 D1 2B 08 00 00 02 00 00 00 00 00 00 00 00
00 0D FD 22 14 00 00 01 00 00 00 00 00 CC 6B 3F
38 00 00 02 00 00 00 01 00 08 A3 E5 00 0D 20 C7
18 00 00 03 00 00 00 01 00 00 00 00 40 39 00 00
0F 7A 2C C8 1C 00 00 02 00 00 00 01 00 09 C5 E1
";
    let data = convert_str_to_bytes(data_str);
    let pdms_database_info = read_attr_info_config("all_attr_info.bin");
    let mut lookup = StringLookupTable::default();
    let ele_data = parse_ele_data(
        data.as_slice(),
        &pdms_database_info.noun_attr_info_map,
        &mut lookup,
        0,
    )
    .unwrap();
    let mut value = SmolStr::new("");
    //dbg!(&ele_data.whole_attmap.merge());
}

//test height
//issue: height is 0
#[test]
fn test_aba_height() {
    // issue : https://gitee.com/happydpc/aios-parse-pdms/issues/I4QDGE
    let data_str = "
00 00 00 11 00 00 40 04 00 00 1C 2C 00 0C C9 49
00 00 40 04 00 00 1C 2B 00 00 00 00 00 00 00 00
00 00 2F 6F 00 0F 60 01 00 00 00 08 45 7A 00 00
00 0E 48 9E 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 02 00 0D 00 00 40 04 00 00 1C 2C
00 00 00 00 00 00 00 00 00 00 40 04 00 00 1C 2D
00 00 40 04 00 00 1C 2E 00 00 40 04 00 00 1C 2F
00 00 40 04 00 00 1C 30 00 00 00 1D 00 00 40 04
00 00 1C 2D 00 09 DB 31 00 00 40 04 00 00 1C 2C
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 03
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00
";
    let data = convert_str_to_bytes(data_str);
    let pdms_database_info = read_attr_info_config("all_attr_info.bin");
    let mut lookup = StringLookupTable::default();
    let ele_data = parse_ele_data(
        data.as_slice(),
        &pdms_database_info.noun_attr_info_map,
        &mut lookup,
        0,
    )
    .unwrap();
    let mut value = SmolStr::new("");
    //dbg!(&ele_data.whole_attmap.merge());
}

//issue: position is zero
#[test]
fn test_aba_positon() {
    //dbg!(db1_dehash(u32::from_be_bytes([0x0D, 0xC3, 0x4A, 0xB5])));
    // //dbg!(db1_dehash(u32::from_be_bytes([0x4D, 0x7C, 0x74, 0xD0])));
    let data_str = "
00 00 00 2F 00 00 20 04 00 00 2A 3A 02 C7 5C 2C
00 00 20 04 00 00 2A 39 00 00 03 02 00 17 80 01
00 00 03 02 00 16 60 01 00 02 00 04 00 09 5B 08
00 00 1A F4 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 01 4D 7C 74 D0 00 00 00 03 C6 0F C0 00
C7 30 2C 00 43 48 00 00 00 00 00 03 00 00 00 00
00 00 00 00 C2 B4 00 00 00 00 00 00 00 02 00 09
00 00 20 04 00 00 2A 3A 00 00 00 00 00 00 00 00
00 00 20 04 00 00 2A 3B 00 00 20 04 00 00 2A 3E
00 01 00 0D 00 00 20 04 00 00 2A 3A 00 00 00 00
00 00 00 00 00 09 C1 8E 3C 00 00 06 00 00 00 13
2F 31 44 42 32 30 30 30 31 48 4D 2F 31 30 33 2F
45 43 48 00
";
    let data = convert_str_to_bytes(data_str);
    let pdms_database_info = read_attr_info_config("all_attr_info.bin");
    let mut lookup = StringLookupTable::default();
    let ele_data = parse_ele_data(
        data.as_slice(),
        &pdms_database_info.noun_attr_info_map,
        &mut lookup,
        0,
    )
    .unwrap();
    let mut value = SmolStr::new("");
    //dbg!(&ele_data.whole_attmap.merge());
}

//issue: position is zero
#[test]
fn test_sample_positon() {
    let data_str = "
00 00 00 1D 00 00 5C 20 00 00 25 DF 00 0E A0 01
00 00 5C 20 00 00 21 7E 00 00 05 47 00 3C A0 01
00 00 05 47 00 38 40 01 20 02 80 1E 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 03 00 00 00 00
40 C7 CC 00 00 00 00 00 40 B2 16 00 00 00 00 00
40 B3 1A 00 00 00 00 03 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 02 00 23 00 00 5C 20 00 00 25 DF
00 00 00 00 00 00 00 00 00 00 5C 20 00 00 25 E0
00 00 5C 20 00 00 25 E1 00 00 5C 20 00 00 25 E2
00 00 5C 20 00 00 25 E3 00 00 5C 20 00 00 26 98
00 00 5C 20 00 00 26 AA 00 00 5C 20 00 00 26 B4
00 00 5C 20 00 00 26 BD 00 00 5C 20 00 00 26 C6
00 00 5C 20 00 00 26 D4 00 00 5C 20 00 00 26 D7
00 00 5C 20 00 00 26 E5 00 00 5C 20 00 00 26 E8
00 00 5C 20 00 00 26 EE 00 00 5C 20 00 00 26 F9
00 01 00 0F 00 00 5C 20 00 00 25 DF 00 00 00 00
00 00 00 00 00 CC 6B 3F 38 00 00 02 00 00 00 01
00 0E A0 01 00 09 C1 8E 3C 00 00 04 00 00 00 0B
2F 53 54 41 49 52 43 2E 54 4F 50 00
";
    let data = convert_str_to_bytes(data_str);
    let pdms_database_info = read_attr_info_config("all_attr_info.bin");
    let mut lookup = StringLookupTable::default();
    let ele_data = parse_ele_data(
        data.as_slice(),
        &pdms_database_info.noun_attr_info_map,
        &mut lookup,
        0,
    )
    .unwrap();
    let mut value = SmolStr::new("");
    //dbg!(&ele_data.whole_attmap.merge());
}
