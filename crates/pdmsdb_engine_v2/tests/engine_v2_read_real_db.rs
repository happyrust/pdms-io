use pdmsdb_engine_v2::db4::{ElementRecordView, parse_explicit_blocks, parse_member_refs};
use pdmsdb_engine_v2::{EngineOptions, EngineV2, RefNo};

const DB_PATH: &str = r"D:\work\plant-code\pdms-io-fork\test-file\ams1112_0001";

fn open_db() -> Option<pdmsdb_engine_v2::DbHandle> {
    if !std::path::Path::new(DB_PATH).exists() {
        println!("数据库不存在，跳过: {}", DB_PATH);
        return None;
    }
    match EngineV2::open_read(DB_PATH, EngineOptions::default()) {
        Ok(h) => Some(h),
        Err(e) => {
            println!("打开失败: {:?}", e);
            None
        }
    }
}

#[test]
fn open_and_show_sessions() {
    let handle = match open_db() {
        Some(h) => h,
        None => return,
    };

    let header = handle.header();
    println!("page_size: {}", handle.page_size());
    println!("session_page_no: {}", header.session_page_no);
    println!("latest_ses_pgno: {}", header.latest_ses_pgno);
    println!("extent_count: {}", handle.extent_count());

    let sessions = handle.sessions();
    println!("session_count: {}", sessions.len());
    for (i, s) in sessions.iter().enumerate().take(5) {
        println!(
            "  session[{}]: sesno={}, page={}, index_root={}, end_page={}",
            i, s.sesno, s.page.page_no, s.index_root.page_no, s.end_page.page_no
        );
    }
    if sessions.len() > 5 {
        println!("  ... ({} more)", sessions.len() - 5);
    }
    assert!(!sessions.is_empty(), "should have at least one session");
}

#[test]
fn read_sample_refno_17496_171138() {
    let handle = match open_db() {
        Some(h) => h,
        None => return,
    };

    let refno = RefNo::from_parts(17496, 171138);
    let hit = handle.find_refno(refno, None).unwrap();
    assert!(hit.is_some(), "refno 17496:171138 should exist");

    let hit = hit.unwrap();
    println!(
        "found: sesno={}, page_no={}, offset={}",
        hit.sesno, hit.loc.page_no, hit.loc.byte_offset
    );

    let raw = handle.read_record(hit.loc).unwrap();
    println!("record.len: {} bytes", raw.len());

    let view = ElementRecordView::from_raw(&raw).unwrap();
    println!("refno: {:?}", view.refno);
    println!("noun_hash: 0x{:08X}", view.noun_hash);
    println!("owner: {:?}", view.owner);
    println!("impl_len_words: {}", view.impl_len_words);
    println!("implicit_data.len: {} bytes", view.implicit_data.len());
    println!("members_data.len: {} bytes", view.members_data.len());
    println!("explicit_data.len: {} bytes", view.explicit_data.len());

    assert_eq!(view.refno, refno);
    assert!(view.noun_hash != 0);

    let children = parse_member_refs(&view.members_data);
    println!("children: {}", children.len());
    for (i, c) in children.iter().enumerate().take(5) {
        println!("  child[{}]: {}:{}", i, c.hi(), c.lo());
    }

    if !view.explicit_data.is_empty() {
        match parse_explicit_blocks(&view.explicit_data) {
            Ok(blocks) => {
                println!("explicit_blocks: {}", blocks.len());
                for (i, b) in blocks.iter().enumerate().take(10) {
                    println!(
                        "  block[{}]: flag=0x{:02X}, hash=0x{:08X}, payload={}B",
                        i, b.flag, b.hash, b.payload.len()
                    );
                }
            }
            Err(e) => println!("explicit parse error: {:?}", e),
        }
    }
}

#[test]
fn scan_all_refnos_and_sample_10() {
    let handle = match open_db() {
        Some(h) => h,
        None => return,
    };

    let entries = handle.iter_all_refnos().unwrap();
    println!("total refnos: {}", entries.len());
    assert!(entries.len() > 0, "should have entries in index");

    let sample_size = 10.min(entries.len());
    let step = entries.len() / sample_size;

    println!("\nsampling {} refnos:", sample_size);
    let mut success = 0;
    let mut fail = 0;

    for i in 0..sample_size {
        let entry = &entries[i * step];
        let refno = entry.refno;
        let loc = entry.loc;

        match handle.read_record(loc) {
            Ok(raw) => match ElementRecordView::from_raw(&raw) {
                Ok(view) => {
                    println!(
                        "  [{}] {}:{} => noun=0x{:08X}, impl={}B, memb={}B, expl={}B",
                        i,
                        refno.hi(),
                        refno.lo(),
                        view.noun_hash,
                        view.implicit_data.len(),
                        view.members_data.len(),
                        view.explicit_data.len(),
                    );
                    success += 1;
                }
                Err(e) => {
                    println!("  [{}] {}:{} => parse error: {:?}", i, refno.hi(), refno.lo(), e);
                    fail += 1;
                }
            },
            Err(e) => {
                println!("  [{}] {}:{} => read error: {:?}", i, refno.hi(), refno.lo(), e);
                fail += 1;
            }
        }
    }

    println!("\nresult: {} success, {} fail out of {}", success, fail, sample_size);
    assert!(success > 0, "at least some records should parse successfully");
}
