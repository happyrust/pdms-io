use crate::parse::parse_ele_children;
use aios_core::RefU64Vec;

fn build_children_block(refno0: u32, refno1: u32, children: &[u64]) -> Vec<u8> {
    // impl_len = 6 (words) => 24 bytes隐含，覆盖 refno/type_hash/owner
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(6i32.to_be_bytes())); // impl len words
    bytes.extend_from_slice(&refno0.to_be_bytes());
    bytes.extend_from_slice(&refno1.to_be_bytes());
    bytes.extend_from_slice(&0x12345678u32.to_be_bytes()); // type_hash
    bytes.extend_from_slice(&0u64.to_be_bytes()); // owner placeholder
    // padding to actual_impl_len = 16 bytes already (no extra 0/7)
    // members flag 0x0002
    bytes.extend_from_slice(&[0x00, 0x02]);
    // len words = header(5 words=20 bytes) + 每个子节点 2 words
    let words = (5 + children.len() * 2) as u16; // each child is 8 bytes (2 words)
    bytes.extend_from_slice(&words.to_be_bytes());
    // self refno
    bytes.extend_from_slice(&refno0.to_be_bytes());
    bytes.extend_from_slice(&refno1.to_be_bytes());
    // 8 字节填充，确保成员数据从 offset 20 开始
    bytes.extend_from_slice(&0u64.to_be_bytes());
    // children
    for c in children {
        bytes.extend_from_slice(&c.to_be_bytes());
    }
    bytes
}

#[test]
fn parse_children_happy_path() {
    let data = build_children_block(1, 2, &[3u64, 4u64]);
    // 确认测试数据布局符合预期，便于解析器调试
    assert_eq!(data.len(), 60);
    let impl_len_bytes = i32::from_be_bytes(data[0..4].try_into().unwrap()) as usize * 4;
    assert_eq!(impl_len_bytes, 24);
    let (refno, children) = parse_ele_children(&data);
    assert_eq!(refno.get_0(), 1);
    assert_eq!(refno.get_1(), 2);
    let expected = RefU64Vec(vec![3u64.into(), 4u64.into()]);
    assert_eq!(children.len(), expected.len());
    assert_eq!(children.as_slice(), expected.as_slice());
}

#[test]
fn parse_children_bad_len() {
    let data = vec![0u8; 8]; // too short
    let (refno, children) = parse_ele_children(&data);
    assert_eq!(refno, Default::default());
    assert!(children.is_empty());
}
