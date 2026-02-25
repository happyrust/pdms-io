use aios_core::pdms_types::RefU64;
use pdms_io::io::{EleOperationDetail, PdmsIO};
use pdms_io::test::resolve_test_db_path;
use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::time::{Duration, Instant};

#[tokio::test]
async fn diag_ams1112_full_parse() -> anyhow::Result<()> {
    let db_path = match resolve_test_db_path("ams1112_0001") {
        Some(path) => path,
        None => {
            eprintln!("数据库文件不存在，跳过: ams1112_0001");
            return Ok(());
        }
    };
    if !Path::new(&db_path).exists() {
        eprintln!("数据库文件不存在，跳过: {}", db_path.display());
        return Ok(());
    }

    let mut io = PdmsIO::new("ams", &db_path, false);
    io.open()?;
    io.init_ses_range_map()?;

    let latest_sesno = io.get_latest_sesno()? as i32;
    let sesno_range = 1..=latest_sesno;

    println!(
        "开始全量会话解析: file={}, sesno_range={:?}",
        db_path.display(),
        sesno_range
    );
    let t0 = Instant::now();
    let mut grouped = std::collections::BTreeMap::new();
    let mut panic_sessions: Vec<(i32, String)> = Vec::new();
    let mut error_sessions: Vec<(i32, String)> = Vec::new();

    let result = catch_unwind(AssertUnwindSafe(|| {
        io.collect_increment_eles(Some(sesno_range.clone()))
    }));
    match result {
        Ok(Ok(all)) => grouped = all,
        Ok(Err(e)) => error_sessions.push((-1, e.to_string())),
        Err(panic_payload) => {
            let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "未知 panic".to_string()
            };
            panic_sessions.push((-1, panic_msg));
        }
    }
    let elapsed = t0.elapsed();

    let mut add_count = 0usize;
    let mut modified_count = 0usize;
    let mut deleted_count = 0usize;
    let mut none_count = 0usize;
    let mut total_ops = 0usize;

    for ops in grouped.values() {
        total_ops += ops.len();
        for op in ops {
            match &op.detail {
                EleOperationDetail::Add(_) => add_count += 1,
                EleOperationDetail::Modified(_) => modified_count += 1,
                EleOperationDetail::Deleted => deleted_count += 1,
                EleOperationDetail::None => none_count += 1,
            }
        }
    }

    let covered_sessions: BTreeSet<u32> = grouped.keys().copied().collect();
    let requested_total = latest_sesno as u32;
    let missing_sessions = (1..=requested_total)
        .filter(|s| !covered_sessions.contains(s))
        .count();

    println!("全量解析完成: 耗时={:?}", elapsed);
    println!(
        "覆盖会话数: {} / {}",
        covered_sessions.len(),
        requested_total
    );
    println!("无变更会话数: {}", missing_sessions);
    println!(
        "操作统计: total={}, add={}, modified={}, deleted={}, none={}",
        total_ops, add_count, modified_count, deleted_count, none_count
    );
    println!(
        "会话级问题统计: panics={}, errors={}",
        panic_sessions.len(),
        error_sessions.len()
    );
    if let Some((ses, msg)) = panic_sessions.first() {
        println!("首个 panic 会话: sesno={}, msg={}", ses, msg);
    }
    if let Some((ses, msg)) = error_sessions.first() {
        println!("首个 error 会话: sesno={}, err={}", ses, msg);
    }

    assert!(
        !grouped.is_empty(),
        "全量解析结果为空，请检查会话映射或解析流程"
    );
    assert!(
        panic_sessions.is_empty() && error_sessions.is_empty(),
        "全量解析存在会话级异常: panics={}, errors={}",
        panic_sessions.len(),
        error_sessions.len()
    );

    Ok(())
}

