//! 属性解析器模块
//!
//! 提供 PDMS 属性的解析功能，包括：
//! - `expression` - 表达式属性解析（轴向、函数、常量等）
//! - `explicit` - 显式属性解析
//! - `axis` - 轴向数据解析
//! - `opcode` - 操作码定义与分发

pub mod axis;
pub mod expression;
pub mod expression_payload;
pub mod explicit;
pub mod implicit;
pub mod opcode;

// 重新导出常用类型和函数
pub use axis::*;
pub use expression::*;
pub use expression_payload::*;
pub use explicit::*;
pub use implicit::*;
pub use opcode::*;
