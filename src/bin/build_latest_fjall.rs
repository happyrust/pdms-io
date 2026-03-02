use anyhow::Result;
use pdms_io::latest_fjall::build_latest_fjall_from_db;
use std::path::PathBuf;

fn print_usage() {
    eprintln!("用法: cargo run --bin build_latest_fjall -- <db_file> [out_dir] [batch_size]");
    eprintln!("示例: cargo run --bin build_latest_fjall -- D:/.../ams8000_0001 ./.fjall_latest 500");
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);

    let Some(db_file) = args.next() else {
        print_usage();
        std::process::exit(1);
    };

    let out_dir = args.next().unwrap_or_else(|| ".fjall_latest".to_string());
    let batch_size = args
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(500);

    let db_file = PathBuf::from(db_file);
    let out_dir = PathBuf::from(out_dir);

    let summary = build_latest_fjall_from_db(&db_file, &out_dir, batch_size).await?;

    println!("构建完成:");
    println!("  db_file: {}", summary.db_file);
    println!("  out_dir: {}", summary.out_dir);
    println!("  latest_sesno: {}", summary.latest_sesno);
    println!("  latest_refnos: {}", summary.total_latest_refnos);
    println!("  parsed_ok: {}", summary.parsed_ok);
    println!("  parsed_failed: {}", summary.parsed_failed);
    println!("  elapsed_ms: {}", summary.elapsed_ms);

    Ok(())
}
