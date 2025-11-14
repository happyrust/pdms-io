# Sesno 时间戳查询 API

本文档介绍了如何通过指定的 sesno（会话号）获取对应的保存时间戳的新功能。

## 新增方法

在 `PdmsIO` 结构体中新增了两个方法：

### 1. `get_sesno_datetime(sesno: u32) -> anyhow::Result<DateTime<Utc>>`

获取指定会话号的保存时间，返回 `DateTime<Utc>` 类型。

**参数：**
- `sesno: u32` - 要查询的会话号

**返回值：**
- `anyhow::Result<DateTime<Utc>>` - 成功返回指定会话的保存时间，失败返回错误

**错误情况：**
- 当找不到指定会话号对应的页面时返回错误
- 读取会话页数据失败时返回错误

### 2. `get_sesno_timestamp(sesno: u32) -> anyhow::Result<i64>`

获取指定会话号的保存时间戳，返回 Unix 时间戳（秒）。

**参数：**
- `sesno: u32` - 要查询的会话号

**返回值：**
- `anyhow::Result<i64>` - 成功返回指定会话的Unix时间戳(秒)，失败返回错误

**错误情况：**
- 当找不到指定会话号对应的页面时返回错误
- 读取会话页数据失败时返回错误

## 使用示例

### 基本使用

```rust
use pdms_io::io::PdmsIO;

fn main() -> anyhow::Result<()> {
    // 初始化 PDMS IO
    let mut io = PdmsIO::new("ams", "path/to/database", true);
    io.open()?;
    io.init_ses_range_map()?;
    
    let sesno = 1112; // 要查询的会话号
    
    // 方法1：获取 DateTime<Utc>
    let datetime = io.get_sesno_datetime(sesno)?;
    println!("会话 {} 的保存时间: {}", sesno, datetime);
    println!("RFC3339 格式: {}", datetime.to_rfc3339());
    
    // 方法2：获取 Unix 时间戳
    let timestamp = io.get_sesno_timestamp(sesno)?;
    println!("会话 {} 的时间戳: {}", sesno, timestamp);
    
    // 验证两种方法的一致性
    assert_eq!(datetime.timestamp(), timestamp);
    
    Ok(())
}
```

### 批量查询多个会话的时间

```rust
use pdms_io::io::PdmsIO;

fn query_multiple_sessions(io: &mut PdmsIO, sesnos: &[u32]) -> anyhow::Result<()> {
    for &sesno in sesnos {
        match io.get_sesno_datetime(sesno) {
            Ok(datetime) => {
                println!("会话 {}: {}", sesno, datetime);
            }
            Err(e) => {
                println!("会话 {} 查询失败: {}", sesno, e);
            }
        }
    }
    Ok(())
}
```

### 时间范围查询

```rust
use pdms_io::io::PdmsIO;
use chrono::{DateTime, Utc};

fn find_sessions_in_time_range(
    io: &mut PdmsIO, 
    start_time: DateTime<Utc>, 
    end_time: DateTime<Utc>
) -> anyhow::Result<Vec<u32>> {
    let mut sessions_in_range = Vec::new();
    
    // 遍历所有会话
    for (&sesno, _) in &io.ses_range_map {
        if let Ok(session_time) = io.get_sesno_datetime(sesno as u32) {
            if session_time >= start_time && session_time <= end_time {
                sessions_in_range.push(sesno as u32);
            }
        }
    }
    
    // 按时间排序
    sessions_in_range.sort_by_key(|&sesno| {
        io.get_sesno_timestamp(sesno).unwrap_or(0)
    });
    
    Ok(sessions_in_range)
}
```

## 测试程序

项目中包含了一个测试程序 `test_sesno_timestamp`，可以用来验证新功能：

```bash
# 编译测试程序
cargo build --bin test_sesno_timestamp

# 运行测试程序
cargo run --bin test_sesno_timestamp -- "数据库路径" [会话号]

# 示例
cargo run --bin test_sesno_timestamp -- "D:/AVEVA/Projects/E3D2.1/AvevaMarineSample/ams000/ams1112_0001" 1112
```

## 实现原理

新功能基于现有的会话数据查询机制：

1. **会话数据获取**：使用 `get_ses_data(sesno)` 方法获取指定会话号对应的 `SessionPageData`
2. **时间提取**：调用 `SessionPageData` 的 `get_utc_dt()` 方法从时间字段中提取时间信息
3. **格式转换**：对于时间戳方法，将 `DateTime<Utc>` 转换为 Unix 时间戳

### 时间字段存储

在 `SessionPageData` 中，时间信息存储在以下字段：
- `year: u32` - 年份
- `month: u32` - 月份  
- `hours: u32` - 小时数（包含天数信息）
- `seconds: u32` - 秒数（包含分钟信息）

### 时间计算逻辑

```rust
pub fn get_dt(&self) -> DateTime<Utc> {
    let year = self.year;
    let month = self.month;
    let days = self.hours / 24;
    let hours = self.hours % 24;
    let minutes = self.seconds / 60;
    let seconds = self.seconds % 60;
    
    Local.with_ymd_and_hms(
        year as i32,
        month as u32,
        days,
        hours as u32,
        minutes,
        seconds,
    )
    .unwrap()
    .into()
}
```

## 性能考虑

- 新方法复用了现有的会话数据缓存机制（`ses_data_map`）
- 首次访问会话数据时会从文件读取并缓存
- 后续访问同一会话的数据将直接从缓存返回
- 时间计算是轻量级操作，性能开销很小

## 错误处理

两个新方法都会返回 `anyhow::Result`，主要的错误情况包括：

1. **会话不存在**：指定的 sesno 在数据库中不存在
2. **文件读取错误**：无法读取会话页数据
3. **数据解析错误**：会话页数据格式异常

建议在使用时进行适当的错误处理：

```rust
match io.get_sesno_datetime(sesno) {
    Ok(datetime) => {
        // 处理成功情况
        println!("时间: {}", datetime);
    }
    Err(e) => {
        // 处理错误情况
        eprintln!("查询失败: {}", e);
    }
}
```
