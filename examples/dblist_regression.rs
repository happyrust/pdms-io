use anyhow::{bail, Result};
use pdms_io::dblist::parse_dblist_file;
use pdms_io::io::{EleOperationDetail, PdmsIO};
use std::collections::HashMap;
use std::env;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let mut dblist_path: Option<String> = None;
    let mut snapshot_path: Option<String> = None;
    let mut update_snapshot = false;
    let mut db_path: Option<String> = None;
    let mut sessions: Option<u32> = None;
    let mut strict = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--snapshot" => {
                snapshot_path = Some(next_arg(&mut args, "--snapshot 需要路径")?);
            }
            "--update" => update_snapshot = true,
            "--db" => {
                db_path = Some(next_arg(&mut args, "--db 需要数据库路径")?);
            }
            "--sessions" => {
                let val = next_arg(&mut args, "--sessions 需要数值")?;
                sessions = Some(val.parse::<u32>().map_err(|_| "sessions 必须为数字")?);
            }
            "--strict" => strict = true,
            _ => {
                if dblist_path.is_none() {
                    dblist_path = Some(arg);
                } else if snapshot_path.is_none() {
                    snapshot_path = Some(arg);
                } else {
                    bail!("未知参数: {}", arg);
                }
            }
        }
    }

    let dblist_path = match dblist_path {
        Some(p) => p,
        None => {
            print_usage();
            bail!("缺少 DBLIST 路径");
        }
    };

    let snapshot_path = snapshot_path.unwrap_or_else(|| default_snapshot_path(&dblist_path));

    let doc = parse_dblist_file(&dblist_path)?;
    if !doc.warnings.is_empty() {
        eprintln!("解析警告: {:?}", doc.warnings);
    }

    let actual = serde_json::to_value(&doc)?;
    if update_snapshot {
        let pretty = serde_json::to_string_pretty(&actual)?;
        std::fs::write(&snapshot_path, pretty)?;
        println!("已更新快照: {}", snapshot_path);
    } else {
        let expected_str = std::fs::read_to_string(&snapshot_path)?;
        let expected: serde_json::Value = serde_json::from_str(&expected_str)?;
        if actual != expected {
            bail!("解析结果与快照不一致: {}", snapshot_path);
        }
        println!("快照一致: {}", snapshot_path);
    }

    if let Some(db_path) = db_path {
        let name_index = build_name_index(&db_path, sessions).await?;
        let mut missing: Vec<String> = Vec::new();
        let mut matched = 0usize;
        for ele in &doc.elements {
            if let Some(name) = &ele.name {
                if lookup_name(&name_index, name).is_some() {
                    matched += 1;
                } else {
                    missing.push(name.clone());
                }
            }
        }

        println!("名称命中: {} 个, 未命中: {} 个", matched, missing.len());
        if !missing.is_empty() {
            println!("未命中名称(前20个):");
            for name in missing.iter().take(20) {
                println!("  {}", name);
            }
        }

        if strict && !missing.is_empty() {
            bail!("存在未命中的名称");
        }
    }

    Ok(())
}

fn next_arg<I: Iterator<Item = String>>(args: &mut I, err: &str) -> Result<String> {
    args.next().ok_or_else(|| err.into())
}

fn default_snapshot_path(dblist_path: &str) -> String {
    if let Some(stripped) = dblist_path.strip_suffix(".txt") {
        format!("{}.json", stripped)
    } else {
        format!("{}.json", dblist_path)
    }
}

fn lookup_name<'a>(index: &'a HashMap<String, Vec<String>>, name: &str) -> Option<&'a Vec<String>> {
    if let Some(val) = index.get(name) {
        return Some(val);
    }
    if let Some(stripped) = name.strip_prefix('/') {
        return index.get(stripped);
    }
    None
}

async fn build_name_index(
    db_path: &str,
    sessions: Option<u32>,
) -> Result<HashMap<String, Vec<String>>> {
    if !Path::new(db_path).exists() {
        bail!("数据库不存在: {}", db_path);
    }
    let mut io = PdmsIO::new("dblist", db_path, true);
    io.open()?;
    io.init_ses_range_map()?;

    let latest = io.collect_latest_eles(sessions).await?;
    let mut index: HashMap<String, Vec<String>> = HashMap::new();
    for (refno, op) in latest {
        if let EleOperationDetail::Add(ele) = &op.detail {
            let att_map = ele.att_map();
            if let Some(name) = att_map.get("NAME") {
                let name = name.get_val_as_string();
                if !name.is_empty() {
                    index
                        .entry(name)
                        .or_default()
                        .push(refno.to_string());
                }
            }
        }
    }
    Ok(index)
}

fn print_usage() {
    println!("用法: cargo run --example dblist_regression -- <dblist_path> [snapshot_path] [options]");
    println!("选项:");
    println!("  --snapshot <path>   指定快照路径");
    println!("  --update            更新快照");
    println!("  --db <path>         指定数据库路径，构建名称索引");
    println!("  --sessions <n>      限制会话数量（仅用于名称索引）");
    println!("  --strict            名称未命中即失败");
}
