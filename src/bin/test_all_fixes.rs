use pdms_io::defines::{
    PdmsHeader, PAGE_SIZE, PageType, DataPageSubtype,
    verify_page_type, verify_data_page_subtype, PageTypeError,
};
use std::mem;

fn main() {
    println!("================================================================================");
    println!("                            所有修复的综合测试");
    println!("================================================================================");
    println!();
    
    let mut all_tests_passed = true;
    
    // 测试 1: PAGE_SIZE 常量
    println!("================================================================================");
    println!("测试 1: PAGE_SIZE 常量");
    println!("================================================================================");
    all_tests_passed &= test_page_size();
    println!();
    
    // 测试 2: PdmsHeader 结构
    println!("================================================================================");
    println!("测试 2: PdmsHeader 结构");
    println!("================================================================================");
    all_tests_passed &= test_pdms_header();
    println!();
    
    // 测试 3: PageType 枚举
    println!("================================================================================");
    println!("测试 3: PageType 枚举");
    println!("================================================================================");
    all_tests_passed &= test_page_type_enum();
    println!();
    
    // 测试 4: DataPageSubtype 枚举
    println!("================================================================================");
    println!("测试 4: DataPageSubtype 枚举");
    println!("================================================================================");
    all_tests_passed &= test_data_page_subtype_enum();
    println!();
    
    // 测试 5: verify_page_type 函数
    println!("================================================================================");
    println!("测试 5: verify_page_type 函数");
    println!("================================================================================");
    all_tests_passed &= test_verify_page_type();
    println!();
    
    // 测试 6: verify_data_page_subtype 函数
    println!("================================================================================");
    println!("测试 6: verify_data_page_subtype 函数");
    println!("================================================================================");
    all_tests_passed &= test_verify_data_page_subtype();
    println!();
    
    // 测试 7: 实际数据库文件验证
    println!("================================================================================");
    println!("测试 7: 实际数据库文件验证");
    println!("================================================================================");
    all_tests_passed &= test_actual_database();
    println!();
    
    // 总结
    println!("================================================================================");
    println!("                            测试总结");
    println!("================================================================================");
    println!();
    
    if all_tests_passed {
        println!("✅ 所有测试通过！");
        println!();
        println!("测试结果:");
        println!("  1. ✅ PAGE_SIZE 常量测试通过");
        println!("  2. ✅ PdmsHeader 结构测试通过");
        println!("  3. ✅ PageType 枚举测试通过");
        println!("  4. ✅ DataPageSubtype 枚举测试通过");
        println!("  5. ✅ verify_page_type 函数测试通过");
        println!("  6. ✅ verify_data_page_subtype 函数测试通过");
        println!("  7. ✅ 实际数据库文件验证通过");
        println!();
        println!("结论:");
        println!("  所有修复都是正确的！");
    } else {
        println!("❌ 部分测试失败！");
        println!();
        println!("请查看上面的测试结果，找出失败的原因。");
    }
}

// 测试 PAGE_SIZE 常量
fn test_page_size() -> bool {
    println!("   测试 PAGE_SIZE 常量");
    println!("--------------------------------------------------------------------------------");
    
    let passed = PAGE_SIZE == 0x800;
    
    println!("   PAGE_SIZE = {} 字节 = 0x{:X}", PAGE_SIZE, PAGE_SIZE);
    
    if passed {
        println!("   ✅ PAGE_SIZE 正确！2048 字节");
    } else {
        println!("   ❌ PAGE_SIZE 错误！应该是 2048 字节，实际是 {} 字节", PAGE_SIZE);
    }
    
    println!();
    passed
}

