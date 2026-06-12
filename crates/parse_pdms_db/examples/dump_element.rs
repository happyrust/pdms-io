use std::fs;
use std::path::PathBuf;

use aios_core::tool::db_tool::db1_dehash;
use aios_core::{RefU64, get_default_pdms_db_info};
use anyhow::{Context, Result, bail};
use clap::Parser;
use parse_pdms_db::parse::{parse_ele_data_with_info_sync, set_current_element_file_offset};
use parse_pdms_db::refno_index::find_refno_entry;
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(
    about = "Dump one PDMS element from a db file as parsed attribute JSON",
    long_about = "Locate a refno inside a single PDMS db file or parse an already-sliced element record, then print JSON for byte-level parser debugging."
)]
struct Args {
    /// PDMS db file path, for example D:/AVEVA/Projects/.../ams5054_0001
    #[arg(long)]
    db_file: Option<PathBuf>,

    /// Target refno. Accepts 13246_243899, 13246/243899, or =13246/243899.
    #[arg(long)]
    refno: Option<String>,

    /// Already-sliced element record bytes, starting at the element impl_len word.
    #[arg(long)]
    record_file: Option<PathBuf>,

    /// Include the whole element record as a space-separated hex string.
    #[arg(long)]
    raw_hex: bool,

    /// Pretty-print JSON.
    #[arg(long)]
    pretty: bool,
}

#[derive(Debug, Serialize)]
struct ElementDump {
    db_file: Option<String>,
    record_file: Option<String>,
    requested_refno: Option<String>,
    found_refno: String,
    world_refno: Option<String>,
    data_start: usize,
    data_end: usize,
    record_len: usize,
    noun_hash: i32,
    noun_name: String,
    name: String,
    merged_attrs: aios_core::NamedAttrMap,
    implicit_attrs: aios_core::NamedAttrMap,
    explicit_attrs: aios_core::NamedAttrMap,
    raw_record_hex: Option<String>,
}

fn parse_refno(input: &str) -> Result<RefU64> {
    let normalized = input.trim().trim_start_matches('=').replace('_', "/");
    let Some((db, ele)) = normalized.split_once('/') else {
        bail!("invalid refno '{input}', expected db/element or db_element");
    };

    let db = db
        .parse::<u32>()
        .with_context(|| format!("invalid refno db part in '{input}'"))?;
    let ele = ele
        .parse::<u32>()
        .with_context(|| format!("invalid refno element part in '{input}'"))?;
    Ok(RefU64::from_two_nums(db, ele))
}

fn hex_string(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.db_file.is_some() == args.record_file.is_some() {
        bail!("pass exactly one of --db-file or --record-file");
    }
    if args.db_file.is_some() && args.refno.is_none() {
        bail!("--db-file mode requires --refno");
    }

    let mut db_file = None;
    let mut record_file = None;
    let requested_refno: Option<String>;
    let world_refno = None;
    let data_start;
    let data_end;
    let record: Vec<u8>;
    let noun_hash;

    if let Some(path) = args.db_file {
        let refno_arg = args.refno.as_deref().expect("validated above");
        let target_refno = parse_refno(refno_arg)?;
        let bytes = fs::read(&path).with_context(|| format!("read db file {}", path.display()))?;

        let Some(entry) = find_refno_entry(&bytes, target_refno) else {
            bail!(
                "refno {} not found in {}",
                target_refno.to_e3d_id(),
                path.display()
            );
        };
        let pos = entry.pos;
        noun_hash = entry.noun_hash;
        data_start = pos.saturating_sub(4);
        data_end = bytes.len();
        record = bytes
            .get(data_start..)
            .with_context(|| format!("invalid record start {data_start}"))?
            .to_vec();

        db_file = Some(path.display().to_string());
        requested_refno = Some(target_refno.to_e3d_id());
    } else {
        let path = args.record_file.expect("validated above");
        let bytes =
            fs::read(&path).with_context(|| format!("read record file {}", path.display()))?;
        if bytes.len() < 16 {
            bail!("record file too short: {} bytes", bytes.len());
        }
        noun_hash = i32::from_be_bytes(bytes[12..16].try_into().unwrap());
        data_start = 0;
        data_end = bytes.len();
        record = bytes;
        record_file = Some(path.display().to_string());
        requested_refno = args
            .refno
            .as_deref()
            .map(parse_refno)
            .transpose()?
            .map(|r| r.to_e3d_id());
    };

    let db_info = get_default_pdms_db_info();
    set_current_element_file_offset(Some(data_start as u64));
    let element =
        parse_ele_data_with_info_sync(&record, &db_info).context("parse element record")?;
    set_current_element_file_offset(None);

    let dump = ElementDump {
        db_file,
        record_file,
        requested_refno,
        found_refno: element.refno.to_e3d_id(),
        world_refno,
        data_start,
        data_end,
        record_len: record.len(),
        noun_hash,
        noun_name: db1_dehash(noun_hash as u32),
        name: element.name,
        merged_attrs: element.whole_attmap.merge(),
        implicit_attrs: element.whole_attmap.attmap,
        explicit_attrs: element.whole_attmap.explicit_attmap,
        raw_record_hex: args.raw_hex.then(|| hex_string(&record)),
    };

    if args.pretty {
        println!("{}", serde_json::to_string_pretty(&dump)?);
    } else {
        println!("{}", serde_json::to_string(&dump)?);
    }

    Ok(())
}
