# 最新数据收集和保存功能

本文档介绍了 PDMS IO 库中新增的最新数据收集和保存功能，主要包括 `collect_latest_eles` 和 `collect_and_save_latest_data` 两个核心方法。

## 功能概述

### 1. `collect_latest_eles` 方法

这个方法用于从 PDMS 数据库中收集最新的元素数据。它采用从最新会话向前遍历的策略，只保留新增的元素，跳过已删除和修改的元素。

**特点：**
- 从最新会话开始向前遍历
- 只收集新增的元素（`EleOperationDetail::Add`）
- 自动跳过已删除的元素
- 避免重复处理相同的元素
- 支持限制处理的会话数量

**方法签名：**
```rust
pub fn collect_latest_eles(
    &mut self,
    max_sessions: Option<u32>,
) -> anyhow::Result<HashMap<RefU64, EleOperationData>>
```

**参数：**
- `max_sessions`: 可选的最大会话数量限制，如果为 `None` 则处理所有会话

**返回值：**
- `HashMap<RefU64, EleOperationData>`: 参考号到元素操作数据的映射

### 2. `collect_and_save_latest_data` 方法

这个方法结合了数据收集和数据库保存功能，是一个完整的数据处理流程。

**特点：**
- 自动收集最新元素数据
- 按会话组织数据
- 保存会话信息到 SurrealDB
- 保存元素数据到 SurrealDB
- 更新会话统计信息
- 提供详细的执行进度和性能统计

**方法签名：**
```rust
pub async fn collect_and_save_latest_data(
    &mut self,
    max_sessions: Option<u32>,
) -> anyhow::Result<()>
```

**执行流程：**
1. 收集最新元素数据
2. 按会话组织数据
3. 保存会话信息到数据库
4. 统计并更新会话的增删改数量
5. 保存元素数据到数据库

## 数据库表结构

### sessions 表
存储会话信息：
```sql
{
    id: "dbnum_sesno",
    sesno: 会话号,
    timestamp: 会话时间戳,
    dbnum: 数据库编号,
    add_count: 新增元素数量,
    modify_count: 修改元素数量,
    delete_count: 删除元素数量,
    computer_name: 计算机名称,
    comments: 注释,
    end_pgno: 结束页号,
    index_root_pageno: 索引根页号,
    claim_pageno: 声明页号
}
```

### element_changes 表
存储元素变更记录：
```sql
{
    id: [pe_key, sesno],
    refno: 参考号,
    operation_type: 操作类型,
    entity_type: 实体类型,
    timestamp: 时间戳,
    session_id: 会话ID,
    sesno: 会话号,
    details: 变更详情
}
```

## 使用示例

### 基本使用

```rust
use pdms_io::io::PdmsIO;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化 PDMS IO
    let mut io = PdmsIO::new("ams", "path/to/database", true);
    io.open()?;
    
    // 收集并保存最新数据（处理最近5个会话）
    io.collect_and_save_latest_data(Some(5)).await?;
    
    Ok(())
}
```

### 只收集数据不保存

```rust
// 只收集最新元素数据
let latest_elements = io.collect_latest_eles(Some(10))?;
println!("收集到 {} 个最新元素", latest_elements.len());

// 处理收集到的数据
for (refno, element_data) in latest_elements {
    println!("元素 {}: 类型 {}, 会话 {}", 
             refno, 
             element_data.get_noun_type(), 
             element_data.sesno);
}
```

## 命令行工具

### demo_latest_data_save

专门用于演示最新数据收集和保存功能的命令行工具。

**用法：**
```bash
cargo run --bin demo_latest_data_save [数据库路径] [最大会话数]
```

**参数：**
- `数据库路径`: PDMS 数据库文件路径（可选）
- `最大会话数`: 要处理的最大会话数量（可选，默认为 5）

**示例：**
```bash
# 使用默认参数
cargo run --bin demo_latest_data_save

# 指定数据库路径和会话数
cargo run --bin demo_latest_data_save "D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams8000_0001" 10
```

### test_meilisearch

集成了数据收集、保存和搜索索引功能的测试工具。

**用法：**
```bash
cargo run --bin test_meilisearch [数据库路径] [Meilisearch URL]
```

## 性能优化

### 批量处理
- 会话记录：每批 50 条
- 元素记录：每批 50 条
- SurrealQL 语句：每批 50 条

### 错误处理
- 使用 `INSERT IGNORE` 避免重复插入错误
- 单独处理每个批次，避免单个错误影响整个流程
- 详细的错误日志记录

### 内存优化
- 流式处理，避免一次性加载所有数据
- 及时清理临时数据结构
- 分批处理大量数据

## 注意事项

1. **数据库连接**: 确保 SurrealDB 连接已正确初始化
2. **文件权限**: 确保对 PDMS 数据库文件有读取权限
3. **内存使用**: 处理大量会话时注意内存使用情况
4. **网络连接**: 保存到 SurrealDB 时需要稳定的网络连接
5. **事务处理**: 当前实现不使用事务，建议在生产环境中考虑事务处理

## 错误处理

常见错误及解决方案：

1. **文件不存在**: 检查数据库文件路径是否正确
2. **权限不足**: 确保对数据库文件有读取权限
3. **SurrealDB 连接失败**: 检查数据库连接配置
4. **内存不足**: 减少 `max_sessions` 参数值
5. **网络超时**: 检查网络连接和 SurrealDB 服务状态

## 扩展功能

可以基于这些核心方法扩展的功能：

1. **增量同步**: 定期运行以保持数据同步
2. **数据验证**: 添加数据完整性检查
3. **性能监控**: 添加详细的性能指标收集
4. **并发处理**: 支持多线程并发处理
5. **数据压缩**: 对大量数据进行压缩存储 