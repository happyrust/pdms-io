use aios_core::RefU64;
use parse_pdms_db::parse::{collect_explict_data, parse_raw_ele_data};
use parse_pdms_db::parser::combinator::collect_segmented_payload;
use pdms_io::io::PdmsIO;
use pdms_io::test::resolve_test_db_path;
use std::path::Path;

const DESP_HASH: i32 = 0x000D20C7; // 860359

#[tokio::test]
async fn test_ams1112_0001_desp_not_empty() -> anyhow::Result<()> {
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

    let refno: RefU64 = "17496/171603".into();

    let mut io = PdmsIO::new("ams", &db_path, true);
    io.open()?;

    // 额外：对比“原始解析(不 refine)”与“当前 PdmsIO 解析(可能 refine)”的差异，
    // 以判断 DESP 缺失是否来自 refine/filter 逻辑。
    if let Some((_sesno, offset)) = io.search_latest_refno(refno, None) {
        let record = io.read_element_record_cached(offset)?;
        let desp_bytes = DESP_HASH.to_be_bytes();
        let mut hit = 0usize;
        for i in 0..record.len().saturating_sub(4) {
            if record[i..i + 4] == desp_bytes {
                hit += 1;
                if hit <= 5 {
                    println!("[RAW-BYTES] 找到 DESP hash at +0x{:X}", i);
                    let start = i.saturating_sub(32);
                    let end = (i + 96).min(record.len());
                    for (j, chunk) in record[start..end].chunks(16).enumerate() {
                        let hex: String = chunk.iter().map(|b| format!("{:02X} ", b)).collect();
                        println!("  {:#06X}: {}", start + j * 16, hex);
                    }
                }
            }
        }
        println!(
            "[RAW-BYTES] record.len()={}, DESP hash hit={}",
            record.len(),
            hit
        );

        let mut input = record.as_slice();
        let mut prefix = 0usize;
        while input.len() >= 4 && (input[..4] == [0, 0, 0, 0] || input[..4] == [0, 0, 0, 7]) {
            input = &input[4..];
            prefix += 4;
        }

        // 粗略还原 parse_raw_ele_data_with_info 的 explicit_start 计算，用于判断 DESP hash 落在哪个区域。
        if input.len() >= 24 {
            let impl_len_words = i32::from_be_bytes(input[0..4].try_into().unwrap());
            let mut actual_impl_len = (impl_len_words as usize) * 4;
            while actual_impl_len + 4 <= input.len() {
                let w = &input[actual_impl_len..actual_impl_len + 4];
                if w == [0, 0, 0, 0] || w == [0, 0, 0, 7] {
                    actual_impl_len += 4;
                } else {
                    break;
                }
            }

            let mut memb_bytes_len = 0usize;
            if actual_impl_len + 12 <= input.len() {
                let membs_data = &input[actual_impl_len..];
                if membs_data.len() >= 4
                    && &membs_data[0..2] == [0x00, 0x02].as_slice()
                    && RefU64::from(&membs_data[4..12]) == refno
                {
                    let len_words =
                        u16::from_be_bytes(membs_data[2..4].try_into().unwrap()) as usize;
                    let declared_bytes = len_words * 4;
                    if let Ok((rest, _payload)) =
                        collect_segmented_payload(membs_data, declared_bytes, 0x02)
                    {
                        memb_bytes_len = membs_data.len().saturating_sub(rest.len());
                    }
                }
            }
            let explicit_start = actual_impl_len + memb_bytes_len;
            let explicit_data = if explicit_start <= input.len() {
                &input[explicit_start..]
            } else {
                &[][..]
            };
            let collected = collect_explict_data(explicit_data, refno);
            let mut desp_in_collected = 0usize;
            for w in collected.windows(4) {
                if w == desp_bytes.as_slice() {
                    desp_in_collected += 1;
                }
            }
            println!(
                "[RAW-BYTES] explicit_data.len()=0x{:X}, collect_explict_data.len()=0x{:X}, DESP hit in collected={}",
                explicit_data.len(),
                collected.len(),
                desp_in_collected
            );

            if let Some(pos_in_record) = record.windows(4).position(|w| w == desp_bytes.as_slice())
            {
                let pos_in_input = pos_in_record.saturating_sub(prefix);
                println!(
                    "[RAW-BYTES] prefix=0x{:X}, actual_impl_len=0x{:X}, memb_bytes_len=0x{:X}, explicit_start=0x{:X}, DESP@record+0x{:X} => input+0x{:X}",
                    prefix,
                    actual_impl_len,
                    memb_bytes_len,
                    explicit_start,
                    pos_in_record,
                    pos_in_input
                );
            }
        }

        match parse_raw_ele_data(input) {
            Ok(raw_ele) => {
                // 注意：EleData::att_map() 仅返回“隐式属性 attmap”，显式属性在 explicit_attmap 里。
                // DESP 在 E3D/PDMS 中通常是显式 DOUBLEVEC（几何参数向量），不是字符串。
                let merged = raw_ele.whole_attmap.merge();
                if let Some(desp) = merged.get_f32_vec("DESP") {
                    println!("[RAW] DESP vec len={}", desp.len());
                    let head = desp.iter().take(20).cloned().collect::<Vec<_>>();
                    println!("[RAW] DESP head(<=20)={:?}", head);
                    assert!(!desp.is_empty(), "期望 DESP 向量不为空，但解析结果为空 vec");
                } else {
                    let keys: Vec<String> = merged.map.keys().cloned().collect();
                    println!("[RAW] 未找到 DESP（显式/合并后属性）；keys={:?}", keys);
                }
            }
            Err(e) => {
                println!("[RAW] parse_raw_ele_data 失败: {:?}", e);
            }
        }
    }

    let ele_data = io.auto_get_element(refno).await?;
    let merged = ele_data.whole_attmap.merge();
    let desp = merged.get_f32_vec("DESP").unwrap_or_default();

    println!("RefNo: {}", refno);
    println!("TYPE: {}", merged.get_type());
    println!("DESP vec len={}", desp.len());
    let head = desp.iter().take(40).cloned().collect::<Vec<_>>();
    println!("DESP head(<=40)={:?}", head);

    assert!(
        !desp.is_empty(),
        "期望 DESP 向量不为空，但未找到 DESP 或其值为空 vec"
    );

    Ok(())
}
