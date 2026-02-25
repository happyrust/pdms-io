use crate::parser::attlib::AttlibData;
use aios_core::tool::db_tool::{db1_dehash, db1_hash};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

const ATTLIB_PATH: &str = "D:\\work\\plant-code\\pdms-io-fork\\test-file\\attlib.dat";
const PAGE_SIZE: usize = 2048;

fn read_page(file: &mut File, page_num: usize) -> Vec<u32> {
    let offset = page_num * PAGE_SIZE;
    file.seek(SeekFrom::Start(offset as u64)).unwrap();
    let mut buf = vec![0u8; PAGE_SIZE];
    file.read_exact(&mut buf).unwrap();
    buf.chunks_exact(4)
        .map(|c| u32::from_be_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[test]
fn diag_directory_page() {
    if !std::path::Path::new(ATTLIB_PATH).exists() {
        return;
    }
    let mut file = File::open(ATTLIB_PATH).unwrap();

    // Page 0
    let page0 = read_page(&mut file, 0);
    println!("=== Page 0 (前32个word) ===");
    for (i, &v) in page0.iter().take(32).enumerate() {
        println!("  [{}] 0x{:08X} ({})", i, v, v);
    }

    // Page 1 (目录页)
    let page1 = read_page(&mut file, 1);
    println!("\n=== Page 1 (目录页, 前32个word) ===");
    for (i, &v) in page1.iter().take(32).enumerate() {
        let decoded = if v >= 531_442 && v <= 387_951_929 {
            format!(" -> dehash=\"{}\"", db1_dehash(v))
        } else {
            String::new()
        };
        println!("  [{}] 0x{:08X} ({}){}", i, v, v, decoded);
    }
}

#[test]
fn diag_atnain_raw_data() {
    if !std::path::Path::new(ATTLIB_PATH).exists() {
        return;
    }
    let mut file = File::open(ATTLIB_PATH).unwrap();

    let page1 = read_page(&mut file, 1);
    let atnain_start = page1.get(3).copied().unwrap_or(0) as usize;
    println!("ATNAIN start page (dir[3]): {}", atnain_start);

    if atnain_start == 0 {
        return;
    }

    // 转储 ATNAIN 起始页的前 60 个 word
    let atnain_page = read_page(&mut file, atnain_start);
    println!("\n=== ATNAIN Page {} (前60个word) ===", atnain_start);
    for (i, &v) in atnain_page.iter().take(60).enumerate() {
        let decoded = if v >= 531_442 && v <= 387_951_929 {
            format!(" -> dehash=\"{}\"", db1_dehash(v))
        } else {
            String::new()
        };
        println!("  [{}] 0x{:08X} ({}){}", i, v, v, decoded);
    }

    // 检查是否有已知 NOUN hash
    let known_hashes: Vec<(&str, u32)> = vec![
        ("ELBO", db1_hash("ELBO") as u32),
        ("PIPE", db1_hash("PIPE") as u32),
        ("TEE", db1_hash("TEE") as u32),
        ("EQUI", db1_hash("EQUI") as u32),
        ("SITE", db1_hash("SITE") as u32),
    ];

    println!("\n=== 已知 NOUN Hash ===");
    for (name, hash) in &known_hashes {
        println!("  {} -> 0x{:08X} ({})", name, hash, hash);
    }

    // 在整个 ATNAIN 区搜索已知 hash
    println!("\n=== 在 ATNAIN 区搜索已知 NOUN ===");
    for page_idx in atnain_start..atnain_start + 30 {
        let page = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            read_page(&mut file, page_idx)
        })) {
            Ok(p) => p,
            Err(_) => break,
        };
        for (i, &v) in page.iter().enumerate() {
            for (name, hash) in &known_hashes {
                if v == *hash {
                    println!(
                        "  找到 {} (0x{:08X}) 在 page {} offset {}",
                        name, hash, page_idx, i
                    );
                    // 打印上下文
                    let start = i.saturating_sub(3);
                    let end = (i + 6).min(page.len());
                    for j in start..end {
                        let marker = if j == i { " <--" } else { "" };
                        let decoded = if page[j] >= 531_442 && page[j] <= 387_951_929 {
                            format!(" dehash=\"{}\"", db1_dehash(page[j]))
                        } else {
                            String::new()
                        };
                        println!(
                            "    [{:3}] 0x{:08X} ({}){}{}",
                            j, page[j], page[j], decoded, marker
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn diag_scan_all_pages_for_nouns() {
    if !std::path::Path::new(ATTLIB_PATH).exists() {
        return;
    }
    let mut file = File::open(ATTLIB_PATH).unwrap();

    let elbo_hash = db1_hash("ELBO") as u32;
    let pipe_hash = db1_hash("PIPE") as u32;

    // 扫描所有页面（最多2000页）
    println!(
        "=== 全局搜索 ELBO(0x{:08X}) 和 PIPE(0x{:08X}) ===",
        elbo_hash, pipe_hash
    );
    for page_idx in 0..2000 {
        let page = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            read_page(&mut file, page_idx)
        })) {
            Ok(p) => p,
            Err(_) => break,
        };
        for (i, &v) in page.iter().enumerate() {
            if v == elbo_hash || v == pipe_hash {
                let name = if v == elbo_hash { "ELBO" } else { "PIPE" };
                println!("  {} 在 page={} offset={}", name, page_idx, i);
                let start = i.saturating_sub(2);
                let end = (i + 5).min(page.len());
                for j in start..end {
                    let marker = if j == i { " <--" } else { "" };
                    let decoded = if page[j] >= 531_442 && page[j] <= 387_951_929 {
                        format!(" dehash=\"{}\"", db1_dehash(page[j]))
                    } else {
                        String::new()
                    };
                    println!(
                        "    [{:3}] 0x{:08X} ({}){}{}",
                        j, page[j], page[j], decoded, marker
                    );
                }
            }
        }
    }
}
