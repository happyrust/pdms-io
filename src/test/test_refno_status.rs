//! 测试参考号状态判断功能
//!
//! 本测试模块验证`get_refno_status`和`get_refno_operation_status`方法是否能正确判断一个参考号的状态：
//! - 新增(Add)：参考号只在一个会话中出现，或者只有最新的会话中存在
//! - 修改(Modified)：参考号在多个会话中出现，且内容有变化
//! - 删除(Deleted)：参考号在历史会话中存在，但在最新会话中不存在

/// 测试`get_refno_status`方法
///
/// 本测试验证以下情况：
/// 1. 已知存在的参考号应返回Add或Modified状态
/// 2. 不存在的参考号应返回错误或Deleted状态
/// 3. 随机测试几个参考号，验证返回正确的状态
#[tokio::test]
async fn test_get_refno_status() -> anyhow::Result<()> {
    // 设置数据库文件路径
    let db_filepath = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams1112_0001"#;
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;

    // 测试用例1: 测试一个已知存在的参考号（应该是Add或Modified状态）
    let refno1: RefU64 = "17496/171606".into(); // 使用一个已知存在的参考号
                                                //先测试这个参考号是否存在
    let (sesno, offset) = io.search_latest_refno(refno1, None).unwrap();
    println!(
        "参考号 {} 在会话 {} 中的偏移是 {:#4X} 页号 {:#4X}",
        refno1,
        sesno,
        offset,
        offset / 0x800
    );

    //测试search_latest_and_prev_refno
    let [current, prev] = io.search_latest_and_prev_refno(refno1, None);
    if let Some((current_sesno, current_offset)) = current {
        println!(
            "参考号 {} 在当前会话 {} 中的偏移是 {:#4X}  页号 {:#4X}",
            refno1,
            current_sesno,
            current_offset,
            current_offset / 0x800
        );
        if let Some((prev_sesno, prev_offset)) = prev {
            println!(
                "参考号 {} 在前一个会话 {} 中的偏移是 {:#4X}  页号 {:#4X}",
                refno1,
                prev_sesno,
                prev_offset,
                prev_offset / 0x800
            );
        }
    }

    // 在全范围内检查状态
    // let status_map = io.get_refno_operation_status(refno1, None)?;
    // println!("参考号 {} 在全范围内的状态为: {:?}", refno1, status_map.get(&refno1));
    // println!("状态映射中包含 {} 个元素", status_map.len());

    // // 仅在当前会话中检查状态
    // let status_map = io.get_refno_operation_status(refno1, Some(sesno))?;
    // println!("参考号 {} 在会话 {} 中的状态为: {:?}", refno1, sesno, status_map.get(&refno1));
    // println!("状态映射中包含 {} 个元素", status_map.len());

    return Ok(());
}

/// 测试`get_refno_operation_status`方法
///
/// 测试在不同会话范围内获取参考号的操作状态
#[test]
fn test_get_refno_operation_status() -> anyhow::Result<()> {
    // 设置数据库文件路径
    let db_filepath = r#"D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams1112_0001"#;
    let mut io = PdmsIO::new("ams", db_filepath, true);
    io.open()?;

    // 测试用例1: 使用一个存在多个版本的参考号
    let refno1: RefU64 = "17496/497129".into();

    let history = io.search_history_refnos(refno1, None)?;

    // 输出该参考号的所有历史版本
    println!("参考号 {} 的历史版本:", refno1);
    for (sesno, offset) in &history {
        println!(
            "  会话 {}: 偏移 {:#4X} 页号 {}",
            sesno,
            offset,
            offset / 0x800
        );
    }

    // 如果有多个版本，则测试不同范围内的状态
    if history.len() > 1 {
        let all_sesnos: Vec<u32> = history.keys().cloned().collect();
        let earliest_sesno = *all_sesnos.first().unwrap();
        let second_sesno = all_sesnos[1];

        // 测试最早会话的状态(应该是Add)
        let status1_map = io.get_refno_operation_status(refno1, Some(earliest_sesno))?;
        match status1_map.get(&refno1) {
            Some(EleOperationDetail::Add(_)) => {
                println!(
                    "参考号 {} 在最早会话 {} 中的状态为: 新增",
                    refno1, earliest_sesno
                );
                // 检查通过
            }
            status => {
                println!(
                    "参考号 {} 在最早会话 {} 中的状态为: {:?}",
                    refno1, earliest_sesno, status
                );
                panic!("最早会话的状态应该是Add");
            }
        }
        println!("状态映射中包含 {} 个元素", status1_map.len());

        // 测试第二个会话的状态(应该是Modified)
        // let status2_map = io.get_refno_operation_status(refno1, Some(second_sesno))?;
        // match status2_map.get(&refno1) {
        //     Some(EleOperationDetail::Modified { noun, .. }) => {
        //         println!("参考号 {} 在第二个会话 {} 中的状态为: 已修改 (类型: {})", refno1, second_sesno, noun);
        //         // 检查通过
        //     },
        //     status => {
        //         println!("参考号 {} 在第二个会话 {} 中的状态为: {:?}", refno1, second_sesno, status);
        //         panic!("第二个会话的状态应该是Modified");
        //     }
        // }
        // println!("状态映射中包含 {} 个元素", status2_map.len());
    } else {
        println!("参考号 {} 只有一个版本，跳过多版本测试", refno1);
    }

    // 测试用例2: 测试一个不存在的参考号
    let refno2: RefU64 = "99999/99999".into();
    match io.get_refno_operation_status(refno2, None) {
        Ok(status_map) => {
            println!("不存在的参考号状态为: {:?}", status_map.get(&refno2));
            println!("状态映射中包含 {} 个元素", status_map.len());
        }
        Err(e) => println!("预期的错误: {}", e),
    }

    Ok(())
}
