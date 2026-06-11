//! e3d-syncd —— E3D⇄DB 常驻同步守护(specs/006 T201/T202)。
//!
//! 用法:
//!   e3d-syncd --dirs <d1;d2> --surreal <url> --ns <ns> --dbname <db>
//!             [--dbnums 1,2] [--settle-ms 500] [--poll-secs 30]
//!             [--bootstrap N] [--once]
//!
//! - notify 监控(递归)+ **静定窗**(默认 500ms 无新事件才跑轮)+ **轮询兜底**(默认 30s);
//! - 纯水位驱动(无本地状态),重启安全;单库失败隔离不退程(G3);
//! - `--bootstrap N`:启动时对每库先显式初灌 N 个会话(既有唯一入口);
//! - `--once`:跑一轮即退出(smoke/脚本用);Ctrl-C 优雅退出(完成当前轮)。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use notify::{RecursiveMode, Watcher};
use pdms_io::sync_core::{DbSyncOutcome, Trigger, round_due, scan_targets, sync_db};

fn opt(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn req(args: &[String], name: &str) -> anyhow::Result<String> {
    opt(args, name).with_context(|| format!("missing required arg {name} <value>"))
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

struct Counters {
    rounds: u64,
    synced: u64,
    skipped: u64,
    failed: u64,
}

async fn run_round(
    dirs: &[PathBuf],
    whitelist: Option<&[i32]>,
    trigger: Trigger,
    counters: &mut Counters,
) {
    counters.rounds += 1;
    let targets = scan_targets(dirs, whitelist);
    println!(
        "[syncd] round #{} trigger={trigger:?} targets={}",
        counters.rounds,
        targets.len()
    );
    for t in &targets {
        match sync_db(t).await {
            DbSyncOutcome::Synced { from_sesno, to_sesno, sessions } => {
                counters.synced += 1;
                println!(
                    "[syncd]   {} dbnum={} SYNCED ses {from_sesno}..={to_sesno} ({sessions} sessions)",
                    t.path.display(),
                    t.dbnum
                );
            }
            DbSyncOutcome::Baseline { sesno, ops } => {
                counters.synced += 1;
                println!(
                    "[syncd]   {} dbnum={} BASELINE ses {sesno} ({ops} ops)",
                    t.path.display(),
                    t.dbnum
                );
            }
            DbSyncOutcome::SkippedUpToDate { sesno } => {
                counters.skipped += 1;
                println!(
                    "[syncd]   {} dbnum={} up-to-date (ses {sesno})",
                    t.path.display(),
                    t.dbnum
                );
            }
            DbSyncOutcome::Failed(e) => {
                counters.failed += 1;
                eprintln!(
                    "[syncd]   {} dbnum={} FAILED (isolated, retry next round): {e}",
                    t.path.display(),
                    t.dbnum
                );
            }
        }
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dirs: Vec<PathBuf> =
        req(&args, "--dirs")?.split(';').filter(|s| !s.is_empty()).map(PathBuf::from).collect();
    if dirs.is_empty() {
        bail!("--dirs is empty");
    }
    let whitelist: Option<Vec<i32>> = opt(&args, "--dbnums").map(|s| {
        s.split(',').filter_map(|x| x.trim().parse().ok()).collect()
    });
    let settle = Duration::from_millis(opt(&args, "--settle-ms").and_then(|s| s.parse().ok()).unwrap_or(500));
    let poll = Duration::from_secs(opt(&args, "--poll-secs").and_then(|s| s.parse().ok()).unwrap_or(30));
    let once = flag(&args, "--once");

    let url = req(&args, "--surreal")?;
    let ns = req(&args, "--ns")?;
    let dbname = req(&args, "--dbname")?;
    aios_core::SUL_DB.connect(url.as_str()).await.context("connect surreal")?;
    aios_core::use_ns_db_compat(&aios_core::SUL_DB, &ns, &dbname)
        .await
        .map_err(|e| anyhow::anyhow!("use ns/db: {e}"))?;

    // --bootstrap N:显式初灌(G1-A4)。
    if let Some(n) = opt(&args, "--bootstrap").and_then(|s| s.parse::<u32>().ok()) {
        for t in scan_targets(&dirs, whitelist.as_deref()) {
            println!("[syncd] bootstrap {} (last {n} sessions)...", t.path.display());
            if let Err(e) = pdms_io::sync_core::bootstrap_db(&t, n).await {
                eprintln!("[syncd] bootstrap {} failed: {e:#}", t.path.display());
            }
        }
    }

    // notify:事件只置脏标记 + 刷新最后事件时刻(真正动作在静定窗后,G2-A1)。
    let dirty = Arc::new(AtomicBool::new(false));
    let last_event = Arc::new(std::sync::Mutex::new(Instant::now()));
    let (d2, le2) = (dirty.clone(), last_event.clone());
    let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
        if res.is_ok() {
            d2.store(true, Ordering::SeqCst);
            *le2.lock().unwrap() = Instant::now();
        }
    })
    .context("create watcher")?;
    for d in &dirs {
        watcher.watch(d, RecursiveMode::Recursive).with_context(|| format!("watch {}", d.display()))?;
    }

    let mut counters = Counters { rounds: 0, synced: 0, skipped: 0, failed: 0 };
    println!(
        "[syncd] watching {} dir(s), settle={}ms poll={}s once={once}",
        dirs.len(),
        settle.as_millis(),
        poll.as_secs()
    );

    // 首轮立即跑(启动即对齐),其后由事件/轮询驱动。
    run_round(&dirs, whitelist.as_deref(), Trigger::Poll, &mut counters).await;
    let mut last_round = Instant::now();

    if !once {
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    println!("[syncd] ctrl-c, exiting after current round");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(200)) => {
                    let since_event = last_event.lock().unwrap().elapsed();
                    let due = round_due(
                        dirty.load(Ordering::SeqCst),
                        since_event,
                        settle,
                        last_round.elapsed(),
                        poll,
                    );
                    if let Some(trigger) = due {
                        dirty.store(false, Ordering::SeqCst); // 先清标记:轮中新事件触发下一轮
                        run_round(&dirs, whitelist.as_deref(), trigger, &mut counters).await;
                        last_round = Instant::now();
                    }
                }
            }
        }
    }

    println!(
        "[syncd] done: rounds={} synced={} skipped={} failed={}",
        counters.rounds, counters.synced, counters.skipped, counters.failed
    );
    Ok(())
}