#[tokio::test]
async fn diag_ams1112_panic_sessions_regression() -> anyhow::Result<()> {
    let db_path = match resolve_test_db_path("ams1112_0001") {
        Some(path) => path,
        None => {
            eprintln!("数据库文件不存在，跳过: ams1112_0001");
            return Ok(());
        }
    };
    if !Path::new(&db_path).exists() {
        eprintln!("数据库文件不存在，跳过: {}", db_path.display());
        return Ok(());
    }

    // 来自上一轮全量诊断日志中的 panic 会话集合。
    let panic_sessions = [
        5, 8, 10, 11, 13, 15, 18, 30, 33, 716, 717, 718, 720, 721, 723, 726, 727, 728, 729, 730,
        731, 732, 733, 735, 736, 737, 739, 740, 741, 742, 743, 744, 745, 746, 747, 748, 749, 756,
        757, 758, 759, 766, 767, 768, 771, 772, 773, 774, 776, 778, 779, 781, 782, 783, 785, 787,
        790, 796, 805, 807, 810, 818, 822, 824, 826, 841, 842, 843, 844, 875, 876, 877, 878, 892,
        928, 971, 972, 973, 975, 976, 977, 978, 979, 980, 981, 982, 983, 984, 985, 986, 987, 1002,
    ];

    let mut io = PdmsIO::new("ams", &db_path, false);
    io.open()?;
    io.init_ses_range_map()?;

    let t0 = Instant::now();
    let mut panic_hits = Vec::new();

    for sesno in panic_sessions {
        let result = catch_unwind(AssertUnwindSafe(|| {
            io.collect_increment_eles(Some(sesno..=sesno))
        }));
        if let Err(payload) = result {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "未知 panic".to_string()
            };
            panic_hits.push((sesno, msg));
        }
    }

    println!(
        "panic 回归会话复测完成: sessions={}, panic_hits={}, elapsed={:?}",
        panic_sessions.len(),
        panic_hits.len(),
        t0.elapsed()
    );
    if let Some((sesno, msg)) = panic_hits.first() {
        println!("首个残留 panic: sesno={}, msg={}", sesno, msg);
    }

    assert!(
        panic_hits.is_empty(),
        "仍存在会话 panic，数量={}",
        panic_hits.len()
    );

    Ok(())
}

#[tokio::test]
async fn diag_ams1112_key_sessions_no_panic() -> anyhow::Result<()> {
    let db_path = match resolve_test_db_path("ams1112_0001") {
        Some(path) => path,
        None => {
            eprintln!("数据库文件不存在，跳过: ams1112_0001");
            return Ok(());
        }
    };
    if !Path::new(&db_path).exists() {
        eprintln!("数据库文件不存在，跳过: {}", db_path.display());
        return Ok(());
    }

    // 覆盖早期、中期、后期三个曾经触发 panic 的关键会话。
    let key_sessions = [5, 716, 975];

    let mut io = PdmsIO::new("ams", &db_path, false);
    io.open()?;
    io.init_ses_range_map()?;

    for sesno in key_sessions {
        let result = catch_unwind(AssertUnwindSafe(|| {
            io.collect_increment_eles(Some(sesno..=sesno))
        }));
        match result {
            Ok(Ok(grouped)) => {
                let op_count = grouped.get(&(sesno as u32)).map(|v| v.len()).unwrap_or(0);
                println!("关键会话解析成功: sesno={}, op_count={}", sesno, op_count);
            }
            Ok(Err(e)) => {
                panic!("关键会话返回 error: sesno={}, err={}", sesno, e);
            }
            Err(_) => {
                panic!("关键会话发生 panic: sesno={}", sesno);
            }
        }
    }

    Ok(())
}

#[tokio::test]
async fn diag_ams1112_session5_no_panic() -> anyhow::Result<()> {
    let db_path = match resolve_test_db_path("ams1112_0001") {
        Some(path) => path,
        None => {
            eprintln!("数据库文件不存在，跳过: ams1112_0001");
            return Ok(());
        }
    };
    if !Path::new(&db_path).exists() {
        eprintln!("数据库文件不存在，跳过: {}", db_path.display());
        return Ok(());
    }

    let mut io = PdmsIO::new("ams", &db_path, false);
    io.open()?;
    io.init_ses_range_map()?;

    let sesno = 5;
    let result = catch_unwind(AssertUnwindSafe(|| {
        io.collect_increment_eles(Some(sesno..=sesno))
    }));
    match result {
        Ok(Ok(grouped)) => {
            let op_count = grouped.get(&(sesno as u32)).map(|v| v.len()).unwrap_or(0);
            println!("会话5解析成功: op_count={}", op_count);
        }
        Ok(Err(e)) => {
            panic!("会话5返回 error: {}", e);
        }
        Err(_) => {
            panic!("会话5发生 panic");
        }
    }
    Ok(())
}

