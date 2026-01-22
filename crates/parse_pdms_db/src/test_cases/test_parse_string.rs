use crate::parse::parse_ele_data;
use crate::test_cases::convert_str_to_bytes;
use aios_core::get_default_pdms_db_info;

//13244/142148
#[tokio::test]
async fn test_ams_pcon() {
    let data_str = "
00 00 00 21 00 00 33 BC 00 02 2B 44 00 0F 56 3E
00 00 33 BC 00 02 2B 42 00 00 47 0E 00 2E 80 01
00 00 00 00 00 00 00 00 20 05 00 00 00 00 00 02
00 00 00 04 00 00 00 28 00 00 00 07 00 00 02 22
00 00 02 32 00 00 00 04 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 00 00 00 04 00 00 00 00
00 00 00 01 00 00 00 00 00 00 00 00 00 00 00 02
00 00 00 02 00 00 00 01 00 00 00 00 00 00 00 00
00 00 00 00 00 01 00 19 00 00 33 BC 00 02 2B 44
00 00 00 00 00 00 00 00 FF F2 51 1C 1C 00 00 12
00 00 00 11 00 00 00 11 00 00 00 01 00 00 00 65
00 00 00 06 00 10 00 00 00 00 00 00 40 00 03 FF
00 00 00 00 00 00 00 06 00 00 00 6A 00 00 00 02
00 0D 20 C7 FF FF FF FF FF FF FF FF 00 00 00 00
00 00 06 41 00 00 06 A5 00 ";
    let data = convert_str_to_bytes(data_str);
    let _pdms_database_info = get_default_pdms_db_info();
    let ele_data = parse_ele_data(data.as_slice()).await.unwrap();
    dbg!(&ele_data);
    if let Some(val) = ele_data.whole_attmap.attmap.get_val("PCON") {
        let _result = val.string_value();
    }
}