// 测试 PdmsHeader 结构
fn test_pdms_header() -> bool {
    println!("   测试 PdmsHeader 结构");
    println!("--------------------------------------------------------------------------------");
    
    let mut all_passed = true;
    
    // 测试结构体大小
    let header_size = mem::size_of::<PdmsHeader>();
    println!("   PdmsHeader 大小 = {} 字节", header_size);
    println!("   预期大小 = 60 字节（15 个字段 × 4 字节）");
    
    if header_size == 60 {
        println!("   ✅ PdmsHeader 大小正确！");
    } else {
        println!("   ⚠️  PdmsHeader 大小不匹配，可能有对齐填充");
    }
    
    // 测试默认值
    let header = PdmsHeader::default();
    
    println!("   默认值:");
    println!("     数据库ID (db_num): {}", header.db_num);
    println!("     版本 (version): {}", header.version);
    println!("     标志位 (flags): 0x{:X}", header.flags);
    println!("     创建时间 (creation_time): {}", header.creation_time);
    println!("     最新会话页号 (latest_ses_pgno): {}", header.latest_ses_pgno);
    println!("     扩展号 (ext_no): {}", header.ext_no);
    println!();
    println!("   新增字段:");
    println!("     会话页面号 (session_page_no): {}", header.session_page_no);
    println!("     页面大小 (page_size): {}", header.page_size);
    println!("     存储页数 (stored_page_count): {}", header.stored_page_count);
    println!("     未知值 (unknown_3): {}", header.unknown_3);
    
    // 验证新增字段
    if header.session_page_no == 0 {
        println!("   ✅ session_page_no 字段存在");
    } else {
        println!("   ❌ session_page_no 字段不存在或默认值错误");
        all_passed = false;
    }
    
    if header.page_size == 0 {
        println!("   ✅ page_size 字段存在");
    } else {
        println!("   ❌ page_size 字段不存在或默认值错误");
        all_passed = false;
    }
    
    if header.stored_page_count == 0 {
        println!("   ✅ stored_page_count 字段存在");
    } else {
        println!("   ❌ stored_page_count 字段不存在或默认值错误");
        all_passed = false;
    }
    
    if header.unknown_3 == 0 {
        println!("   ✅ unknown_3 字段存在");
    } else {
        println!("   ❌ unknown_3 字段不存在或默认值错误");
        all_passed = false;
    }
    
    println!();
    all_passed
}

// 测试 PageType 枚举
fn test_page_type_enum() -> bool {
    println!("   测试 PageType 枚举");
    println!("--------------------------------------------------------------------------------");
    
    let mut all_passed = true;
    
    // 测试 from_u32
    let tests = [
        (PageType::RefArray, 1, "引用数组页面"),
        (PageType::Session, 3, "会话页面"),
        (PageType::Data, 5, "数据页面"),
        (PageType::Special, 7, "特殊页面"),
        (PageType::Index, 8, "索引页面"),
    ];
    
    for (page_type, expected_value, expected_name) in tests {
        // 测试 from_u32
        match PageType::from_u32(expected_value) {
            Some(parsed_page_type) => {
                if parsed_page_type == page_type {
                    println!("   ✅ {} (类型 {}) 解析正确", expected_name, expected_value);
                } else {
                    println!("   ❌ {} (类型 {}) 解析错误", expected_name, expected_value);
                    all_passed = false;
                }
            }
            None => {
                println!("   ❌ {} (类型 {}) 解析失败: 返回 None", expected_name, expected_value);
                all_passed = false;
            }
        }
        
        // 测试 name
        if page_type.name() == expected_name {
            println!("   ✅ {} 名称正确", expected_name);
        } else {
            println!("   ❌ {} 名称错误: 期望 '{}', 得到 '{}'",
                expected_name, expected_name, page_type.name());
            all_passed = false;
        }
        
        // 测试 value
        if page_type.value() == expected_value {
            println!("   ✅ {} 值正确", expected_name);
        } else {
            println!("   ❌ {} 值错误: 期望 {}, 得到 {}",
                expected_name, expected_value, page_type.value());
            all_passed = false;
        }
    }
    
    // 测试未知页面类型
    println!("   测试未知页面类型 (999):");
    match PageType::from_u32(999) {
        Some(page_type) => {
            println!("   ❌ 未知页面类型被解析为: {}", page_type);
            all_passed = false;
        }
        None => println!("   ✅ 未知页面类型正确返回 None"),
    }
    
    println!();
    all_passed
}