#[tokio::test]
async fn diag_ams1112_session716_profile() -> anyhow::Result<()> {
    let db_path = match resolve_test_db_path("ams1112_0001") {
        Some(path) => path,
        None => {
            eprintln!("数据库文件不存在，跳过: ams1112_0001");
            return Ok(());
        }
    };
    if !Path::new(&db_path).exists() {
        eprintln!("数据库文件不存在，跳过: {}", db_path.display());
        return Ok(());
    }

    let mut io = PdmsIO::new("ams", &db_path, false);
    io.open()?;
    io.init_ses_range_map()?;

    let sesno = 716;
    let max_refs = std::env::var("DIAG_MAX_REFNOS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(2000);
    let max_secs = std::env::var("DIAG_MAX_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(300);
    let run_increment = std::env::var("DIAG_RUN_INCREMENT")
        .map(|s| s == "1")
        .unwrap_or(false);

    println!("开始剖析会话 {} ...", sesno);
    println!(
        "剖析参数: max_refs={}, max_secs={}, run_increment={}",
        max_refs, max_secs, run_increment
    );

    let t_collect = Instant::now();
    let locs = io.collect_refno_locs(sesno);
    let collect_elapsed = t_collect.elapsed();
    let unique_refnos: BTreeSet<RefU64> = locs
        .iter()
        .map(|loc| RefU64::from_two_nums(loc.refno_0, loc.refno_1))
        .collect();
    let duplicate_count = locs.len().saturating_sub(unique_refnos.len());
    println!(
        "collect_refno_locs 完成: sesno={}, ref_count={}, unique_ref_count={}, duplicate_count={}, elapsed={:?}",
        sesno,
        locs.len(),
        unique_refnos.len(),
        duplicate_count,
        collect_elapsed
    );
    if locs.len() > max_refs {
        println!(
            "注意: 本次只抽样前 {} 个 refno（总数 {}）",
            max_refs,
            locs.len()
        );
    }

    let mut add_count = 0usize;
    let mut modified_count = 0usize;
    let mut deleted_count = 0usize;
    let mut none_count = 0usize;
    let mut error_count = 0usize;
    let mut slow_samples: Vec<(String, Duration)> = Vec::new();
    let mut error_samples: Vec<String> = Vec::new();
    let t_status = Instant::now();

    let mut processed = 0usize;
    for (idx, loc) in locs.iter().take(max_refs).enumerate() {
        if t_status.elapsed() >= Duration::from_secs(max_secs) {
            println!(
                "达到时间上限，提前停止状态计算: processed={}, elapsed={:?}",
                processed,
                t_status.elapsed()
            );
            break;
        }

        let refno = RefU64::from_two_nums(loc.refno_0, loc.refno_1);
        let t_one = Instant::now();
        let status = io.get_refno_operation_status(refno, Some(sesno as u32));
        let one_elapsed = t_one.elapsed();
        processed += 1;

        if one_elapsed >= Duration::from_millis(200) && slow_samples.len() < 30 {
            slow_samples.push((refno.to_string(), one_elapsed));
        }

        match status {
            Ok(status_map) => match status_map.get(&refno) {
                Some(EleOperationDetail::Add(_)) => add_count += 1,
                Some(EleOperationDetail::Modified(_)) => modified_count += 1,
                Some(EleOperationDetail::Deleted) => deleted_count += 1,
                Some(EleOperationDetail::None) => none_count += 1,
                None => {
                    // 理论上应当命中当前 refno，未命中视作 None 计数，便于观察异常比例。
                    none_count += 1;
                }
            },
            Err(e) => {
                error_count += 1;
                if error_samples.len() < 20 {
                    error_samples.push(format!("refno={}: {}", refno, e));
                }
            }
        }

        if (idx + 1) % 200 == 0 || idx + 1 == max_refs.min(locs.len()) {
            println!(
                "状态计算进度: {}/{} (add={}, modified={}, deleted={}, none={}, errors={})",
                idx + 1,
                max_refs.min(locs.len()),
                add_count,
                modified_count,
                deleted_count,
                none_count,
                error_count
            );
        }
    }

    let status_elapsed = t_status.elapsed();
    println!(
        "get_refno_operation_status 汇总: calls={}, elapsed={:?}, add={}, modified={}, deleted={}, none={}, errors={}",
        processed,
        status_elapsed,
        add_count,
        modified_count,
        deleted_count,
        none_count,
        error_count
    );

    if !slow_samples.is_empty() {
        println!("慢调用样本(>=200ms, 最多30条):");
        for (refno, elapsed) in &slow_samples {
            println!("  refno={} elapsed={:?}", refno, elapsed);
        }
    }
    if !error_samples.is_empty() {
        println!("错误样本(最多20条):");
        for item in &error_samples {
            println!("  {}", item);
        }
    }

    if run_increment {
        let t_inc = Instant::now();
        let grouped = io.collect_increment_eles(Some(sesno..=sesno))?;
        let inc_elapsed = t_inc.elapsed();
        let op_count = grouped.get(&(sesno as u32)).map(|v| v.len()).unwrap_or(0);
        println!(
            "collect_increment_eles 完成: sesno={}, grouped_sessions={}, op_count={}, elapsed={:?}",
            sesno,
            grouped.len(),
            op_count,
            inc_elapsed
        );
    } else {
        println!("已跳过 collect_increment_eles；设置 DIAG_RUN_INCREMENT=1 可启用");
    }

    assert!(
        !locs.is_empty(),
        "会话 {} 的 refno 为空，无法完成性能剖析",
        sesno
    );
    Ok(())
}
