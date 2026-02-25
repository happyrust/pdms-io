use crate::parser::attlib::noun_schema::NounSchema;
use crate::parser::attlib::{AttlibData, AttrDataType, AttrDefiType};
use aios_core::tool::db_tool::{db1_dehash, db1_hash};

const ATTLIB_PATH: &str = "D:\\work\\plant-code\\pdms-io-fork\\test-file\\attlib.dat";

fn load_attlib() -> Option<AttlibData> {
    if !std::path::Path::new(ATTLIB_PATH).exists() {
        eprintln!("attlib.dat not found at: {}", ATTLIB_PATH);
        return None;
    }
    Some(AttlibData::parse_attlib_file(ATTLIB_PATH).expect("Failed to parse attlib.dat"))
}

#[test]
fn test_parse_attlib_basic() {
    let Some(data) = load_attlib() else { return };

    println!("解析到 {} 个属性定义", data.attributes.len());
    println!("Noun 映射: {} 个 NOUN", data.noun_attr_map.len());
    println!("属性元数据: {} 个", data.attr_meta_map.len());
    println!("ATNAIN 条目: {} 个", data.noun_attr_entries.len());
    println!("ATGTIX 条目: {} 个", data.atgtix.len());
    println!("ATGTDF 条目: {} 个", data.atgtdf.len());
    println!("ATGTSX 条目: {} 个", data.atgtsx.len());

    assert!(
        data.attributes.len() > 100,
        "应解析到大量属性定义，实际: {}",
        data.attributes.len()
    );
    assert!(!data.noun_attr_map.is_empty(), "noun_attr_map 不应为空");
    assert!(!data.attr_meta_map.is_empty(), "attr_meta_map 不应为空");
}

#[test]
fn test_hash_roundtrip() {
    let names = [
        "ELBO", "PIPE", "TEE", "VALV", "EQUI", "SITE", "BORE", "TYPE", "NAME",
    ];
    for name in &names {
        let hash = db1_hash(name) as u32;
        let decoded = db1_dehash(hash);
        assert_eq!(
            decoded.trim(),
            *name,
            "Hash 往返失败: {} -> 0x{:08X} -> {}",
            name,
            hash,
            decoded
        );
    }
}

#[test]
fn test_noun_schema_elbo() {
    let Some(data) = load_attlib() else { return };

    let schema = NounSchema::from_attlib(&data, "ELBO");
    assert!(schema.is_some(), "ELBO 应存在于 attlib 中");

    let schema = schema.unwrap();
    println!("{}", schema.summary());

    assert!(
        schema.attribute_count() > 0,
        "ELBO 应有属性，实际: {}",
        schema.attribute_count()
    );

    let elbo_hash = db1_hash("ELBO") as u32;
    assert_eq!(schema.noun_hash, elbo_hash);
    assert_eq!(schema.noun_name, "ELBO");
}

#[test]
fn test_noun_schema_pipe() {
    let Some(data) = load_attlib() else { return };

    let schema = NounSchema::from_attlib(&data, "PIPE");
    assert!(schema.is_some(), "PIPE 应存在于 attlib 中");

    let schema = schema.unwrap();
    println!("{}", schema.summary());

    assert!(
        schema.attribute_count() > 0,
        "PIPE 应有属性，实际: {}",
        schema.attribute_count()
    );
}

#[test]
fn test_noun_schema_multiple_nouns() {
    let Some(data) = load_attlib() else { return };

    let nouns = [
        "ELBO", "TEE", "VALV", "EQUI", "NOZZ", "SITE", "ZONE", "PIPE",
    ];
    for noun in &nouns {
        match NounSchema::from_attlib(&data, noun) {
            Some(schema) => {
                println!(
                    "{}: {} 个属性 (hash=0x{:08X})",
                    noun,
                    schema.attribute_count(),
                    schema.noun_hash
                );
            }
            None => {
                println!("{}: 未找到 (hash=0x{:08X})", noun, db1_hash(noun) as u32);
            }
        }
    }
}

#[test]
fn test_noun_schema_filter_by_type() {
    let Some(data) = load_attlib() else { return };
    let Some(schema) = NounSchema::from_attlib(&data, "ELBO") else {
        return;
    };

    let int_attrs = schema.filter_by_type(AttrDataType::Integer);
    let real_attrs = schema.filter_by_type(AttrDataType::Real);
    let text_attrs = schema.filter_by_type(AttrDataType::Text);
    let ref_attrs = schema.filter_by_type(AttrDataType::Reference);

    println!("ELBO 属性按类型分布:");
    println!("  Integer: {}", int_attrs.len());
    println!("  Real:    {}", real_attrs.len());
    println!("  Text:    {}", text_attrs.len());
    println!("  Ref:     {}", ref_attrs.len());
}

#[test]
fn test_noun_schema_filter_by_defi() {
    let Some(data) = load_attlib() else { return };
    let Some(schema) = NounSchema::from_attlib(&data, "ELBO") else {
        return;
    };

    let dab_attrs = schema.filter_by_defi(AttrDefiType::Dab);
    let pseudo_attrs = schema.filter_by_defi(AttrDefiType::Pseudo);

    println!("ELBO 属性按存储方式:");
    println!("  DAB:    {}", dab_attrs.len());
    println!("  Pseudo: {}", pseudo_attrs.len());
}

#[test]
fn test_list_nouns() {
    let Some(data) = load_attlib() else { return };

    let nouns = data.list_nouns();
    println!("共 {} 个 NOUN:", nouns.len());
    assert!(!nouns.is_empty(), "应至少有一个 NOUN");

    // 打印前 20 个
    let mut sorted: Vec<_> = nouns.iter().collect();
    sorted.sort_by_key(|(_, name)| name.clone());
    for (hash, name) in sorted.iter().take(20) {
        println!("  {} (0x{:08X})", name, hash);
    }
}

#[test]
fn test_attr_meta_map_consistency() {
    let Some(data) = load_attlib() else { return };

    // 验证 attr_meta_map 中的每个条目都有合法的 hash
    for (hash, meta) in &data.attr_meta_map {
        assert_eq!(
            *hash, meta.hash,
            "hash 不匹配: {} -> 0x{:08X} vs 0x{:08X}",
            meta.name, hash, meta.hash
        );
        assert!(!meta.name.is_empty(), "属性名不应为空");

        let computed_hash = db1_hash(&meta.name) as u32;
        assert_eq!(
            *hash, computed_hash,
            "属性 {} 的 hash 计算不一致: 存储=0x{:08X} 计算=0x{:08X}",
            meta.name, hash, computed_hash
        );
    }
}
