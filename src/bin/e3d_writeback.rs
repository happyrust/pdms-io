//! e3d-writeback —— SurrealDB → E3D 写回 CLI(specs/004 T302)。
//!
//! 用法:
//!   e3d-writeback plan        --db <file> --exe <schema_dir> --edits <json>
//!   e3d-writeback apply       --db <file> --exe <schema_dir> --edits <json> [--inplace --yes]
//!   e3d-writeback queue-apply --db <file> --exe <schema_dir> --dbnum <n>
//!                             --surreal <url> --ns <ns> --dbname <db> [--inplace --yes]
//!
//! - `plan`:内存内执行(纯函数)只打印报告,不写任何文件(dry-run 预览)。
//! - `apply`:默认写副本 `<db>.e3dout`(原文件零字节变化);`--inplace` 须配 `--yes`
//!   二次确认(契约 E3-A5,宪法 IV)。
//! - `queue-apply`:连接 SurrealDB,按契约 E2 执行 `writeback_queue` 中该 dbnum 的
//!   pending 批次(首败即停,applied 幂等跳过)。
//!
//! edits JSON = `EditBatch`(serde 外部标签枚举),例:
//! ```json
//! { "schema_version": 1,
//!   "edits": [
//!     { "SetPos":  { "refno": [23584, 8], "pos": [1.0, 2.0, 3.0] } },
//!     { "Rename":  { "refno": [23584, 5656], "new_name": "/NEW-NAME" } },
//!     { "Delete":  { "refno": [23584, 9999], "force": false } } ] }
//! ```

use anyhow::{Context, bail};
use pdms_io::writeback_core::{EditBatch, WriteMode, WritebackReport, apply_writeback, apply_writeback_file};

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn opt(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn req(args: &[String], name: &str) -> anyhow::Result<String> {
    opt(args, name).with_context(|| format!("missing required arg {name} <value>"))
}

fn load_batch(path: &str) -> anyhow::Result<EditBatch> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read edits file {path}"))?;
    serde_json::from_str(&text).context("parse edits JSON (EditBatch)")
}

fn print_report(rep: &WritebackReport) {
    println!("new sesno : {}", rep.new_sesno);
    println!(
        "diff      : +{} -{} ~{}",
        rep.diff.added, rep.diff.removed, rep.diff.modified
    );
    for r in &rep.results {
        match r.new_refno {
            Some((a, b)) => println!("  {:<13} ({:#x},{})  -> new refno ({a:#x},{b})", r.kind, r.refno.0, r.refno.1),
            None => println!("  {:<13} ({:#x},{})", r.kind, r.refno.0, r.refno.1),
        }
    }
}

fn write_mode(args: &[String]) -> anyhow::Result<WriteMode> {
    if flag(args, "--inplace") {
        if !flag(args, "--yes") {
            bail!("--inplace 需要 --yes 二次确认(契约 E3-A5;默认写副本不需要)");
        }
        Ok(WriteMode::InPlace { confirmed: true })
    } else {
        Ok(WriteMode::Copy)
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().cloned() else {
        bail!("usage: e3d-writeback <plan|apply|queue-apply> ... (见源码头注)");
    };

    match cmd.as_str() {
        "plan" => {
            let db = req(&args, "--db")?;
            let exe = req(&args, "--exe")?;
            let batch = load_batch(&req(&args, "--edits")?)?;
            let ss = pdms_io::e3d_decode::SchemaSet::load(&exe);
            let bytes = std::fs::read(&db).with_context(|| format!("read {db}"))?;
            let (_out, rep) = apply_writeback(bytes, &ss, &batch.edits)?;
            println!("[plan] dry-run OK — 不落盘,以下为将发生的变更:");
            print_report(&rep);
        }
        "apply" => {
            let db = req(&args, "--db")?;
            let exe = req(&args, "--exe")?;
            let batch = load_batch(&req(&args, "--edits")?)?;
            let mode = write_mode(&args)?;
            let ss = pdms_io::e3d_decode::SchemaSet::load(&exe);
            let (out_path, rep) = apply_writeback_file(&db, &ss, &batch.edits, mode)?;
            println!("[apply] written: {}", out_path.display());
            print_report(&rep);
        }
        "queue-apply" => {
            let db = req(&args, "--db")?;
            let exe = req(&args, "--exe")?;
            let dbnum: i32 = req(&args, "--dbnum")?.parse().context("--dbnum must be i32")?;
            let url = req(&args, "--surreal")?;
            let ns = req(&args, "--ns")?;
            let dbname = req(&args, "--dbname")?;
            let mode = write_mode(&args)?;

            aios_core::SUL_DB.connect(url.as_str()).await.context("connect surreal")?;
            aios_core::use_ns_db_compat(&aios_core::SUL_DB, &ns, &dbname)
                .await
                .map_err(|e| anyhow::anyhow!("use ns/db: {e}"))?;

            let ss = pdms_io::e3d_decode::SchemaSet::load(&exe);
            let rep = pdms_io::surreal_writeback::apply_queue(dbnum, &db, &ss, mode).await?;
            println!(
                "[queue-apply] applied {} | skipped(applied) {} | skipped(failed) {} | new sesnos {:?}",
                rep.applied, rep.skipped_applied, rep.skipped_failed, rep.new_sesnos
            );
        }
        other => bail!("unknown command {other} (plan|apply|queue-apply)"),
    }
    Ok(())
}
