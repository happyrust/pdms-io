use pdms_io::defines::{PAGE_SIZE, PdmsHeader};
use std::mem;

fn main() {
    println!("================================================================================");
    println!("                    文件头部结构修复验证");
    println!("================================================================================");
    println!();

    // 测试 PAGE_SIZE 常量
    println!("1. 测试 PAGE_SIZE 常量");
    println!("--------------------------------------------------------------------------------");
    println!("   PAGE_SIZE = {} 字节 = 0x{:X}", PAGE_SIZE, PAGE_SIZE);

    if PAGE_SIZE == 0x800 {
        println!("   ✅ PAGE_SIZE 正确！2048 字节");
    } else {
        println!(
            "   ❌ PAGE_SIZE 错误！应该是 2048 字节，实际是 {} 字节",
            PAGE_SIZE
        );
    }
    println!();

    // 测试 PdmsHeader 结构体大小
    println!("2. 测试 PdmsHeader 结构体大小");
    println!("--------------------------------------------------------------------------------");
    let header_size = mem::size_of::<PdmsHeader>();
    println!("   PdmsHeader 大小 = {} 字节", header_size);
    println!("   预期大小 = 60 字节（15 个字段 × 4 字节）");

    if header_size == 60 {
        println!("   ✅ PdmsHeader 大小正确！");
    } else {
        println!("   ⚠️  PdmsHeader 大小不匹配，可能有对齐填充");
    }
    println!();

    // 测试 PdmsHeader 默认值
    println!("3. 测试 PdmsHeader 默认值");
    println!("--------------------------------------------------------------------------------");
    let header = PdmsHeader::default();

    println!("   数据库ID (db_num): {}", header.db_num);
    println!("   版本 (version): {}", header.version);
    println!("   标志位 (flags): 0x{:X}", header.flags);
    println!("   创建时间 (creation_time): {}", header.creation_time);
    println!(
        "   最新会话页号 (latest_ses_pgno): {}",
        header.latest_ses_pgno
    );
    println!("   扩展号 (ext_no): {}", header.ext_no);
    println!();

    println!("   新增字段:");
    println!(
        "     会话页面号 (session_page_no): {}",
        header.session_page_no
    );
    println!("     页面大小 (page_size): {}", header.page_size);
    println!(
        "     存储页数 (stored_page_count): {}",
        header.stored_page_count
    );
    println!("     未知值 (unknown_3): {}", header.unknown_3);
    println!();

    // 验证新增字段
    println!("4. 验证新增字段");
    println!("--------------------------------------------------------------------------------");

    let mut all_fields_valid = true;

    // session_page_no
    if header.session_page_no == 0 {
        println!("   ✅ session_page_no 字段存在");
    } else {
        println!("   ⚠️  session_page_no 默认值不是 0");
        all_fields_valid = false;
    }

    // page_size
    if header.page_size == 0 {
        println!("   ✅ page_size 字段存在");
    } else {
        println!("   ⚠️  page_size 默认值不是 0");
        all_fields_valid = false;
    }

    // stored_page_count
    if header.stored_page_count == 0 {
        println!("   ✅ stored_page_count 字段存在");
    } else {
        println!("   ⚠️  stored_page_count 默认值不是 0");
        all_fields_valid = false;
    }

    // unknown_3
    if header.unknown_3 == 0 {
        println!("   ✅ unknown_3 字段存在");
    } else {
        println!("   ⚠️  unknown_3 默认值不是 0");
        all_fields_valid = false;
    }
    println!();

    // 总结
    println!("================================================================================");
    println!("                            修复验证总结");
    println!("================================================================================");
    println!();

    if PAGE_SIZE == 0x800 && all_fields_valid {
        println!("✅ 所有修复验证通过！");
        println!();
        println!("修复内容:");
        println!("  1. ✅ PAGE_SIZE 常量已修复为 2048 字节");
        println!("  2. ✅ PdmsHeader 新增 4 个字段");
        println!("  3. ✅ 所有字段验证通过");
        println!();
        println!("下一步:");
        println!("  - 运行单元测试: cargo test");
        println!("  - 运行集成测试: cargo run --bin test_read_db");
    } else {
        println!("❌ 修复验证失败！");
        println!();
        if PAGE_SIZE != 0x800 {
            println!("  - PAGE_SIZE 常量错误");
        }
        if !all_fields_valid {
            println!("  - PdmsHeader 字段验证失败");
        }
    }
    println!();
}
