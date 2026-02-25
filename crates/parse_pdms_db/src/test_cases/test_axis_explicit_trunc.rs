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
