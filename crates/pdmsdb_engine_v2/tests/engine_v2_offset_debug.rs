use pdmsdb_engine_v2::{EngineOptions, EngineV2, RefNo};

const DB_PATH: &str = r"D:\work\plant-code\pdms-io-fork\test-file\ams1112_0001";

#[test]
fn verify_known_refno_offset() {
    if !std::path::Path::new(DB_PATH).exists() {
        println!("DB not found, skip");
        return;
    }

    let handle = EngineV2::open_read(DB_PATH, EngineOptions::default()).unwrap();
    let refno = RefNo::from_parts(17496, 171138);
    let hit = handle.find_refno(refno, None).unwrap().unwrap();

    println!("page_no: {}", hit.loc.page_no);
    println!("byte_offset: {}", hit.loc.byte_offset);
    println!("ext_no: {}", hit.loc.ext_no);

    let page_size = handle.page_size();
    let file_offset = hit.loc.page_no as u64 * page_size as u64 + hit.loc.byte_offset as u64;
    println!("file_offset: 0x{:X} ({})", file_offset, file_offset);

    let raw = handle.read_record(hit.loc).unwrap();
    println!("record.len: {} bytes", raw.len());

    let first_word = i32::from_be_bytes(raw[0..4].try_into().unwrap());
    println!("first_word (impl_len): {}", first_word);
    assert!(first_word > 0 && first_word < 10000, "impl_len should be reasonable");

    let refno_hi = u32::from_be_bytes(raw[4..8].try_into().unwrap());
    let refno_lo = u32::from_be_bytes(raw[8..12].try_into().unwrap());
    println!("record refno: {}:{}", refno_hi, refno_lo);
    assert_eq!(refno_hi, 17496);
    assert_eq!(refno_lo, 171138);
}

#[test]
fn analyze_failing_refnos() {
    if !std::path::Path::new(DB_PATH).exists() {
        return;
    }

    let handle = EngineV2::open_read(DB_PATH, EngineOptions::default()).unwrap();
    let entries = handle.iter_all_refnos().unwrap();
    println!("total: {} entries", entries.len());

    let page_size = handle.page_size();
    let mut ok = 0;
    let mut fail = 0;

    for entry in &entries {
        let loc = entry.loc;
        let file_offset = loc.page_no as u64 * page_size as u64 + loc.byte_offset as u64;

        match handle.read_record(loc) {
            Ok(raw) => {
                if raw.len() >= 24 {
                    let impl_len = i32::from_be_bytes(raw[0..4].try_into().unwrap());
                    if impl_len > 0 && impl_len < 100000 {
                        ok += 1;
                        continue;
                    }
                }
                println!(
                    "SUSPECT: {}:{} page={} offset={} file=0x{:X} raw_len={} first_4=[{:02X} {:02X} {:02X} {:02X}]",
                    entry.refno.hi(), entry.refno.lo(),
                    loc.page_no, loc.byte_offset, file_offset, raw.len(),
                    raw.get(0).copied().unwrap_or(0),
                    raw.get(1).copied().unwrap_or(0),
                    raw.get(2).copied().unwrap_or(0),
                    raw.get(3).copied().unwrap_or(0),
                );
                fail += 1;
            }
            Err(e) => {
                println!(
                    "FAIL: {}:{} page={} offset={} file=0x{:X} err={:?}",
                    entry.refno.hi(), entry.refno.lo(),
                    loc.page_no, loc.byte_offset, file_offset, e
                );
                fail += 1;
            }
        }
    }

    println!("\nresult: {} ok, {} fail out of {}", ok, fail, entries.len());
}
