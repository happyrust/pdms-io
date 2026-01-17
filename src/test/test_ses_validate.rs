use crate::io::PdmsIO;
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

#[test]
fn test_validate_all_sessions() -> Result<()> {
    let db_path = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams1112_0001"#;
    if !Path::new(db_path).exists() {
        println!("数据库文件不存在，跳过: {}", db_path);
        return Ok(());
    }

    let mut io = PdmsIO::new("ams", db_path, true);
    io.open()?;

    let basic = io.get_page_basic_info()?;
    let mut issues: Vec<String> = Vec::new();

    let mut index_ok = 0usize;
    let mut index_err = 0usize;
    let mut index_zero = 0usize;

    let ses_entries: Vec<(i32, u32)> = io
        .sesno_pgno_map
        .iter()
        .map(|(&sesno, &pgno)| (sesno, pgno))
        .collect();

    for (sesno, pgno) in ses_entries {
        let ses = match io.read_ses_data(pgno).map(|s| s.clone()) {
            Ok(ses) => ses,
            Err(e) => {
                issues.push(format!(
                    "会话读取失败: sesno={}, pgno={}, err={}",
                    sesno, pgno, e
                ));
                continue;
            }
        };

        if ses.sesno != sesno {
            issues.push(format!(
                "会话号不匹配: map_sesno={}, page_sesno={}, pgno={}",
                sesno, ses.sesno, pgno
            ));
        }

        for issue in ses.validate_basic() {
            issues.push(format!(
                "会话字段异常: sesno={}, pgno={}, {}",
                sesno, pgno, issue
            ));
        }

        if ses.end_pgno < pgno {
            issues.push(format!(
                "end_pgno 小于会话页号: sesno={}, pgno={}, end_pgno={}",
                sesno, pgno, ses.end_pgno
            ));
        }

        if ses.last_ses_pageno >= 0 {
            let last_pgno = ses.last_ses_pageno as u32;
            if io.read_ses_data(last_pgno).is_err() {
                issues.push(format!(
                    "last_ses_pageno 无法读取: sesno={}, pgno={}, last_pgno={}",
                    sesno, pgno, last_pgno
                ));
            }
        }

        if ses.index_root_pageno == 0 {
            index_zero += 1;
        } else {
            match io.read_index_data(ses.index_root_pageno) {
                Ok(_) => index_ok += 1,
                Err(e) => {
                    index_err += 1;
                    issues.push(format!(
                        "索引根页读取失败: sesno={}, pgno={}, index_root={}, err={}",
                        sesno, pgno, ses.index_root_pageno, e
                    ));
                }
            }
        }
    }

    let mut chain_count = 0usize;
    let mut visited = HashSet::new();
    let mut cur_pgno = basic.latest_ses_pageno;
    while cur_pgno > 4 {
        if !visited.insert(cur_pgno) {
            issues.push(format!("会话链路出现环: pgno={}", cur_pgno));
            break;
        }
        let ses = io.read_ses_data(cur_pgno)?;
        chain_count += 1;
        if ses.last_ses_pageno < 0 {
            break;
        }
        cur_pgno = ses.last_ses_pageno as u32;
    }

    let total_sessions = io.sesno_pgno_map.len();
    println!(
        "会话验证完成: total={}, chain_count={}, index_ok={}, index_err={}, index_zero={}",
        total_sessions, chain_count, index_ok, index_err, index_zero
    );

    if !issues.is_empty() {
        println!("发现问题数量: {}", issues.len());
        for (i, issue) in issues.iter().take(50).enumerate() {
            println!("  {}. {}", i + 1, issue);
        }
        if issues.len() > 50 {
            println!("  ... 其余 {} 条已省略", issues.len() - 50);
        }
    }

    Ok(())
}
