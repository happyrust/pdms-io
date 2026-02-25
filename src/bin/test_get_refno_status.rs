use aios_core::NamedAttrValue;
use aios_core::pdms_types::{EleOperation, RefU64};
use anyhow::Result;
use pdms_io::io::{EleOperationDetail, PdmsIO};
use std::env;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    // 获取命令行参数
    let args: Vec<String> = env::args().collect();

    // 设置默认参数
    let mut db_path = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams1112_0001"#;
    let mut refno_str = "17496/497128";
    let mut min_sesno = 0;
    let mut max_sesno = 9999;

    // 处理命令行参数
    if args.len() > 1 {
        db_path = &args[1];
    }

    if args.len() > 2 {
        refno_str = &args[2];
    }

    if args.len() > 3 {
        min_sesno = args[3].parse().unwrap_or(0);
    }

    if args.len() > 4 {
        max_sesno = args[4].parse().unwrap_or(9999);
    }

    // 解析参考号
    let refno = match RefU64::try_from(refno_str) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("无效的参考号格式 '{}': {}", refno_str, e);
            return Ok(());
        }
    };

    println!("测试参考号 {} 的状态", refno);

    // 创建并打开数据库
    let mut io = PdmsIO::new("test", db_path, true);
    io.open()?;

    // 首先检查参考号是否存在
    println!("检查参考号是否存在...");
    match io.search_latest_refno(refno, None) {
        Some((sesno, offset)) => {
            println!("引用号 {} 在会话 {} 的偏移量: {}", refno, sesno, offset);
        }
        None => {
            println!("引用号 {} 不存在", refno);
            return Ok(());
        }
    }

    //测试search_latest_and_prev_refno
    let [current, prev] = io.search_latest_and_prev_refno(refno, None);
    if let Some((current_sesno, current_offset)) = current {
        println!(
            "参考号 {} 在当前会话 {} 中的偏移是 {:#4X}",
            refno, current_sesno, current_offset
        );
        if let Some((prev_sesno, prev_offset)) = prev {
            println!(
                "参考号 {} 在前一个会话 {} 中的偏移是 {:#4X}",
                refno, prev_sesno, prev_offset
            );
        }
    }

    println!("搜索参考号的历史记录...");
    match io.search_history_refnos(refno, None) {
        Ok(history) => {
            println!("找到 {} 条历史记录:", history.len());
            for (i, (sesno, offset)) in history.iter().enumerate() {
                println!(
                    "  历史记录 {}: 会话号={}, 偏移={:#4X}",
                    i + 1,
                    sesno,
                    offset
                );
            }
        }
        Err(e) => {
            println!("搜索历史记录失败: {}", e);
        }
    }

    // 测试获取参考号操作状态功能
    println!("\n测试参考号操作状态判断功能...");
    // println!("会话范围: {} 到 {}", min_sesno, max_sesno);

    let start = Instant::now();
    match io.get_refno_operation_status(refno, None) {
        Ok(status_map) => {
            let elapsed = start.elapsed();

            let refno_status = status_map.get(&refno).cloned();
            match &refno_status {
                Some(EleOperationDetail::Add(ele_data)) => {
                    println!(
                        "参考号 {} 在会话范围 {} 到 {} 的操作状态为: 新增",
                        refno, min_sesno, max_sesno
                    );
                    println!("解释: 该参考号在此会话范围内是新增的");
                    println!("元素属性数量: {}", ele_data.att_map().len());
                }
                Some(EleOperationDetail::Modified(modified_ele)) => {
                    println!("发现修改操作：");
                    println!("类型：{}", modified_ele.noun);
                    println!("新增属性：{:?}", modified_ele.added_attrs.keys());
                    println!("删除属性：{:?}", modified_ele.deleted_attrs.keys());
                    println!("修改属性：{:?}", modified_ele.modified_attrs.keys());
                }
                Some(EleOperationDetail::Deleted) => {
                    println!(
                        "参考号 {} 在会话范围 {} 到 {} 的操作状态为: 已删除",
                        refno, min_sesno, max_sesno
                    );
                    println!("解释: 该参考号在此会话范围内被删除了");
                }
                Some(EleOperationDetail::None) => {
                    println!(
                        "参考号 {} 在会话范围 {} 到 {} 的操作状态为: 无操作",
                        refno, min_sesno, max_sesno
                    );
                    println!("解释: 该参考号在此会话范围内没有操作记录");
                }
                None => {
                    println!("参考号 {} 在状态映射中不存在", refno);
                }
            }

            println!("状态映射中包含 {} 个元素", status_map.len());

            // 打印子元素的状态
            if status_map.len() > 1 {
                println!("子元素状态列表:");
                for (child_refno, operation) in status_map.iter() {
                    if *child_refno != refno {
                        println!(" - 子元素 {} 的状态: {:?}", child_refno, operation);
                    }
                }
            }

            println!("状态判断耗时: {:?}", elapsed);
        }
        Err(e) => {
            println!("获取操作状态失败: {}", e);
        }
    }

    Ok(())
}

//如何指定一个参考号，就能找到它的上一个版本，仅仅是通过索引
//17496/497128
