use std::fs::File;
use std::io::Read;
use aios_core::pdms_types::{AiosStr, AttrMap, Integer, PdmsTree, RefI32Tuple, RefnoInfo, RefU64, RefU64Vec, StringLookupTable };
use aios_core::tool::db_tool::db1_hash;
use id_tree::Tree;
use skytable::actions::Actions;
use smol_str::SmolStr;
use crate::consts::ATT_ROOM;
use crate::parse::RoomCode;
// use crate::pdms_types::{AiosStr, AiosStrHash, Integer, PdmsTree, RefI32Tuple, RefU64Vec, StringLookupTable};
// use crate::{AttrMap, db1_hash, RefU64};

fn decode_json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    serde_json::from_slice::<T>(bytes).ok()
}

#[test]
fn query_room_code() {
    let db = sled::open("AIOS_DBS/APS/room.sled").expect("not found");
    let string_db = sled::open("AIOS_DBS/APS/names.sled").expect("not found");
    let vals = db.iter();
    for val in vals {
        if let Ok((_k,v)) = val {
            let Some(v) = decode_json::<RoomCode>(&v.to_vec()) else { continue; };
            let name_bytes = string_db.get(v.name_hash.to_be_bytes()).unwrap().unwrap().to_vec();
            let Some(room_name) = decode_json::<AiosStr>(&name_bytes) else { continue; };
            println!("refno={:?} room={:?}",v.refno.to_refno_str(),room_name);
        }
    }
}

#[test]
fn query_attr() {
    let db = sled::open("AIOS_DBS/SCB/attr.sled").expect("not found");
    let refno : RefU64 = RefI32Tuple((14195,6)).into();
    let val = db.get(refno.to_be_bytes()).unwrap().unwrap();
    let Some(val) = decode_json::<AttrMap>(&val.to_vec()) else { return; };
    println!("val={:?}",val.to_string_hashmap());
}

#[test]
fn test_tree() {
    let mut file = File::open("ssc_sample_250204.bin").unwrap();
    let mut buf = vec![];
    file.read_to_end(&mut buf).unwrap();
    let Some(tree) = decode_json::<PdmsTree>(&buf) else { return; };
    let tree_id = tree.0.root_node_id().unwrap();
    let children = tree.0.children(tree_id).unwrap();
    for child in children {
        println!("val={:?}",child.data());
    }
}

#[test]
fn test_name_hash() {
    let mut file = File::open("StringLookupTable_250204.bin").unwrap();
    let mut buf = vec![];
    file.read_to_end(&mut buf).unwrap();
    let Some(string_map) = decode_json::<StringLookupTable>(&buf) else { return; };
    if let Some(val) = string_map.lookup.get(&3696643365){
        println!("v={:?}",val.value());
    };
}

#[test]
fn test_project_type_refnos() {
    let db = sled::open("AIOS_DBS/APS/type_eles.sled").expect("not found");
    let attr_db = sled::open("AIOS_DBS/APS/attr.sled").expect("not found");

    if let Ok(Some(val)) = db.get(db1_hash("DB").to_be_bytes()) {
        let Some(refnos) = decode_json::<RefU64Vec>(&val.to_vec()) else { return; };
        for refno in refnos {
            if let Ok(Some(attr)) = attr_db.get(&refno.to_be_bytes()){
                let Some(attr) = decode_json::<AttrMap>(&attr.to_vec()) else { continue; };
                println!("attr={:?}",attr.to_string_hashmap());
            }
        }
    }
}

#[test]
fn test_aios_hash() {
    let aios_str = AiosStr(SmolStr::new("SCB"));
    println!("r={}",aios_str.get_u32_hash());
}
