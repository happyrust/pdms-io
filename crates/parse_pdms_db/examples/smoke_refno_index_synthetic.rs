use aios_core::RefU64;
use anyhow::{Result, bail};
use parse_pdms_db::refno_index::{find_refno_entry, gen_ref_type_pos_table_from_index};
use serde::Serialize;

const PAGE_SIZE: usize = 2048;
const LATEST_SESSION_PGNO_OFFSET: usize = 0x28;
const SESSION_PGNO: u32 = 1;
const INDEX_ROOT_PGNO: u32 = 2;
const ELEMENT_PGNO: u32 = 3;
const ELEMENT_OFFSET_WORDS: u32 = 8;
const TARGET_NOUN_HASH: i32 = 0x123456;
const WORLD_NOUN_HASH: i32 = 0x0BEB83;

#[derive(Debug, Serialize)]
struct SmokeResult {
    target_refno: String,
    single_lookup_pos: usize,
    single_lookup_noun_hash: i32,
    table_lookup_pos: usize,
    table_lookup_noun_hash: i32,
    world_refno: String,
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn put_i32(bytes: &mut [u8], offset: usize, value: i32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_index_entry(bytes: &mut [u8], offset: usize, refno: RefU64, pgno: u32, offset_words: u32) {
    put_u32(bytes, offset, refno.get_0());
    put_u32(bytes, offset + 4, refno.get_1());
    put_u32(bytes, offset + 8, pgno);
    put_u32(bytes, offset + 12, offset_words << 12);
}

fn write_element_record(bytes: &mut [u8], offset: usize, refno: RefU64, noun_hash: i32) {
    put_u32(bytes, offset, 8);
    put_u32(bytes, offset + 4, refno.get_0());
    put_u32(bytes, offset + 8, refno.get_1());
    put_i32(bytes, offset + 12, noun_hash);
}

fn build_synthetic_db() -> (Vec<u8>, RefU64, RefU64) {
    let mut bytes = vec![0u8; PAGE_SIZE * 4];
    let target_refno = RefU64::from_two_nums(13246, 243899);
    let world_refno = RefU64::from_two_nums(13246, 1);

    put_u32(&mut bytes, LATEST_SESSION_PGNO_OFFSET, SESSION_PGNO);

    let session_offset = SESSION_PGNO as usize * PAGE_SIZE;
    put_i32(&mut bytes, session_offset, 3);
    put_u32(&mut bytes, session_offset + 0x1C, INDEX_ROOT_PGNO);

    let index_offset = INDEX_ROOT_PGNO as usize * PAGE_SIZE;
    put_i32(&mut bytes, index_offset, 1);
    put_i32(&mut bytes, index_offset + 4, 0x00CC_47DF);
    put_u32(&mut bytes, index_offset + 8, 0);

    let entry_offset = index_offset + 0x1C;
    write_index_entry(
        &mut bytes,
        entry_offset,
        world_refno,
        ELEMENT_PGNO,
        ELEMENT_OFFSET_WORDS + 16,
    );
    write_index_entry(
        &mut bytes,
        entry_offset + 16,
        target_refno,
        ELEMENT_PGNO,
        ELEMENT_OFFSET_WORDS,
    );

    let target_record_offset =
        ELEMENT_PGNO as usize * PAGE_SIZE + ELEMENT_OFFSET_WORDS as usize * 2;
    let world_record_offset =
        ELEMENT_PGNO as usize * PAGE_SIZE + (ELEMENT_OFFSET_WORDS as usize + 16) * 2;
    write_element_record(
        &mut bytes,
        target_record_offset,
        target_refno,
        TARGET_NOUN_HASH,
    );
    write_element_record(
        &mut bytes,
        world_record_offset,
        world_refno,
        WORLD_NOUN_HASH,
    );

    (bytes, target_refno, world_refno)
}

fn main() -> Result<()> {
    let (bytes, target_refno, expected_world_refno) = build_synthetic_db();

    let Some(single_entry) = find_refno_entry(&bytes, target_refno) else {
        bail!("single refno lookup failed");
    };
    let Some((table, world_refno)) = gen_ref_type_pos_table_from_index(&bytes) else {
        bail!("index table build failed");
    };
    let Some(table_entry) = table.get(&target_refno) else {
        bail!("target refno missing from index table");
    };
    if world_refno != expected_world_refno {
        bail!(
            "world refno mismatch: got {}, expected {}",
            world_refno.to_e3d_id(),
            expected_world_refno.to_e3d_id()
        );
    }
    if single_entry.pos != table_entry.pos || single_entry.noun_hash != table_entry.noun_hash {
        bail!("single lookup and table lookup disagree");
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&SmokeResult {
            target_refno: target_refno.to_e3d_id(),
            single_lookup_pos: single_entry.pos,
            single_lookup_noun_hash: single_entry.noun_hash,
            table_lookup_pos: table_entry.pos,
            table_lookup_noun_hash: table_entry.noun_hash,
            world_refno: world_refno.to_e3d_id(),
        })?
    );

    Ok(())
}
