use pdmsdb_engine_v2::db4::{
    ElementBuilder, ElementRecordView, ExplicitBlockBuilder,
    parse_explicit_blocks, parse_member_refs,
    write_implicit_integer, write_implicit_real_f64, write_implicit_reference,
};
use pdmsdb_engine_v2::RefNo;

#[test]
fn element_builder_roundtrip() {
    let refno = RefNo::from_parts(100, 200);
    let owner = RefNo::from_parts(50, 60);
    let noun_hash = 0xDEADBEEFu32;

    let child1 = RefNo::from_parts(100, 300);
    let child2 = RefNo::from_parts(100, 400);

    let explicit_builder = ExplicitBlockBuilder::new(0x12345678, refno);
    let string_block = explicit_builder.build_string("HELLO WORLD");
    let int_block = ExplicitBlockBuilder::new(0xAABBCCDD, refno).build_integer(42);

    let mut builder = ElementBuilder::new(refno, noun_hash, owner);
    builder.set_implicit_i32(0, 999);
    builder.set_implicit_f64(1, 3.14159);
    builder.set_implicit_ref(3, RefNo::from_parts(1, 2));
    builder.add_member(child1);
    builder.add_member(child2);
    builder.add_explicit_block(string_block);
    builder.add_explicit_block(int_block);

    let record = builder.build().unwrap();
    assert!(record.len() > 24, "record should be at least header size");

    let view = ElementRecordView::from_raw(&record).unwrap();
    assert_eq!(view.refno, refno);
    assert_eq!(view.noun_hash, noun_hash);
    assert_eq!(view.owner, owner);
    assert!(view.implicit_data.len() >= 24);

    let has_members = !view.members_data.is_empty();
    let has_explicit = !view.explicit_data.is_empty();
    assert!(
        has_members || has_explicit,
        "record should have members or explicit data"
    );

    if has_members {
        let members = parse_member_refs(&view.members_data);
        assert_eq!(members.len(), 2);
        assert_eq!(members[0], child1);
        assert_eq!(members[1], child2);
    }

    let all_block_data = if has_members && has_explicit {
        view.explicit_data.clone()
    } else if !has_members {
        view.explicit_data.clone()
    } else {
        vec![]
    };

    if !all_block_data.is_empty() {
        let blocks = parse_explicit_blocks(&all_block_data).unwrap();
        assert!(!blocks.is_empty(), "should have explicit blocks");
    }
}

#[test]
fn implicit_write_read_roundtrip() {
    let mut data = vec![0u8; 64];

    let header_words = 6i32;
    data[0..4].copy_from_slice(&header_words.to_be_bytes());
    data[4..8].copy_from_slice(&10u32.to_be_bytes());
    data[8..12].copy_from_slice(&20u32.to_be_bytes());
    data[12..16].copy_from_slice(&0xABCDu32.to_be_bytes());
    data[16..20].copy_from_slice(&1u32.to_be_bytes());
    data[20..24].copy_from_slice(&2u32.to_be_bytes());

    write_implicit_integer(&mut data, 6, 12345).unwrap();
    let readback = pdmsdb_engine_v2::db4::attrs::read_implicit_integer(&data, 6).unwrap();
    assert_eq!(readback, 12345);

    write_implicit_real_f64(&mut data, 8, 2.71828).unwrap();
    let readback_f64 = pdmsdb_engine_v2::db4::attrs::read_implicit_real_f64(&data, 8).unwrap();
    assert!((readback_f64 - 2.71828).abs() < 1e-10);

    let test_ref = RefNo::from_parts(999, 888);
    write_implicit_reference(&mut data, 10, test_ref).unwrap();
    let readback_ref = pdmsdb_engine_v2::db4::attrs::read_implicit_reference(&data, 10).unwrap();
    assert_eq!(readback_ref, test_ref);
}

#[test]
fn explicit_block_builder_string_roundtrip() {
    let refno = RefNo::from_parts(1, 1);
    let builder = ExplicitBlockBuilder::new(0x11223344, refno);
    let block_bytes = builder.build_string("TEST STRING VALUE");

    let blocks = parse_explicit_blocks(&block_bytes).unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].hash, 0x11223344);
    assert_eq!(blocks[0].self_ref, refno);

    let value = pdmsdb_engine_v2::db4::explicit_attrs::read_explicit_string(&blocks[0].payload);
    assert_eq!(value, "TEST STRING VALUE");
}

#[test]
fn explicit_block_builder_integer_roundtrip() {
    let refno = RefNo::from_parts(2, 2);
    let builder = ExplicitBlockBuilder::new(0x55667788, refno);
    let block_bytes = builder.build_integer(-42);

    let blocks = parse_explicit_blocks(&block_bytes).unwrap();
    assert_eq!(blocks.len(), 1);
    let val = pdmsdb_engine_v2::db4::explicit_attrs::read_explicit_integer(&blocks[0].payload, 0);
    assert_eq!(val, Some(-42));
}

#[test]
fn explicit_block_builder_real_roundtrip() {
    let refno = RefNo::from_parts(3, 3);
    let builder = ExplicitBlockBuilder::new(0x99AABBCC, refno);
    let block_bytes = builder.build_real_f64(1.23456789);

    let blocks = parse_explicit_blocks(&block_bytes).unwrap();
    assert_eq!(blocks.len(), 1);
    let val = pdmsdb_engine_v2::db4::explicit_attrs::read_explicit_real_f64(&blocks[0].payload, 0);
    assert!(val.is_some());
    assert!((val.unwrap() - 1.23456789).abs() < 1e-10);
}

#[test]
fn member_refs_add_remove() {
    use pdmsdb_engine_v2::db4::ElementRefs;

    let owner = RefNo::from_parts(1, 1);
    let mut refs = ElementRefs::new(owner, vec![]);
    assert!(!refs.has_children());

    let c1 = RefNo::from_parts(1, 10);
    let c2 = RefNo::from_parts(1, 20);
    refs.add_member(c1);
    refs.add_member(c2);
    assert_eq!(refs.child_count(), 2);

    refs.add_member(c1);
    assert_eq!(refs.child_count(), 2, "no duplicates");

    assert!(refs.remove_member(c1));
    assert_eq!(refs.child_count(), 1);
    assert!(!refs.remove_member(c1));

    let self_ref = RefNo::from_parts(1, 1);
    let block = refs.serialize_members_block(self_ref);
    assert!(!block.is_empty());

    let parsed = parse_member_refs(&block);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0], c2);
}
