use aios_core::RefU64;
use pdms_io::io::PdmsIO;
use parse_pdms_db::parse::gen_ref_type_pos_table;
use std::collections::BTreeMap;
use std::path::PathBuf;

const CATALOGUE_DB_PATH: &str = "/Volumes/DPC/work/e3d_models/AvevaCatalogue/acp000/acp7021_0001";
const PHEI_HASH: i32 = 0xFFF5_20EFu32 as i32;
const MISATTRIBUTED_PHEI_REFNO: &str = "15213/81343";
const PHEI_REFNO: &str = "15213/80836";

/// Local regression/debug test for the ACP7021 PHEI expression payload truncation
/// seen in `/Volumes/DPC/parser-error/parse-1.log`.
///
/// Run manually:
/// `cargo test --test aps7201_expression_debug test_parse_aps7201_phei_expression_debug -- --ignored --nocapture`
#[tokio::test]
#[ignore]
async fn test_parse_aps7201_phei_expression_debug() -> anyhow::Result<()> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let db_option_file = manifest_dir.join("DbOption");
    unsafe {
        std::env::set_var("DB_OPTION_FILE", &db_option_file);
    }

    let path = PathBuf::from(CATALOGUE_DB_PATH);
    assert!(path.exists(), "数据库文件不存在: {}", path.display());

    let mut io = PdmsIO::new("acp", &path, true);
    io.open()?;

    let misattributed_refno: RefU64 = MISATTRIBUTED_PHEI_REFNO.into();
    let (sesno, offset) = io
        .search_latest_refno(misattributed_refno, None)
        .unwrap_or_else(|| panic!("未解析到目标 refno: {misattributed_refno}"));
    println!("{misattributed_refno} latest sesno={sesno}, offset=0x{offset:X}");
    let record = io.read_element_record_cached(offset)?;
    assert!(
        !record
            .windows(4)
            .any(|window| window == PHEI_HASH.to_be_bytes().as_slice()),
        "{misattributed_refno} 的真实 latest record 不应包含 PHEI；若失败说明又读入了相邻记录"
    );
    let ele_data = io.auto_get_element(misattributed_refno).await?;
    let merged = ele_data.whole_attmap.merge();
    assert!(
        merged.get_as_string("PHEI").is_none(),
        "{misattributed_refno} 不应从相邻元素误归属出 PHEI"
    );

    let refno: RefU64 = PHEI_REFNO.into();
    if let Some((sesno, offset)) = io.search_latest_refno(refno, None) {
        println!("{refno} latest sesno={sesno}, offset=0x{offset:X}");
        let record = io.read_element_record_cached(offset)?;
        dump_phei_entry_header(refno, &record);
        dump_phei_history_entry_headers(&mut io, refno)?;
        dump_full_scan_entry(&io, refno)?;
    } else {
        dump_refno_diagnostics(&mut io, refno)?;
        panic!("未解析到目标 refno: {refno}");
    }

    let ele_data = io.auto_get_element(refno).await?;
    let merged = ele_data.whole_attmap.merge();
    let phei = merged
        .get_as_string("PHEI")
        .unwrap_or_else(|| panic!("{refno} 缺少 PHEI，可能在显式表达式解析处提前退出"));

    assert!(
        !phei.trim().is_empty(),
        "{refno} 的 PHEI 为空，表达式 payload 可能被截断或错位"
    );

    println!("{refno} PHEI = {phei}");

    Ok(())
}

fn dump_full_scan_entry(io: &PdmsIO, refno: RefU64) -> anyhow::Result<()> {
    let file_bytes = std::fs::read(&io.file_path)?;
    let (refno_table_map, _world_refno) = gen_ref_type_pos_table(&file_bytes);
    let Some(entry) = refno_table_map.get(&refno) else {
        println!("{refno} full_scan entry not found");
        return Ok(());
    };

    println!(
        "{refno} full_scan pos=0x{:X}, noun_hash=0x{:X}",
        entry.pos, entry.noun_hash
    );
    if entry.pos >= 4 && entry.pos - 4 < file_bytes.len() {
        dump_phei_entry_header(refno, &file_bytes[entry.pos - 4..]);
    }
    Ok(())
}

