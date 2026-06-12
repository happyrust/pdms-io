use std::collections::{BTreeMap, BTreeSet};

use aios_core::pdms_types::DbAttributeType;
use aios_core::types::AttrVal;
use aios_core::{get_default_pdms_db_info, tool::db_tool::db1_dehash};

#[derive(Debug, Default)]
struct IntVecAttrHit {
    attr_name: String,
    attr_hash: i32,
    att_type_intvec_count: usize,
    default_int_array_count: usize,
    att_types: BTreeSet<String>,
    default_lengths: BTreeSet<usize>,
    nouns: BTreeSet<String>,
}

fn main() {
    let db_info = get_default_pdms_db_info();
    let mut hits: BTreeMap<String, IntVecAttrHit> = BTreeMap::new();

    for noun_entry in db_info.noun_attr_info_map.iter() {
        let noun_name = db1_dehash(*noun_entry.key() as u32);
        for attr_entry in noun_entry.value().iter() {
            let info = attr_entry.value();
            let attr_name = if info.name.is_empty() {
                db1_dehash(info.hash as u32)
            } else {
                info.name.clone()
            };
            let default_len = match &info.default_val {
                AttrVal::IntArrayType(values) => Some(values.len()),
                _ => None,
            };
            let has_intvec_type = matches!(info.att_type, DbAttributeType::INTVEC);
            if !has_intvec_type && default_len.is_none() {
                continue;
            }

            let hit = hits
                .entry(attr_name.clone())
                .or_insert_with(|| IntVecAttrHit {
                    attr_name,
                    attr_hash: info.hash,
                    ..Default::default()
                });
            if has_intvec_type {
                hit.att_type_intvec_count += 1;
            }
            hit.att_types.insert(format!("{:?}", info.att_type));
            if let Some(len) = default_len {
                hit.default_int_array_count += 1;
                hit.default_lengths.insert(len);
            }
            hit.nouns.insert(noun_name.clone());
        }
    }

    println!(
        "attr,hash,att_type_intvec_count,default_int_array_count,att_types,default_lengths,noun_count,nouns"
    );
    for hit in hits.values() {
        println!(
            "{},{:#X},{},{},{:?},{:?},{},{}",
            hit.attr_name,
            hit.attr_hash,
            hit.att_type_intvec_count,
            hit.default_int_array_count,
            hit.att_types,
            hit.default_lengths,
            hit.nouns.len(),
            hit.nouns.iter().cloned().collect::<Vec<_>>().join("|")
        );
    }

    let suspicious = hits
        .values()
        .filter(|hit| !matches!(hit.attr_name.as_str(), "LEVE" | "PTS"))
        .collect::<Vec<_>>();
    println!();
    println!("suspicious_non_leve_pts_count={}", suspicious.len());
    for hit in suspicious {
        println!(
            "SUSPICIOUS {}, hash={:#X}, att_type_intvec_count={}, default_int_array_count={}, att_types={:?}, default_lengths={:?}, nouns={}",
            hit.attr_name,
            hit.attr_hash,
            hit.att_type_intvec_count,
            hit.default_int_array_count,
            hit.att_types,
            hit.default_lengths,
            hit.nouns.iter().cloned().collect::<Vec<_>>().join("|")
        );
    }
}
