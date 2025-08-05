//! Raphtory 时间图数据库集成模块
//!
//! 提供将 PDMS 数据存储到 Raphtory 时间图数据库的功能，支持：
//! - 历史数据追踪
//! - 时间序列分析
//! - 元素关系图构建

pub mod graph_builder;
pub mod integration;

pub use graph_builder::GraphBuilder;
pub use integration::RaphtoryIntegration;

use anyhow::Result;
use aios_core::pdms_types::RefU64;
use crate::io::EleOperationData;
use std::collections::HashMap;

/// Raphtory 集成配置
#[derive(Clone, Debug)]
pub struct RaphtoryConfig {
    /// 图数据库名称
    pub graph_name: String,
    /// 数据库编号
    pub db_num: i32,
    /// 是否启用详细日志
    pub verbose_logging: bool,
    /// 批量处理大小
    pub batch_size: usize,
}

impl Default for RaphtoryConfig {
    fn default() -> Self {
        Self {
            graph_name: "pdms_temporal_graph".to_string(),
            db_num: 0,
            verbose_logging: false,
            batch_size: 1000,
        }
    }
}

/// 时间戳转换工具
pub struct TimeUtils;

impl TimeUtils {
    /// 将会话号转换为时间戳
    pub fn session_to_timestamp(session_no: i32) -> i64 {
        // 使用会话号作为时间戳，这样保证了时间顺序
        session_no as i64
    }
    
    /// 将时间戳转换为会话号
    pub fn timestamp_to_session(timestamp: i64) -> i32 {
        timestamp as i32
    }
}

/// 历史查询结果
#[derive(Debug, Clone)]
pub struct HistoricalElement {
    /// 参考号
    pub refno: RefU64,
    /// 时间戳
    pub timestamp: i64,
    /// 会话号
    pub session_no: i32,
    /// 元素数据
    pub data: EleOperationData,
}

/// 时间线查询结果
pub type ElementTimeline = Vec<HistoricalElement>;