fn dump_phei_history_entry_headers(io: &mut PdmsIO, refno: RefU64) -> anyhow::Result<()> {
    let history = io.search_history_refnos(refno, None)?;
    println!("{refno} history_versions={}", history.len());
    for (sesno, offset) in history {
        let record = io.read_element_record_cached(offset)?;
        let hash_bytes = PHEI_HASH.to_be_bytes();
        let has_phei = record.windows(4).any(|w| w == hash_bytes.as_slice());
        println!(
            "{refno} history sesno={sesno}, offset=0x{offset:X}, len={}, has_phei={has_phei}",
            record.len()
        );
        if has_phei {
            dump_phei_entry_header(refno, &record);
        }
    }
    Ok(())
}

fn dump_refno_diagnostics(io: &mut PdmsIO, refno: RefU64) -> anyhow::Result<()> {
    let basic_info = io.get_page_basic_info()?;
    println!(
        "latest_sesno={}, latest_index_root_pgno=0x{:X}, page_size={}",
        basic_info.latest_ses_data.sesno,
        basic_info.latest_ses_data.index_root_pageno,
        io.page_size
    );

    let index_map = io.build_index_map()?;
    let same_dbnum = index_map
        .keys()
        .filter(|key| key.get_0() == refno.get_0())
        .take(10)
        .copied()
        .collect::<Vec<_>>();
    println!(
        "index_map.len()={}, target_dbnum={}, same_dbnum_first_10={:?}",
        index_map.len(),
        refno.get_0(),
        same_dbnum
    );

    let same_element_id = index_map
        .keys()
        .filter(|key| key.get_1() == refno.get_1())
        .take(20)
        .copied()
        .collect::<Vec<_>>();
    println!(
        "target_element_id={}, same_element_id_first_20={:?}",
        refno.get_1(),
        same_element_id
    );

    let mut dbnum_counts = BTreeMap::<u32, usize>::new();
    for key in index_map.keys() {
        *dbnum_counts.entry(key.get_0()).or_default() += 1;
    }
    let dbnum_counts = dbnum_counts.into_iter().take(20).collect::<Vec<_>>();
    println!("dbnum_counts_first_20={dbnum_counts:?}");

    let hash_bytes = PHEI_HASH.to_be_bytes();
    let mut phei_candidates = Vec::new();
    for (key, offsets) in &index_map {
        let Some(offset) = offsets.last().copied() else {
            continue;
        };
        let Ok(record) = io.read_element_record_cached(offset) else {
            continue;
        };
        if record.windows(4).any(|w| w == hash_bytes.as_slice()) {
            phei_candidates.push((*key, offset));
            if phei_candidates.len() >= 20 {
                break;
            }
        }
    }
    println!("phei_candidates_first_20={phei_candidates:?}");

    let file_bytes = std::fs::read(&io.file_path)?;
    let be_hits = find_all_windows(&file_bytes, &PHEI_HASH.to_be_bytes(), 20);
    let le_hits = find_all_windows(&file_bytes, &PHEI_HASH.to_le_bytes(), 20);
    println!("whole_file_phei_be_hits_first_20={be_hits:?}");
    println!("whole_file_phei_le_hits_first_20={le_hits:?}");

    Ok(())
}

fn find_all_windows(haystack: &[u8], needle: &[u8], limit: usize) -> Vec<usize> {
    haystack
        .windows(needle.len())
        .enumerate()
        .filter_map(|(pos, window)| (window == needle).then_some(pos))
        .take(limit)
        .collect()
}

fn dump_phei_entry_header(refno: RefU64, record: &[u8]) {
    let hash_bytes = PHEI_HASH.to_be_bytes();
    let Some(pos) = record.windows(4).position(|w| w == hash_bytes.as_slice()) else {
        println!("{refno} raw record 中未找到 PHEI hash");
        return;
    };

    if pos + 8 > record.len() {
        println!("{refno} PHEI hash 位于记录尾部，缺少 packed header");
        return;
    }

    let packed = u32::from_be_bytes(record[pos + 4..pos + 8].try_into().unwrap());
    let dab_type = packed >> 26;
    let payload_len_words = packed & 0x03ff_ffff;
    let available_words = record[pos + 8..].len() / 4;

    println!(
        "{refno} PHEI@+0x{pos:X}: packed=0x{packed:08X}, dab_type={dab_type}, payload_len_words={payload_len_words}, available_words_after_header={available_words}"
    );

    if pos + 12 <= record.len() {
        let count = u32::from_be_bytes(record[pos + 8..pos + 12].try_into().unwrap());
        println!("{refno} PHEI payload[0]/count={count}");
    }
}
