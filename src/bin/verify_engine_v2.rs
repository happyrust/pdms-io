use std::env;
use std::path::PathBuf;
use std::time::Instant;

use pdms_io::engine_v2::db2::session::SessionManager;
use pdms_io::engine_v2::db3::iter::TableIterator;
use pdms_io::engine_v2::db5::database::Database;
use pdms_io::engine_v2::types::RefNo;

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let default = manifest.join("test-file").join("ams1112_0001");
        default.to_string_lossy().to_string()
    });

    println!("=== Engine V2 验证 ===");
    println!("文件: {}", path);

    let t0 = Instant::now();
    let mut db = match Database::open_read(&path, 512) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("打开失败: {e}");
            return;
        }
    };
    println!("打开耗时: {:?}", t0.elapsed());

    println!("\n--- Header ---");
    println!("  version:       {}", db.header.version);
    println!("  db_num:        {}", db.header.db_num);
    println!("  page_size:     {} bytes", db.header.detected_page_size());
    println!("  latest_ses:    page {}", db.header.latest_ses_pgno);
    println!("  ext_no:        {}", db.header.ext_no);
    println!("  stored_pages:  {}", db.header.stored_page_count);
    println!("  total_pages:   {}", db.handle.total_pages());

    println!("\n--- Session 链 ---");
    let t1 = Instant::now();
    let (dbno, extent, latest_ses) = (db.dbno(), db.extent(), db.header.latest_ses_pgno);
    let mut root_pgno = 2u32;
    match SessionManager::traverse_chain(&mut db.cache, &mut db.handle, dbno, extent, latest_ses) {
        Ok(sessions) => {
            println!("  会话数: {} (耗时 {:?})", sessions.len(), t1.elapsed());
            for s in sessions.iter().take(5) {
                println!(
                    "    ses#{} page={} prev={} ts={} computer=\"{}\" index_root={}",
                    s.ses_no,
                    s.page_no,
                    s.prev_ses_page,
                    s.timestamp,
                    s.computer_name,
                    s.index_root_pgno
                );
            }
            if sessions.len() > 5 {
                println!("    ... 还有 {} 个会话", sessions.len() - 5);
            }
            if let Some(latest) = sessions.first() {
                if latest.index_root_pgno > 0 {
                    root_pgno = latest.index_root_pgno;
                    println!("  → 使用最新会话的索引根页: {}", root_pgno);
                }
            }
        }
        Err(e) => println!("  遍历失败: {e}"),
    }

    println!("\n--- B-树索引遍历 (根页={}) ---", root_pgno);
    let t2 = Instant::now();
    match TableIterator::new(&mut db.cache, &mut db.handle, dbno, extent, root_pgno) {
        Ok(mut iter) => match iter.collect_all(&mut db.cache, &mut db.handle) {
            Ok(entries) => {
                println!("  索引条目数: {} (耗时 {:?})", entries.len(), t2.elapsed());
                for e in entries.iter().take(5) {
                    println!(
                        "    RefNo({:#010X}:{:#010X}) → page={} offset={}",
                        e.refno.hi,
                        e.refno.lo,
                        e.page_no,
                        e.offset()
                    );
                }
                if entries.len() > 5 {
                    println!("    ... 还有 {} 个条目", entries.len() - 5);
                }

                if let Some(first) = entries.first() {
                    println!("\n--- B-树精确搜索 ---");
                    let t3 = Instant::now();
                    let target = first.refno;
                    match db.find_element(target, root_pgno) {
                        Ok(Some(loc)) => {
                            println!(
                                "  搜索 {} → page={} offset={} (耗时 {:?})",
                                target,
                                loc.page_no,
                                loc.offset,
                                t3.elapsed()
                            );
                        }
                        Ok(None) => println!("  搜索 {} → 未找到", target),
                        Err(e) => println!("  搜索失败: {e}"),
                    }
                }
            }
            Err(e) => println!("  遍历失败: {e}"),
        },
        Err(e) => println!("  迭代器创建失败: {e}"),
    }

    println!("\n--- 缓存统计 ---");
    let stats = &db.cache.stats;
    println!(
        "  命中: {}  未命中: {}  命中率: {:.1}%",
        stats.hits,
        stats.misses,
        stats.hit_rate() * 100.0
    );
    println!(
        "  驱逐: {}  脏页写回: {}  预读: {}",
        stats.evictions, stats.dirty_writebacks, stats.prefetch_reads
    );

    println!("\n=== 验证完成 ===");
}
