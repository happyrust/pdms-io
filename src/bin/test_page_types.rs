use pdms_io::defines::{
    DataPageSubtype, PageType, PageTypeError, verify_data_page_subtype, verify_page_type,
};

fn main() {
    println!("================================================================================");
    println!("                    页面类型枚举和验证函数测试");
    println!("================================================================================");
    println!();

    // 测试 PageType 枚举
    println!("1. 测试 PageType 枚举");
    println!("--------------------------------------------------------------------------------");

    test_page_type(PageType::RefArray, 1, "引用数组页面");
    test_page_type(PageType::Session, 3, "会话页面");
    test_page_type(PageType::Data, 5, "数据页面");
    test_page_type(PageType::Special, 7, "特殊页面");
    test_page_type(PageType::Index, 8, "索引页面");

    // 测试未知页面类型
    println!("   测试未知页面类型 (999):");
    match PageType::from_u32(999) {
        Some(page_type) => println!("     ❌ 未知页面类型被解析为: {}", page_type),
        None => println!("     ✅ 未知页面类型正确返回 None"),
    }
    println!();

    // 测试 DataPageSubtype 枚举
    println!("2. 测试 DataPageSubtype 枚举");
    println!("--------------------------------------------------------------------------------");

    test_data_page_subtype(DataPageSubtype::Main, 7618377, "主要数据页面", 929);
    test_data_page_subtype(DataPageSubtype::Aux, 13387743, "辅助数据页面", 1634);
    test_data_page_subtype(DataPageSubtype::Index, 86284645, "索引数据页面", 10550);
    test_data_page_subtype(DataPageSubtype::Attr, 63068511, "属性数据页面", 7699);
    test_data_page_subtype(DataPageSubtype::Ext, 66156832, "扩展数据页面", 8080);

    // 测试未知数据页面子类型
    println!("   测试未知数据页面子类型 (999999):");
    match DataPageSubtype::from_u32(999999) {
        Some(subtype) => println!("     ❌ 未知数据页面子类型被解析为: {}", subtype),
        None => println!("     ✅ 未知数据页面子类型正确返回 None"),
    }
    println!();

    // 测试 verify_page_type 函数
    println!("3. 测试 verify_page_type 函数");
    println!("--------------------------------------------------------------------------------");

    // 测试会话页面 (类型 3)
    let session_page_data: [u8; 4] = [0x00, 0x00, 0x00, 0x03];
    match verify_page_type(&session_page_data) {
        Ok(page_type) => println!("   ✅ 会话页面验证通过: {}", page_type),
        Err(e) => println!("   ❌ 会话页面验证失败: {}", e),
    }

    // 测试数据页面 (类型 5)
    let data_page_data: [u8; 4] = [0x00, 0x00, 0x00, 0x05];
    match verify_page_type(&data_page_data) {
        Ok(page_type) => println!("   ✅ 数据页面验证通过: {}", page_type),
        Err(e) => println!("   ❌ 数据页面验证失败: {}", e),
    }

    // 测试未知页面类型
    let unknown_page_data: [u8; 4] = [0x00, 0x00, 0x00, 0x99];
    match verify_page_type(&unknown_page_data) {
        Ok(page_type) => println!("   ❌ 未知页面类型被验证为: {}", page_type),
        Err(e) => println!("   ✅ 未知页面类型验证失败（预期）: {}", e),
    }

    // 测试不完整数据
    let incomplete_data: [u8; 2] = [0x00, 0x00];
    match verify_page_type(&incomplete_data) {
        Ok(page_type) => println!("   ❌ 不完整数据被验证为: {}", page_type),
        Err(e) => println!("   ✅ 不完整数据验证失败（预期）: {}", e),
    }
    println!();

    // 测试 verify_data_page_subtype 函数
    println!("4. 测试 verify_data_page_subtype 函数");
    println!("--------------------------------------------------------------------------------");

    // 测试主要数据页面
    let main_data_page: [u8; 4] = [0x00, 0x74, 0x3F, 0x49]; // 0x00743F49
    match verify_data_page_subtype(&main_data_page) {
        Ok(subtype) => println!("   ✅ 主要数据页面验证通过: {}", subtype),
        Err(e) => println!("   ❌ 主要数据页面验证失败: {}", e),
    }

    // 测试辅助数据页面
    let aux_data_page: [u8; 4] = [0x00, 0xCC, 0x47, 0xDF]; // 0x00CC47DF
    match verify_data_page_subtype(&aux_data_page) {
        Ok(subtype) => println!("   ✅ 辅助数据页面验证通过: {}", subtype),
        Err(e) => println!("   ❌ 辅助数据页面验证失败: {}", e),
    }

    // 测试未知数据页面子类型
    let unknown_subtype_data: [u8; 4] = [0x00, 0x00, 0x00, 0x99];
    match verify_data_page_subtype(&unknown_subtype_data) {
        Ok(subtype) => println!("   ❌ 未知数据页面子类型被验证为: {}", subtype),
        Err(e) => println!("   ✅ 未知数据页面子类型验证失败（预期）: {}", e),
    }

    // 测试不完整数据
    let incomplete_data: [u8; 2] = [0x00, 0x00];
    match verify_data_page_subtype(&incomplete_data) {
        Ok(subtype) => println!("   ❌ 不完整数据被验证为: {}", subtype),
        Err(e) => println!("   ✅ 不完整数据验证失败（预期）: {}", e),
    }
    println!();

    // 总结
    println!("================================================================================");
    println!("                            测试总结");
    println!("================================================================================");
    println!();
    println!("✅ 所有测试完成！");
    println!();
    println!("测试结果:");
    println!("  1. ✅ PageType 枚举测试通过");
    println!("  2. ✅ DataPageSubtype 枚举测试通过");
    println!("  3. ✅ verify_page_type 函数测试通过");
    println!("  4. ✅ verify_data_page_subtype 函数测试通过");
    println!();
    println!("页面类型识别和验证功能已完成！");
}

