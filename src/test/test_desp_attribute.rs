//! 测试 DESP 属性解析
//!
//! 验证 `ams1112_0001` 中 `refno=17496/171603` 的 `DESP`（显式 DOUBLEVEC）不为空。

use crate::io::PdmsIO;
use aios_core::RefU64;
use parse_pdms_db::parse::parse_raw_ele_data;
use std::path::Path;

/// DESP 属性的 hash 值（db1_hash("DESP")）
const DESP_HASH: i32 = 0x000D20C7;

#[tokio::test]
async fn test_desp_attribute_not_empty() -> anyhow::Result<()> {
    let db_path = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams1112_0001"#;
    if !Path::new(db_path).exists() {
        println!("数据库文件不存在，跳过测试: {}", db_path);
        return Ok(());
    }

    let refno: RefU64 = "17496/171603".into();

    let mut io = PdmsIO::new("ams", db_path, true);
    io.open()?;

    let (sesno, offset) = io
        .search_latest_refno(refno, None)
        .ok_or_else(|| anyhow::anyhow!("无法定位元素 {}", refno))?;
    println!("RefNo {} => sesno={}, offset={:#X}", refno, sesno, offset);

    // 1) 直接从 record bytes 解析（用于排查“读取缺失 vs 解析缺失”）
    let record = io.read_element_record_cached(offset)?;
    let mut input = record.as_slice();
    while input.len() >= 4 && (input[..4] == [0, 0, 0, 0] || input[..4] == [0, 0, 0, 7]) {
        input = &input[4..];
    }

    let hit = input
        .windows(4)
        .filter(|w| *w == DESP_HASH.to_be_bytes().as_slice())
        .count();
    println!("record bytes={}, DESP hash hit={}", input.len(), hit);

    let raw_ele = parse_raw_ele_data(input).map_err(|e| anyhow::anyhow!("{:?}", e))?;
    let merged = raw_ele.whole_attmap.merge();
    let desp = merged.get_f32_vec("DESP").unwrap_or_default();
    println!("[raw] TYPE: {}", merged.get_type());
    println!("[raw] DESP vec len={}", desp.len());
    println!(
        "[raw] DESP head(<=20)={:?}",
        desp.iter().take(20).cloned().collect::<Vec<_>>()
    );
    assert!(!desp.is_empty(), "DESP 不应为空（raw）；refno={}", refno);

    // 2) 走 PdmsIO 高层接口再验证一次
    let ele_data = io.auto_get_element(refno).await?;
    let merged = ele_data.whole_attmap.merge();
    let desp = merged.get_f32_vec("DESP").unwrap_or_default();
    println!("[auto_get_element] TYPE: {}", merged.get_type());
    println!("[auto_get_element] DESP vec len={}", desp.len());
    assert!(
        !desp.is_empty(),
        "DESP 不应为空（auto_get_element）；refno={}",
        refno
    );

    Ok(())
}

