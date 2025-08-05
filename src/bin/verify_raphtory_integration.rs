//! 验证 Raphtory 集成功能的简化测试程序
//! 
//! 这个程序测试基本的集成功能，而不依赖复杂的 Raphtory 编译

use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    println!("🔧 验证 PDMS-Raphtory 集成基础功能");
    println!("=====================================");

    // 测试 1: 验证配置结构
    println!("\n📋 测试 1: 验证 RaphtoryConfig 结构");
    let config = pdms_io::raphtory_integration::RaphtoryConfig::default();
    println!("✅ 默认配置创建成功:");
    println!("   - 图名称: {}", config.graph_name);
    println!("   - 数据库编号: {}", config.db_num);
    println!("   - 批处理大小: {}", config.batch_size);
    println!("   - 详细日志: {}", config.verbose_logging);

    // 测试 2: 验证 TimeUtils
    println!("\n⏰ 测试 2: 验证 TimeUtils 功能");
    let session_no = 12345;
    let timestamp = pdms_io::raphtory_integration::TimeUtils::session_to_timestamp(session_no);
    let converted_back = pdms_io::raphtory_integration::TimeUtils::timestamp_to_session(timestamp);
    
    println!("✅ 时间转换功能正常:");
    println!("   - 会话号: {} -> 时间戳: {}", session_no, timestamp);
    println!("   - 时间戳: {} -> 会话号: {}", timestamp, converted_back);
    assert_eq!(session_no, converted_back);

    // 测试 3: 验证 RaphtoryIntegration 创建
    println!("\n🔗 测试 3: 验证 RaphtoryIntegration 创建");
    let mut custom_config = pdms_io::raphtory_integration::RaphtoryConfig::default();
    custom_config.db_num = 99999;
    custom_config.verbose_logging = true;
    custom_config.graph_name = "test_verification_graph".to_string();

    let integration = pdms_io::raphtory_integration::RaphtoryIntegration::new(custom_config.clone());
    println!("✅ RaphtoryIntegration 创建成功:");
    println!("   - 配置: {:?}", integration.get_config());

    // 测试 4: 验证 with_default_config
    println!("\n🔧 测试 4: 验证 with_default_config 方法");
    let default_integration = pdms_io::raphtory_integration::RaphtoryIntegration::with_default_config(88888);
    let config = default_integration.get_config();
    println!("✅ with_default_config 创建成功:");
    println!("   - 数据库编号: {}", config.db_num);
    println!("   - 图名称: {}", config.graph_name);
    assert_eq!(config.db_num, 88888);
    assert_eq!(config.graph_name, "pdms_db_88888_graph");

    // 测试 5: 验证基本数据结构
    println!("\n📊 测试 5: 验证数据结构");
    
    // 创建模拟的 EleOperationData
    use aios_core::pdms_types::RefU64;
    let test_refno = RefU64::from_two_nums(17496, 123456);
    let test_data = pdms_io::io::EleOperationData {
        refno: test_refno,
        sesno: 999,
        detail: pdms_io::io::EleOperationDetail::None,
    };

    println!("✅ EleOperationData 创建成功:");
    println!("   - 参考号: {}", test_data.refno);
    println!("   - 会话号: {}", test_data.sesno);
    println!("   - 操作类型: {:?}", test_data.detail);

    // 测试 6: 验证 HistoricalElement
    println!("\n📜 测试 6: 验证 HistoricalElement 结构");
    let historical = pdms_io::raphtory_integration::HistoricalElement {
        refno: test_refno,
        timestamp: 1000,
        session_no: 999,
        data: test_data,
    };

    println!("✅ HistoricalElement 创建成功:");
    println!("   - 参考号: {}", historical.refno);
    println!("   - 时间戳: {}", historical.timestamp);
    println!("   - 会话号: {}", historical.session_no);

    // 测试 7: 验证 ElementTimeline 类型
    println!("\n📅 测试 7: 验证 ElementTimeline 类型");
    let mut timeline: pdms_io::raphtory_integration::ElementTimeline = Vec::new();
    timeline.push(historical);

    println!("✅ ElementTimeline 创建成功:");
    println!("   - 时间线记录数: {}", timeline.len());
    if let Some(first) = timeline.first() {
        println!("   - 第一条记录: 元素 {} 在时间 {}", first.refno, first.timestamp);
    }

    // 总结
    println!("\n🎉 验证完成总结");
    println!("================");
    println!("✅ 所有基础功能验证通过!");
    println!("📦 Raphtory 集成模块结构正确");
    println!("🔧 配置功能正常工作");
    println!("⏰ 时间转换功能正确");
    println!("📊 数据结构定义完整");
    println!("🚀 准备进行完整集成测试");

    println!("\n💡 下一步:");
    println!("   1. 确保 Raphtory 依赖编译正常");
    println!("   2. 测试真实数据的存储功能");
    println!("   3. 验证历史查询功能");
    println!("   4. 性能优化和错误处理");

    Ok(())
}