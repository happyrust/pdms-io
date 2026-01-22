//! 元素解析器模块
//!
//! 提供 PDMS 元素数据的解析功能，包括：
//! - `header` - 元素头部解析（refno, type, owner）
//! - `children` - 子元素列表解析
//! - `implicit` - 隐含区数据解析

pub mod children;
pub mod header;

// 重新导出常用类型和函数
pub use children::*;
pub use header::*;
