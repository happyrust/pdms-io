use aios_core::RefU64;
use anyhow::{Context, Result, anyhow};
use pdms_io::io::PdmsIO;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::opt::auth::Root;
use surrealdb::types::{Number, Object, RecordId, RecordIdKey, Value};

fn normalize_pdms_string(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        // PDMS string expr 以单引号包裹，内部用 '' 表示单引号
        return s[1..s.len() - 1].replace("''", "'");
    }
    s.to_string()
}

fn refno_from_record_id(id: &RecordId) -> Option<RefU64> {
    match &id.key {
        RecordIdKey::String(s) => {
            let (a, b) = s.split_once('_')?;
            let r0: u32 = a.parse().ok()?;
            let r1: u32 = b.parse().ok()?;
            Some(RefU64::from_two_nums(r0, r1))
        }
        _ => None,
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn resolve_project_path() -> Option<PathBuf> {
    // 1) 优先使用环境变量（更显式）
    if let Ok(v) = std::env::var("PDMS_PROJECT_PATH") {
        let p = PathBuf::from(v);
        if p.exists() {
            return Some(p);
        }
    }

    // 2) 回退读取 DbOption.toml（本仓库根目录已配置 project_path）
    let dbopt = workspace_root().join("DbOption.toml");
    let content = fs::read_to_string(&dbopt).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("project_path") {
            let v = v
                .trim_start_matches(|c: char| c.is_whitespace() || c == '=')
                .trim();
            let v = v.trim_matches('"');
            if !v.is_empty() {
                let p = PathBuf::from(v);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }

    None
}

fn resolve_ams_db_file(project_path: &Path, dbnum: u32) -> Option<PathBuf> {
    let dir = project_path.join("AvevaMarineSample").join("ams000");
    if !dir.exists() {
        return None;
    }

    // 常见命名：ams{dbnum}_0001
    let direct = dir.join(format!("ams{}_0001", dbnum));
    if direct.exists() {
        return Some(direct);
    }

    // 回退：扫描目录找首个匹配前缀的文件（避免过多“特殊情况”）
    let prefix = format!("ams{}_", dbnum);
    let mut cands = fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.starts_with(&prefix))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    cands.sort();
    cands.into_iter().next()
}

async fn connect_surreal_from_env() -> Result<Surreal<Client>> {
    let url = std::env::var("SURREAL_URL").unwrap_or_else(|_| "127.0.0.1:8020".to_string());
    let ns = std::env::var("SURREAL_NS").unwrap_or_else(|_| "1516".to_string());
    let dbname = std::env::var("SURREAL_DB").unwrap_or_else(|_| "AvevaMarineSample".to_string());
    let user = std::env::var("SURREAL_USER").unwrap_or_else(|_| "root".to_string());
    let pass = std::env::var("SURREAL_PASS").unwrap_or_else(|_| "root".to_string());

    let url = url
        .strip_prefix("ws://")
        .or_else(|| url.strip_prefix("wss://"))
        .unwrap_or(url.as_str())
        .to_string();

    let db: Surreal<Client> = Surreal::init();
    db.connect::<Ws>(url).await.context("connect surreal ws")?;
    db.signin(Root {
        username: user,
        password: pass,
    })
    .await
    .context("signin surreal")?;
    db.use_ns(ns).use_db(dbname).await.context("use ns/db")?;
    Ok(db)
}

/// 从 SurrealDB 抽样 PTCD 记录，并在本机 AMS db 文件上回归验证 PTCD 解析结果。
///
/// 依赖：本机启动 SurrealDB `ws://127.0.0.1:8020`，以及存在对应的 `AvevaMarineSample/ams000/ams{dbnum}_xxxx` 文件。
#[tokio::test]
#[ignore]
async fn test_ptcd_parse_matches_surreal_samples() -> Result<()> {
    let project_path = match resolve_project_path() {
        Some(p) => p,
        None => {
            eprintln!(
                "未找到项目路径：请设置环境变量 PDMS_PROJECT_PATH 或配置 DbOption.toml 的 project_path；跳过。"
            );
            return Ok(());
        }
    };

    let db = connect_surreal_from_env().await?;

    // 拉一批样本，后续按 dbnum 分组抽样，避免打开太多 DB 文件。
    let mut resp = db
        .query("SELECT id, noun, dbnum, refno.PTCD AS ptcd FROM pe WHERE refno.PTCD != NONE LIMIT 2000;")
        .await
        .context("query PTCD rows")?;
    let rows: Vec<Object> = resp.take(0).context("decode PTCD rows")?;
    if rows.is_empty() {
        return Err(anyhow!("Surreal 查询未返回任何 PTCD 样本"));
    }

    let mut by_dbnum: BTreeMap<u32, Vec<Object>> = BTreeMap::new();
    for o in rows {
        let Some(Value::Number(Number::Int(dbnum))) = o.get("dbnum") else {
            continue;
        };
        let Ok(dbnum) = u32::try_from(*dbnum) else {
            continue;
        };
        let Some(Value::String(ptcd)) = o.get("ptcd") else {
            continue;
        };
        if ptcd.trim().is_empty() {
            continue;
        }
        by_dbnum.entry(dbnum).or_default().push(o);
    }

    // 每个 dbnum 取 N 条，最多取 M 个 dbnum，保证测试时间可控。
    const PER_DBNUM: usize = 3;
    const MAX_DBNUMS: usize = 6;

    let mut tested = 0usize;
    let mut mismatches = Vec::new();
    let mut missing_files = BTreeSet::new();
    let mut missing_refnos = Vec::new();

    for (dbnum, mut rs) in by_dbnum.into_iter().take(MAX_DBNUMS) {
        let Some(db_file) = resolve_ams_db_file(&project_path, dbnum) else {
            missing_files.insert(dbnum);
            continue;
        };
        if !db_file.exists() {
            missing_files.insert(dbnum);
            continue;
        }

        // 为了稳定起见，按 record id 排序后取前 N
        rs.sort_by(|a, b| {
            let ak = match a.get("id") {
                Some(Value::RecordId(rid)) => match &rid.key {
                    RecordIdKey::String(s) => s.as_str(),
                    _ => "",
                },
                _ => "",
            };
            let bk = match b.get("id") {
                Some(Value::RecordId(rid)) => match &rid.key {
                    RecordIdKey::String(s) => s.as_str(),
                    _ => "",
                },
                _ => "",
            };
            ak.cmp(bk)
        });
        let samples = rs.into_iter().take(PER_DBNUM).collect::<Vec<_>>();

        let mut io = PdmsIO::new("ams", &db_file, true);
        io.open()
            .with_context(|| format!("open db file: {}", db_file.display()))?;
        io.init_ses_range_map()
            .with_context(|| format!("init ses range map: {}", db_file.display()))?;

        for s in samples {
            let expected = match s.get("ptcd") {
                Some(Value::String(s)) => s.clone(),
                _ => String::new(),
            };
            let noun = match s.get("noun") {
                Some(Value::String(s)) => Some(s.clone()),
                _ => None,
            };
            let Some(Value::RecordId(rid)) = s.get("id") else {
                continue;
            };
            let Some(refno) = refno_from_record_id(rid) else {
                continue;
            };

            // 1) 定位 refno
            let Some((_sesno, offset)) = io.search_latest_refno(refno, None) else {
                missing_refnos.push((dbnum, refno.to_string(), noun, expected));
                continue;
            };

            // 2) 解析元素并对齐 PTCD
            let ele = io
                .parse_element(offset)
                .await
                .with_context(|| format!("parse element: dbnum={}, refno={}", dbnum, refno))?;
            let got = ele
                .att_map()
                .get("PTCD")
                .map(|v| v.get_val_as_string())
                .unwrap_or_default();
            let got = normalize_pdms_string(&got);

            if got != expected {
                mismatches.push((dbnum, refno.to_string(), noun, expected, got));
            }
            tested += 1;
        }
    }

    if !missing_files.is_empty() {
        eprintln!("缺失 AMS DB 文件的 dbnum（跳过）：{:?}", missing_files);
    }
    if !missing_refnos.is_empty() {
        eprintln!(
            "在对应 DB 文件中找不到 refno（跳过，可能为跨库/数据不一致）：{}",
            missing_refnos.len()
        );
        for (dbnum, refno, noun, ptcd) in missing_refnos.iter().take(8) {
            eprintln!(
                "  - dbnum={}, refno={}, noun={:?}, ptcd={}",
                dbnum, refno, noun, ptcd
            );
        }
    }

    if tested == 0 {
        return Err(anyhow!(
            "未执行任何样本验证（可能所有 dbnum 都缺文件，或 refno 均无法定位）"
        ));
    }

    if !mismatches.is_empty() {
        for (dbnum, refno, noun, expected, got) in mismatches.iter().take(20) {
            eprintln!(
                "PTCD 不一致: dbnum={}, refno={}, noun={:?}, expected={}, got={}",
                dbnum, refno, noun, expected, got
            );
        }
        return Err(anyhow!(
            "PTCD 回归失败：{} / {} 不一致",
            mismatches.len(),
            tested
        ));
    }

    Ok(())
}
