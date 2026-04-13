use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use pdmsdb_engine_v2::compare::core_dll_oracle::CoreDllOracle;
use pdmsdb_engine_v2::compare::legacy_oracle::LegacyOracle;
use pdmsdb_engine_v2::{EngineOptions, EngineV2, RefNo};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn default_db_path() -> Result<PathBuf> {
    LegacyOracle::resolve_repo_test_db_path(workspace_root(), "ams1112_0001")
        .ok_or_else(|| anyhow!("未找到默认测试数据库: ams1112_0001"))
}

fn default_refno() -> RefNo {
    RefNo::from_parts(17496, 171138)
}

fn parse_refno(text: &str) -> Result<RefNo> {
    let trimmed = text.trim();
    let parts = trimmed
        .split([':', '/'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        bail!("refno 格式非法，应为 hi:lo 或 hi/lo，实际为: {}", text);
    }
    let hi = parts[0]
        .parse::<u32>()
        .with_context(|| format!("无法解析 refno 高位: {}", parts[0]))?;
    let lo = parts[1]
        .parse::<u32>()
        .with_context(|| format!("无法解析 refno 低位: {}", parts[1]))?;
    Ok(RefNo::from_parts(hi, lo))
}

fn parse_ignore_keys(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
        .collect()
}

fn usage() -> &'static str {
    "用法: engine_v2_compare_fixture [--db <path>] [--refno <hi:lo>] [--output-root <dir>] [--ignore K1,K2] [--seed-fixture]"
}

fn main() -> Result<()> {
    let mut db_path: Option<PathBuf> = None;
    let mut refno = default_refno();
    let mut output_root: Option<PathBuf> = None;
    let mut ignore_keys = vec!["PGNO".to_string()];
    let mut seed_fixture = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => {
                let value = args.next().ok_or_else(|| anyhow!("--db 缺少参数"))?;
                db_path = Some(PathBuf::from(value));
            }
            "--refno" => {
                let value = args.next().ok_or_else(|| anyhow!("--refno 缺少参数"))?;
                refno = parse_refno(&value)?;
            }
            "--output-root" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("--output-root 缺少参数"))?;
                output_root = Some(PathBuf::from(value));
            }
            "--ignore" => {
                let value = args.next().ok_or_else(|| anyhow!("--ignore 缺少参数"))?;
                ignore_keys = parse_ignore_keys(&value);
            }
            "--seed-fixture" => {
                seed_fixture = true;
            }
            "--help" | "-h" => {
                println!("{}", usage());
                return Ok(());
            }
            other => bail!("未知参数: {}\n{}", other, usage()),
        }
    }

    let db_path = db_path.unwrap_or(default_db_path()?);
    let output_root = output_root.unwrap_or_else(workspace_root);
    std::fs::create_dir_all(&output_root)?;

    let repo_root = workspace_root();
    CoreDllOracle::prepare_parse_environment(&repo_root);

    let handle = EngineV2::open_read(&db_path, EngineOptions::default())
        .with_context(|| format!("打开数据库失败: {}", db_path.display()))?;
    let rust_json = CoreDllOracle::build_rust_parse_json(&handle, refno)?;

    let refno_text = CoreDllOracle::refno_to_string(refno);
    let ignore_refs = ignore_keys.iter().map(String::as_str).collect::<Vec<_>>();

    if seed_fixture {
        let fixture_path = CoreDllOracle::write_fixture(&output_root, &refno_text, &rust_json)?;
        println!("已写入 fixture: {}", fixture_path.display());
    } else if !CoreDllOracle::fixture_path(&output_root, &refno_text).exists() {
        bail!(
            "fixture 不存在: {}。可加 --seed-fixture 先用 Rust 输出生成基线。",
            CoreDllOracle::fixture_path(&output_root, &refno_text).display()
        );
    }

    let report =
        CoreDllOracle::compare_refno_with_fixture(&output_root, &handle, refno, &ignore_refs)?;

    println!("db: {}", db_path.display());
    println!("refno: {}", report.refno);
    println!("rust_json: {}", report.rust_path.display());
    println!("fixture: {}", report.fixture_path.display());
    println!("report: {}", report.report_path.display());
    println!("ignored_keys: {}", report.ignored_keys.join(","));
    println!("matches: {}", report.matches);
    println!("diff_count: {}", report.diff_count);
    if !report.matches {
        for diff in &report.diffs {
            println!(
                "diff key={} left={:?} right={:?}",
                diff.key, diff.left, diff.right
            );
        }
    }

    Ok(())
}