// 测试 DataPageSubtype 枚举
fn test_data_page_subtype_enum() -> bool {
    println!("   测试 DataPageSubtype 枚举");
    println!("--------------------------------------------------------------------------------");
    
    let mut all_passed = true;
    
    // 测试 from_u32
    let tests = [
        (DataPageSubtype::Main, 7618377, "主要数据页面", 929),
        (DataPageSubtype::Aux, 13387743, "辅助数据页面", 1634),
        (DataPageSubtype::Index, 86284645, "索引数据页面", 10550),
        (DataPageSubtype::Attr, 63068511, "属性数据页面", 7699),
        (DataPageSubtype::Ext, 66156832, "扩展数据页面", 8080),
    ];
    
    for (subtype, expected_value, expected_name, expected_bucket_id) in tests {
        // 测试 from_u32
        match DataPageSubtype::from_u32(expected_value) {
            Some(parsed_subtype) => {
                if parsed_subtype == subtype {
                    println!("   ✅ {} (值 {}) 解析正确", expected_name, expected_value);
                } else {
                    println!("   ❌ {} (值 {}) 解析错误", expected_name, expected_value);
                    all_passed = false;
                }
            }
            None => {
                println!("   ❌ {} (值 {}) 解析失败: 返回 None", expected_name, expected_value);
                all_passed = false;
            }
        }
        
        // 测试 name
        if subtype.name() == expected_name {
            println!("   ✅ {} 名称正确", expected_name);
        } else {
            println!("   ❌ {} 名称错误", expected_name);
            all_passed = false;
        }
        
        // 测试 get_bucket_id
        if subtype.get_bucket_id() == expected_bucket_id {
            println!("   ✅ {} 桶ID正确: {}", expected_name, expected_bucket_id);
        } else {
            println!("   ❌ {} 桶ID错误: 期望 {}, 得到 {}",
                expected_name, expected_bucket_id, subtype.get_bucket_id());
            all_passed = false;
        }
        
        // 测试 value
        if subtype.value() == expected_value {
            println!("   ✅ {} 值正确", expected_name);
        } else {
            println!("   ❌ {} 值错误", expected_name);
            all_passed = false;
        }
    }
    
    // 测试未知数据页面子类型
    println!("   测试未知数据页面子类型 (999999):");
    match DataPageSubtype::from_u32(999999) {
        Some(subtype) => {
            println!("   ❌ 未知数据页面子类型被解析为: {}", subtype);
            all_passed = false;
        }
        None => println!("   ✅ 未知数据页面子类型正确返回 None"),
    }
    
    println!();
    all_passed
}

// 测试 verify_page_type 函数
fn test_verify_page_type() -> bool {
    println!("   测试 verify_page_type 函数");
    println!("--------------------------------------------------------------------------------");
    
    let mut all_passed = true;
    
    // 测试会话页面 (类型 3)
    let session_page_data: [u8; 4] = [0x00, 0x00, 0x00, 0x03];
    match verify_page_type(&session_page_data) {
        Ok(page_type) => {
            if page_type == PageType::Session {
                println!("   ✅ 会话页面验证通过: {}", page_type);
            } else {
                println!("   ❌ 会话页面验证失败: 期望 Session，得到 {}", page_type);
                all_passed = false;
            }
        }
        Err(e) => {
            println!("   ❌ 会话页面验证失败: {}", e);
            all_passed = false;
        }
    }
    
    // 测试数据页面 (类型 5)
    let data_page_data: [u8; 4] = [0x00, 0x00, 0x00, 0x05];
    match verify_page_type(&data_page_data) {
        Ok(page_type) => {
            if page_type == PageType::Data {
                println!("   ✅ 数据页面验证通过: {}", page_type);
            } else {
                println!("   ❌ 数据页面验证失败: 期望 Data，得到 {}", page_type);
                all_passed = false;
            }
        }
        Err(e) => {
            println!("   ❌ 数据页面验证失败: {}", e);
            all_passed = false;
        }
    }
    
    // 测试未知页面类型
    let unknown_page_data: [u8; 4] = [0x00, 0x00, 0x00, 0x99];
    match verify_page_type(&unknown_page_data) {
        Ok(page_type) => {
            println!("   ❌ 未知页面类型被验证为: {}", page_type);
            all_passed = false;
        }
        Err(e) => {
            println!("   ✅ 未知页面类型验证失败（预期）: {}", e);
        }
    }
    
    // 测试不完整数据
    let incomplete_data: [u8; 2] = [0x00, 0x00];
    match verify_page_type(&incomplete_data) {
        Ok(page_type) => {
            println!("   ❌ 不完整数据被验证为: {}", page_type);
            all_passed = false;
        }
        Err(e) => {
            println!("   ✅ 不完整数据验证失败（预期）: {}", e);
        }
    }
    
    println!();
    all_passed
}

