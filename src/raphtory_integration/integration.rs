//! Raphtory 集成主要接口
//!
//! 提供与 PDMS IO 系统的集成功能

use anyhow::{anyhow, Result};
use aios_core::pdms_types::RefU64;
use crate::io::EleOperationData;
use raphtory::prelude::*;
use std::collections::HashMap;
use super::{RaphtoryConfig, GraphBuilder, TimeUtils, HistoricalElement, ElementTimeline};

/// Raphtory 集成主接口
pub struct RaphtoryIntegration {
    /// 图构建器
    graph_builder: GraphBuilder,
    /// 配置信息
    config: RaphtoryConfig,
    /// 是否已完成初始化
    initialized: bool,
}

impl RaphtoryIntegration {
    /// 创建新的 Raphtory 集成实例
    pub fn new(config: RaphtoryConfig) -> Self {
        let graph_builder = GraphBuilder::new(config.clone());
        
        Self {
            graph_builder,
            config,
            initialized: false,
        }
    }

    /// 使用默认配置创建实例
    pub fn with_default_config(db_num: i32) -> Self {
        let mut config = RaphtoryConfig::default();
        config.db_num = db_num;
        config.graph_name = format!("pdms_db_{}_graph", db_num);
        
        Self::new(config)
    }

    /// 获取图名称
    pub fn get_graph_name(&self) -> &str {
        &self.config.graph_name
    }

    /// 初始化集成
    pub fn initialize(&mut self) -> Result<()> {
        if self.config.verbose_logging {
            println!("初始化 Raphtory 集成: {}", self.config.graph_name);
        }
        
        self.initialized = true;
        Ok(())
    }

    /// 批量存储元素数据
    pub fn store_elements(&mut self, elements: &HashMap<RefU64, EleOperationData>) -> Result<()> {
        if !self.initialized {
            return Err(anyhow!("集成未初始化，请先调用 initialize()"));
        }

        if elements.is_empty() {
            if self.config.verbose_logging {
                println!("没有元素需要存储到 Raphtory");
            }
            return Ok(());
        }

        if self.config.verbose_logging {
            println!("开始存储 {} 个元素到 Raphtory 图数据库", elements.len());
        }

        self.graph_builder.add_elements_batch(elements)?;

        if self.config.verbose_logging {
            let stats = self.graph_builder.get_statistics();
            println!("存储完成，当前图统计信息: {:?}", stats);
        }

        Ok(())
    }

    /// 查询指定时间点的元素状态
    pub fn query_historical_state(
        &self,
        refno: RefU64,
        timestamp: i64,
    ) -> Result<Option<HistoricalElement>> {
        if !self.initialized {
            return Err(anyhow!("集成未初始化"));
        }

        let graph = self.graph_builder.graph();
        let refno_str = refno.to_string();

        // 查询指定时间点的节点
        let windowed_graph = graph.window(i64::MIN, timestamp);
        
        if let Some(node) = windowed_graph.node(&refno_str) {
            // 节点存在，获取其状态
            let session_no = TimeUtils::timestamp_to_session(timestamp);
                
            // 构造历史元素结果
            let historical_element = HistoricalElement {
                refno,
                timestamp,
                session_no,
                data: crate::io::EleOperationData {
                    refno,
                    sesno: session_no as u32,
                    detail: crate::io::EleOperationDetail::None, // 简化处理
                },
            };

            return Ok(Some(historical_element));
        }

        Ok(None)
    }

    /// 获取元素的完整时间线
    pub fn get_element_timeline(&self, refno: RefU64) -> Result<ElementTimeline> {
        if !self.initialized {
            return Err(anyhow!("集成未初始化"));
        }

        let graph = self.graph_builder.graph();
        let refno_str = refno.to_string();

        let mut timeline = Vec::new();

        if let Some(node) = graph.node(&refno_str) {
            // 获取节点的所有历史状态
            for timestamp in node.history() {
                let session_no = TimeUtils::timestamp_to_session(timestamp);

                let historical_element = HistoricalElement {
                    refno,
                    timestamp,
                    session_no,
                    data: crate::io::EleOperationData {
                        refno,
                        sesno: session_no as u32,
                        detail: crate::io::EleOperationDetail::None, // 简化处理
                    },
                };

                timeline.push(historical_element);
            }
        }

        // 按时间戳排序
        timeline.sort_by_key(|item| item.timestamp);

        Ok(timeline)
    }

    /// 查询指定时间范围内的所有变更
    pub fn query_changes_in_range(
        &self,
        start_timestamp: i64,
        end_timestamp: i64,
    ) -> Result<HashMap<RefU64, ElementTimeline>> {
        if !self.initialized {
            return Err(anyhow!("集成未初始化"));
        }

        let graph = self.graph_builder.graph();
        let windowed_graph = graph.window(start_timestamp, end_timestamp);
        
        let mut changes = HashMap::new();

        // 遍历时间窗口内的所有节点
        for node in windowed_graph.nodes() {
            if let Ok(refno) = node.name().parse::<RefU64>() {
                let timeline = self.get_element_timeline(refno)?;
                
                // 过滤时间范围内的变更
                let filtered_timeline: ElementTimeline = timeline
                    .into_iter()
                    .filter(|item| item.timestamp >= start_timestamp && item.timestamp <= end_timestamp)
                    .collect();

                if !filtered_timeline.is_empty() {
                    changes.insert(refno, filtered_timeline);
                }
            }
        }

        Ok(changes)
    }

    /// 获取图的统计信息
    pub fn get_statistics(&self) -> HashMap<String, i64> {
        self.graph_builder.get_statistics()
    }

    /// 获取图的时间范围
    pub fn get_time_range(&self) -> Option<(i64, i64)> {
        self.graph_builder.get_time_range()
    }

    /// 获取配置信息
    pub fn get_config(&self) -> &RaphtoryConfig {
        &self.config
    }

    /// 获取图实例的引用（用于高级查询）
    pub fn graph(&self) -> &Graph {
        self.graph_builder.graph()
    }

    /// 完成并保存图数据
    pub async fn finalize_and_save(&mut self) -> Result<()> {
        if !self.initialized {
            return Err(anyhow!("集成未初始化"));
        }

        if self.config.verbose_logging {
            let stats = self.get_statistics();
            println!("最终图统计信息: {:?}", stats);
            
            if let Some((start, end)) = self.get_time_range() {
                println!("时间范围: {} 到 {}", start, end);
                println!("会话范围: {} 到 {}", 
                    TimeUtils::timestamp_to_session(start),
                    TimeUtils::timestamp_to_session(end)
                );
            }
        }

        // TODO: 保存图数据到文件系统
        // 当前版本先在内存中保存图数据，后续可以添加持久化功能
        if self.config.verbose_logging {
            println!("图数据已在内存中构建完成，可通过 GraphQL 查询访问");
        }
        
        Ok(())
    }

    /// 清理资源
    pub fn cleanup(&mut self) {
        if self.config.verbose_logging {
            println!("清理 Raphtory 集成资源");
        }
        
        self.initialized = false;
    }
}

impl Drop for RaphtoryIntegration {
    fn drop(&mut self) {
        self.cleanup();
    }
}