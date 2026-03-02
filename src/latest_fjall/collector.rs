use crate::latest_fjall::index_scan::LatestRefLoc;
use crate::latest_fjall::model::LatestElementRecord;
use crate::latest_fjall::model::ParseStats;
use crate::io::PdmsIO;
use aios_core::RefU64;
use anyhow::Result;
use parse_pdms_db::parse::EleData;
use std::collections::{BTreeMap, HashMap};

fn build_latest_record(refno: RefU64, loc: &LatestRefLoc, ele: &EleData) -> LatestElementRecord {
    let mut attrs = BTreeMap::new();

    for (key, value) in ele.att_map().iter() {
        attrs.insert(key.clone(), value.get_val_as_string());
    }
    for (key, value) in ele.explicit_attmap().iter() {
        attrs.insert(key.clone(), value.get_val_as_string());
    }
    for uda in ele.uda_atts() {
        attrs.insert(format!("UDA:{}", uda.name), uda.value.get_val_as_string());
    }

    let entity_type = ele.att_map().get_type();
    let name = attrs
        .get("NAME")
        .cloned()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            if ele.name.is_empty() {
                refno.to_string()
            } else {
                ele.name.clone()
            }
        });
    let owner = attrs
        .get("OWNER")
        .cloned()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| ele.owner.to_string());

    attrs
        .entry("TYPE".to_string())
        .or_insert_with(|| entity_type.clone());
    attrs.entry("NAME".to_string()).or_insert_with(|| name.clone());
    attrs
        .entry("OWNER".to_string())
        .or_insert_with(|| owner.clone());

    LatestElementRecord {
        refno: refno.to_string(),
        sesno: loc.sesno,
        entity_type,
        name,
        owner,
        attrs,
        children: ele.children.iter().map(|r| r.to_string()).collect(),
        source_pgno: loc.pgno,
        source_offset: loc.offset,
    }
}

pub async fn collect_latest_records_in_batches<F>(
    io: &mut PdmsIO,
    latest_locs: &HashMap<RefU64, LatestRefLoc>,
    batch_size: usize,
    mut on_batch: F,
) -> Result<ParseStats>
where
    F: FnMut(&[LatestElementRecord]) -> Result<()>,
{
    let mut stats = ParseStats::default();
    stats.total_candidates = latest_locs.len();

    let mut ordered: Vec<(RefU64, LatestRefLoc)> =
        latest_locs.iter().map(|(refno, loc)| (*refno, loc.clone())).collect();
    ordered.sort_by_key(|(refno, _)| (refno.get_0(), refno.get_1()));

    let mut buffer: Vec<LatestElementRecord> = Vec::with_capacity(batch_size.max(1));

    for (refno, loc) in ordered {
        match io.parse_element(loc.offset).await {
            Ok(ele) => {
                buffer.push(build_latest_record(refno, &loc, &ele));
                stats.parsed_ok += 1;
            }
            Err(err) => {
                stats.parsed_failed += 1;
                if io.detail {
                    eprintln!(
                        "latest_fjall: parse failed refno={} sesno={} offset={} err={}",
                        refno, loc.sesno, loc.offset, err
                    );
                }
            }
        }

        if buffer.len() >= batch_size.max(1) {
            on_batch(&buffer)?;
            stats.flush_batches += 1;
            stats.flushed_records += buffer.len();
            buffer.clear();
        }
    }

    if !buffer.is_empty() {
        on_batch(&buffer)?;
        stats.flush_batches += 1;
        stats.flushed_records += buffer.len();
    }

    Ok(stats)
}