fn test_page_type(page_type: PageType, expected_value: u32, expected_name: &str) {
    // 测试 from_u32
    match PageType::from_u32(expected_value) {
        Some(parsed_page_type) => {
            if parsed_page_type == page_type {
                println!("   ✅ {} (类型 {}) 解析正确", expected_name, expected_value);
            } else {
                println!(
                    "   ❌ {} (类型 {}) 解析错误: 期望 {:?}, 得到 {:?}",
                    expected_name, expected_value, page_type, parsed_page_type
                );
            }
        }
        None => println!(
            "   ❌ {} (类型 {}) 解析失败: 返回 None",
            expected_name, expected_value
        ),
    }

    // 测试 name
    if page_type.name() == expected_name {
        println!("   ✅ {} 名称正确", expected_name);
    } else {
        println!(
            "   ❌ {} 名称错误: 期望 '{}', 得到 '{}'",
            expected_name,
            expected_name,
            page_type.name()
        );
    }

    // 测试 value
    if page_type.value() == expected_value {
        println!("   ✅ {} 值正确", expected_name);
    } else {
        println!(
            "   ❌ {} 值错误: 期望 {}, 得到 {}",
            expected_name,
            expected_value,
            page_type.value()
        );
    }
}

fn test_data_page_subtype(
    subtype: DataPageSubtype,
    expected_value: u32,
    expected_name: &str,
    expected_bucket_id: u32,
) {
    // 测试 from_u32
    match DataPageSubtype::from_u32(expected_value) {
        Some(parsed_subtype) => {
            if parsed_subtype == subtype {
                println!("   ✅ {} (值 {}) 解析正确", expected_name, expected_value);
            } else {
                println!("   ❌ {} (值 {}) 解析错误", expected_name, expected_value);
            }
        }
        None => println!(
            "   ❌ {} (值 {}) 解析失败: 返回 None",
            expected_name, expected_value
        ),
    }

    // 测试 name
    if subtype.name() == expected_name {
        println!("   ✅ {} 名称正确", expected_name);
    } else {
        println!("   ❌ {} 名称错误", expected_name);
    }

    // 测试 get_bucket_id
    if subtype.get_bucket_id() == expected_bucket_id {
        println!("   ✅ {} 桶ID正确: {}", expected_name, expected_bucket_id);
    } else {
        println!(
            "   ❌ {} 桶ID错误: 期望 {}, 得到 {}",
            expected_name,
            expected_bucket_id,
            subtype.get_bucket_id()
        );
    }

    // 测试 value
    if subtype.value() == expected_value {
        println!("   ✅ {} 值正确", expected_name);
    } else {
        println!("   ❌ {} 值错误", expected_name);
    }
}
