use aios_core::RefU64;

use crate::parse::convert_to_explicit_axis_string;

fn be_u32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

#[test]
fn test_convert_to_explicit_axis_string_truncated_to_dir_does_not_panic() {
    // 输入恰好 5 个 u32（20 bytes），tuple 解析后 tmp_input 为空；
    // 旧实现会在 [0x2,0x34] 分支里对 tmp_input[4..8] 切片而 panic。
    let mut input = Vec::new();
    input.extend_from_slice(&be_u32(0));
    input.extend_from_slice(&be_u32(0));
    input.extend_from_slice(&be_u32(0));
    input.extend_from_slice(&be_u32(0x2));
    input.extend_from_slice(&be_u32(0x34));

    let _ = convert_to_explicit_axis_string(&input, RefU64(0)).unwrap();
}

#[test]
fn test_convert_to_explicit_axis_string_truncated_default_branch_does_not_panic() {
    // 触发 _ 分支且 tmp_input 为空；旧实现会对 &tmp_input[..8] 切片而 panic。
    let mut input = Vec::new();
    input.extend_from_slice(&be_u32(0));
    input.extend_from_slice(&be_u32(0));
    input.extend_from_slice(&be_u32(0));
    input.extend_from_slice(&be_u32(0xDEAD_BEEF));
    input.extend_from_slice(&be_u32(0xFEED_FACE));

    let _ = convert_to_explicit_axis_string(&input, RefU64(0)).unwrap();
}

#[test]
fn test_parse_raw_explicit_attrs_desync_at_0x2c_does_not_panic_or_eof() {
    use crate::parse::parse_raw_explicit_attrs;
    use dashmap::DashMap;

    // 复现 refno=15192/187689 的错位场景：两条合法显式属性(TYPEX + NAME)之后，
    // 偏移 0x2C 处其实是引用/refno 数据(0x0001002E / 0x00003B58 = DB 号 15192)。
    // 旧实现会把它误当 len=0x3B58(15192 word)的超长属性头而盲跳 ~60KB，游标错位后
    // be_*() 读到块尾(剩 0 字节)触发 nom Eof，并把整条记录的显式属性全部清空。
    // 加“止血闸门(type_code 非法 + 未知属性即停)”后应在 0x2C 干净停止：不 panic、返回 Ok。
    let mut data: Vec<u8> = Vec::new();
    // 0x00 TYPEX 头: hash=0x00CC6B3F, type=0x3800, len=2 word
    data.extend_from_slice(&[0x00, 0xCC, 0x6B, 0x3F, 0x38, 0x00, 0x00, 0x02]);
    // 0x08 TYPEX payload(2 word): count=1, typex=5
    data.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x05]);
    // 0x10 STRING(NAME) 头: hash=0x0009C18E, type=0x3C00, len=5 word
    data.extend_from_slice(&[0x00, 0x09, 0xC1, 0x8E, 0x3C, 0x00, 0x00, 0x05]);
    // 0x18 STRING payload(5 word=20B): strlen=13 + "/BFVE5X0-DTSE" + pad
    data.extend_from_slice(&[0x00, 0x00, 0x00, 0x0D]);
    data.extend_from_slice(b"/BFVE5X0-DTSE");
    data.extend_from_slice(&[0x00, 0x00, 0x00]);
    // 0x2C 伪属性头(实为引用数据): hash=0x0001002E, type=0x0000, len=0x3B58
    data.extend_from_slice(&[0x00, 0x01, 0x00, 0x2E, 0x00, 0x00, 0x3B, 0x58]);
    // 0x34 配对元素号 + 尾部填充
    data.extend_from_slice(&[0x00, 0x00, 0xBF, 0x19, 0x00, 0x00, 0x00, 0x00]);

    let attr_info_map = DashMap::new();
    let refno = RefU64::from_two_nums(15192, 187689);

    // 关键：不 panic，且不再返回 nom Eof(Err)。
    let (_, attrs) = parse_raw_explicit_attrs(&data, &attr_info_map, refno)
        .expect("解析应在 0x2C 干净停止并返回 Ok，而非抛出 nom Eof");
    // 0x2C 的引用数据不应被当成属性头吞掉后续字节；解析出的属性数应 <= 2。
    assert!(
        attrs.len() <= 2,
        "0x2C 处引用数据不应被误当作属性头, attrs.len()={}",
        attrs.len()
    );
}