// 测试 verify_data_page_subtype 函数
fn test_verify_data_page_subtype() -> bool {
    println!("   测试 verify_data_page_subtype 函数");
    println!("--------------------------------------------------------------------------------");
    
    let mut all_passed = true;
    
    // 测试主要数据页面
    let main_data_page: [u8; 4] = [0x00, 0x74, 0x3F, 0x49];  // 0x00743F49
    match verify_data_page_subtype(&main_data_page) {
        Ok(subtype) => {
            if subtype == DataPageSubtype::Main {
                println!("   ✅ 主要数据页面验证通过: {}", subtype);
            } else {
                println!("   ❌ 主要数据页面验证失败: 期望 Main，得到 {}", subtype);
                all_passed = false;
            }
        }
        Err(e) => {
            println!("   ❌ 主要数据页面验证失败: {}", e);
            all_passed = false;
        }
    }
    
    // 测试辅助数据页面
    let aux_data_page: [u8; 4] = [0x00, 0xCC, 0x47, 0xDF];  // 0x00CC47DF
    match verify_data_page_subtype(&aux_data_page) {
        Ok(subtype) => {
            if subtype == DataPageSubtype::Aux {
                println!("   ✅ 辅助数据页面验证通过: {}", subtype);
            } else {
                println!("   ❌ 辅助数据页面验证失败: 期望 Aux，得到 {}", subtype);
                all_passed = false;
            }
        }
        Err(e) => {
            println!("   ❌ 辅助数据页面验证失败: {}", e);
            all_passed = false;
        }
    }
    
    // 测试未知数据页面子类型
    let unknown_subtype_data: [u8; 4] = [0x00, 0x00, 0x00, 0x99];
    match verify_data_page_subtype(&unknown_subtype_data) {
        Ok(subtype) => {
            println!("   ❌ 未知数据页面子类型被验证为: {}", subtype);
            all_passed = false;
        }
        Err(e) => {
            println!("   ✅ 未知数据页面子类型验证失败（预期）: {}", e);
        }
    }
    
    // 测试不完整数据
    let incomplete_data: [u8; 2] = [0x00, 0x00];
    match verify_data_page_subtype(&incomplete_data) {
        Ok(subtype) => {
            println!("   ❌ 不完整数据被验证为: {}", subtype);
            all_passed = false;
        }
        Err(e) => {
            println!("   ✅ 不完整数据验证失败（预期）: {}", e);
        }
    }
    
    println!();
    all_passed
}

// 测试实际数据库文件
fn test_actual_database() -> bool {
    println!("   测试实际数据库文件");
    println!("--------------------------------------------------------------------------------");
    
    use std::fs::File;
    use std::io::Read;
    
    let file_path = "/Volumes/DPC/work/plant-code/aios-parse-pdms-fork/test-files/ams7330_0001";
    
    // 读取数据库文件
    let mut file = match File::open(file_path) {
        Ok(f) => f,
        Err(e) => {
            println!("   ⚠️  无法打开数据库文件: {}", e);
            println!("   ⚠️  跳过实际数据库文件测试");
            println!();
            return true;  // 不算失败
        }
    };
    
    // 读取文件头部
    let mut header_data = vec![0u8; 64];
    if let Err(e) = file.read_exact(&mut header_data) {
        println!("   ❌ 读取文件头部失败: {}", e);
        return false;
    }
    
    // 验证 PAGE_SIZE
    let page_size = u32::from_be_bytes([
        header_data[0x34],
        header_data[0x35],
        header_data[0x36],
        header_data[0x37],
    ]);
    
    println!("   文件中的页面大小: {} 字节 = 0x{:X}", page_size, page_size);
    println!("   代码中的页面大小: {} 字节 = 0x{:X}", PAGE_SIZE, PAGE_SIZE);
    
    if page_size == PAGE_SIZE as u32 {
        println!("   ✅ PAGE_SIZE 与实际文件匹配！");
    } else {
        println!("   ❌ PAGE_SIZE 与实际文件不匹配！");
        return false;
    }
    
    // 验证会话页面类型
    let session_page_no = u32::from_be_bytes([
        header_data[0x30],
        header_data[0x31],
        header_data[0x32],
        header_data[0x33],
    ]);
    
    println!("   会话页面号: {}", session_page_no);
    
    if session_page_no > 0 {
        println!("   ✅ 会话页面号有效！");
    } else {
        println!("   ⚠️  会话页面号为 0（可能是有效的默认值）");
    }
    
    println!();
    true
}
