# PDMS 元素搜索功能

基于 [Meilisearch](https://docs.rs/meilisearch-sdk/latest/meilisearch_sdk/) 的 PDMS 元素检索功能，支持按名称、类型进行模糊搜索。

## 功能特性

- 🔍 **模糊搜索**: 支持容错的全文搜索
- 🏷️ **按类型搜索**: 精确匹配元素类型
- 📝 **按名称搜索**: 搜索元素名称
- 🔧 **高级搜索**: 支持多条件过滤和排序
- ⚡ **高性能**: 毫秒级搜索响应
- 📊 **统计信息**: 提供搜索性能统计

## 安装和设置

### 1. 安装 Meilisearch

#### Windows
```bash
# 下载最新版本
curl -L https://install.meilisearch.com | sh

# 或者从 GitHub 下载
# https://github.com/meilisearch/meilisearch/releases
```

#### Linux/macOS
```bash
# 使用安装脚本
curl -L https://install.meilisearch.com | sh

# 或使用包管理器
brew install meilisearch  # macOS
```

### 2. 启动 Meilisearch 服务器

```bash
# 启动服务器（默认端口 7700）
./meilisearch

# 或指定配置
./meilisearch --http-addr 127.0.0.1:7700
```

### 3. 添加依赖

在 `Cargo.toml` 中已包含：
```toml
meilisearch-sdk = "0.28.0"
```

## 使用方法

### 基本用法

```rust
use pdms_io::search::{ElementSearchClient, MeilisearchConfig};

// 1. 配置 Meilisearch
let config = MeilisearchConfig {
    url: "http://localhost:7700".to_string(),
    api_key: None, // 生产环境建议设置 API 密钥
    index_name: "pdms_elements".to_string(),
};

// 2. 创建搜索客户端
let search_client = ElementSearchClient::new(config)?;

// 3. 初始化索引
search_client.initialize_index().await?;

// 4. 索引元素数据
search_client.index_elements(&elements).await?;
```

### 搜索功能

#### 1. 按名称搜索
```rust
// 搜索名称包含 "PIPE" 的元素
let results = search_client.search_by_name("PIPE", 10).await?;
for result in results {
    println!("{} ({})", result.name, result.element_type);
}
```

#### 2. 按类型搜索
```rust
// 搜索所有 PIPE 类型的元素
let results = search_client.search_by_type("PIPE", 10).await?;
```

#### 3. 模糊搜索
```rust
// 模糊搜索，同时搜索名称和类型
let results = search_client.fuzzy_search("ELBOW", None, 10).await?;

// 在特定类型中搜索
let results = search_client.fuzzy_search("90", Some("ELBOW"), 10).await?;
```

#### 4. 高级搜索
```rust
use std::collections::HashMap;

let mut filters = HashMap::new();
filters.insert("element_type".to_string(), "PIPE".to_string());
filters.insert("operation_type".to_string(), "新增".to_string());

let results = search_client.advanced_search(
    "query",           // 搜索查询
    &filters,          // 过滤条件
    Some("sesno:desc"), // 排序
    20                 // 限制结果数量
).await?;
```

#### 5. 带统计信息的搜索
```rust
let result = search_client.search_with_stats("PIPE", 10).await?;
println!("查询: {}", result.query);
println!("处理时间: {} ms", result.processing_time_ms);
println!("总命中数: {:?}", result.estimated_total_hits);
```

## 测试案例

### 运行完整测试
```bash
# 运行完整的搜索功能测试
cargo run --bin test_meilisearch [数据库路径] [Meilisearch URL]

# 示例
cargo run --bin test_meilisearch "D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams8000_0001" "http://localhost:7700"
```

### 运行集成示例
```bash
# 运行简化的集成示例
cargo run --bin test_search_integration [数据库路径]

# 示例
cargo run --bin test_search_integration "D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams8000_0001"
```

## 数据结构

### ElementDocument
搜索索引中的元素文档结构：

```rust
pub struct ElementDocument {
    pub id: String,              // 文档 ID（使用 refno）
    pub refno: String,           // 参考号
    pub name: String,            // 元素名称
    pub element_type: String,    // 元素类型
    pub sesno: u32,             // 会话号
    pub operation_type: String,  // 操作类型（新增、修改、删除）
    pub attributes_text: String, // 属性文本（用于全文搜索）
    pub attributes: HashMap<String, String>, // 属性映射
    pub children: Vec<String>,   // 子元素引用号列表
    pub timestamp: String,       // 时间戳
}
```

### 搜索配置
```rust
pub struct MeilisearchConfig {
    pub url: String,         // Meilisearch 服务器 URL
    pub api_key: Option<String>, // API 密钥（可选）
    pub index_name: String,  // 索引名称
}
```

## 性能优化

### 索引设置
- **可搜索属性**: `name`, `element_type`, `attributes_text`, `refno`
- **可过滤属性**: `element_type`, `operation_type`, `sesno`, `refno`
- **可排序属性**: `sesno`, `timestamp`, `name`

### 搜索优化
- 使用适当的结果限制（`limit`）
- 利用过滤条件减少搜索范围
- 使用排序优化结果展示

## 错误处理

```rust
match search_client.search_by_name("PIPE", 10).await {
    Ok(results) => {
        println!("找到 {} 个结果", results.len());
        for result in results {
            println!("{}", result.name);
        }
    }
    Err(e) => {
        eprintln!("搜索失败: {}", e);
        // 检查 Meilisearch 服务器是否运行
        // 检查网络连接
        // 检查索引是否存在
    }
}
```

## 常见问题

### Q: Meilisearch 连接失败
**A**: 确保 Meilisearch 服务器正在运行：
```bash
./meilisearch
```

### Q: 搜索结果为空
**A**: 检查以下几点：
1. 数据是否已正确索引
2. 搜索查询是否正确
3. 过滤条件是否过于严格

### Q: 索引速度慢
**A**: 优化建议：
1. 批量索引而不是单个索引
2. 减少不必要的属性文本
3. 使用适当的批次大小

### Q: 搜索不准确
**A**: 调整搜索策略：
1. 使用模糊搜索而不是精确匹配
2. 调整搜索属性权重
3. 使用同义词功能

## 扩展功能

### 自定义过滤器
```rust
// 按会话号范围搜索
let mut filters = HashMap::new();
filters.insert("sesno".to_string(), "1..10".to_string());

// 按时间范围搜索
filters.insert("timestamp".to_string(), "2024-01-01..2024-12-31".to_string());
```

### 搜索建议
```rust
// 实现搜索建议功能
impl ElementSearchClient {
    pub async fn get_suggestions(&self, query: &str) -> Result<Vec<String>> {
        // 基于现有数据提供搜索建议
        // 可以基于元素类型、名称等提供自动完成
    }
}
```

## 生产环境部署

### 安全配置
```bash
# 设置 API 密钥
export MEILI_MASTER_KEY="your-secret-key"
./meilisearch
```

### 性能配置
```bash
# 设置内存限制
export MEILI_MAX_INDEXING_MEMORY="2GB"

# 设置数据目录
export MEILI_DB_PATH="./meili_data"
```

### 监控
- 使用 Meilisearch 的内置监控端点
- 监控索引大小和搜索性能
- 设置日志记录

## 参考资料

- [Meilisearch 官方文档](https://docs.meilisearch.com/)
- [Meilisearch Rust SDK](https://docs.rs/meilisearch-sdk/latest/meilisearch_sdk/)
- [搜索最佳实践](https://docs.meilisearch.com/learn/getting_started/search_preview.html) 