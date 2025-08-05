//! Raphtory 图构建器
//!
//! 负责将 PDMS 元素数据转换为 Raphtory 图结构

use anyhow::{anyhow, Result};
use aios_core::pdms_types::RefU64;
use aios_core::{NamedAttrValue, NamedAttrMap};
use crate::io::{EleOperationData, EleOperationDetail};
use raphtory::prelude::*;
use std::collections::HashMap;
use super::{RaphtoryConfig, TimeUtils};

/// Raphtory 图构建器
pub struct GraphBuilder {
    /// Raphtory 图实例
    graph: Graph,
    /// 配置信息
    config: RaphtoryConfig,
    /// 会话号到时间戳的映射
    session_timestamp_map: HashMap<i32, i64>,
    /// 已处理的节点集合
    processed_nodes: HashMap<RefU64, i64>, // refno -> last_timestamp
}

impl GraphBuilder {
    /// 创建新的图构建器
    pub fn new(config: RaphtoryConfig) -> Self {
        let graph = Graph::new();
        Self {
            graph,
            config,
            session_timestamp_map: HashMap::new(),
            processed_nodes: HashMap::new(),
        }
    }

    /// 获取图实例的引用
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// 批量添加元素到图中
    pub fn add_elements_batch(
        &mut self,
        elements: &HashMap<RefU64, EleOperationData>,
    ) -> Result<()> {
        if self.config.verbose_logging {
            println!("开始批量添加 {} 个元素到 Raphtory 图", elements.len());
        }

        let mut added_count = 0;
        let mut updated_count = 0;

        for (refno, operation_data) in elements {
            match self.add_element(*refno, operation_data) {
                Ok(is_new) => {
                    if is_new {
                        added_count += 1;
                    } else {
                        updated_count += 1;
                    }
                }
                Err(e) => {
                    eprintln!("添加元素 {} 失败: {}", refno, e);
                    return Err(e);
                }
            }
        }

        if self.config.verbose_logging {
            println!("批量添加完成: 新增 {} 个节点, 更新 {} 个节点", added_count, updated_count);
        }

        Ok(())
    }

    /// 添加单个元素到图中
    pub fn add_element(&mut self, refno: RefU64, operation_data: &EleOperationData) -> Result<bool> {
        let timestamp = TimeUtils::session_to_timestamp(operation_data.sesno as i32);
        
        // 更新会话时间戳映射
        self.session_timestamp_map.insert(operation_data.sesno as i32, timestamp);

        // 检查是否为新节点
        let is_new_node = !self.processed_nodes.contains_key(&refno);

        // 添加或更新节点
        self.add_or_update_node(refno, operation_data, timestamp)?;

        // 添加关系边
        self.add_relationships(refno, operation_data, timestamp)?;

        // 更新处理记录
        self.processed_nodes.insert(refno, timestamp);

        Ok(is_new_node)
    }

    /// 添加或更新节点
    fn add_or_update_node(
        &mut self,
        refno: RefU64,
        operation_data: &EleOperationData,
        timestamp: i64,
    ) -> Result<()> {
        let refno_str = refno.to_string();
        
        // 根据操作类型设置节点属性
        let mut properties = Vec::new();
        
        match &operation_data.detail {
            EleOperationDetail::Add(element) => {
                properties.push(("operation_type", Prop::str("add")));
                properties.push(("element_type", Prop::str(element.att_map().get_type())));
                properties.push(("owner", Prop::str(element.owner.to_string())));
                properties.push(("session_no", Prop::I32(operation_data.sesno as i32)));
                properties.push(("db_num", Prop::I32(self.config.db_num)));
                
                // 添加元素属性
                self.add_element_attributes(&mut properties, element.att_map())?;
            }
            EleOperationDetail::Modified(modified_element) => {
                properties.push(("operation_type", Prop::str("modify")));
                properties.push(("element_type", Prop::str(modified_element.noun.clone())));
                properties.push(("session_no", Prop::I32(operation_data.sesno as i32)));
                properties.push(("db_num", Prop::I32(self.config.db_num)));
                
                // 添加修改统计信息
                properties.push(("added_attrs_count", Prop::I32(modified_element.added_attrs.len() as i32)));
                properties.push(("deleted_attrs_count", Prop::I32(modified_element.deleted_attrs.len() as i32)));
                properties.push(("modified_attrs_count", Prop::I32(modified_element.modified_attrs.len() as i32)));
            }
            EleOperationDetail::Deleted => {
                properties.push(("operation_type", Prop::str("delete")));
                properties.push(("session_no", Prop::I32(operation_data.sesno as i32)));
                properties.push(("db_num", Prop::I32(self.config.db_num)));
            }
            EleOperationDetail::None => {
                properties.push(("operation_type", Prop::str("none")));
                properties.push(("session_no", Prop::I32(operation_data.sesno as i32)));
                properties.push(("db_num", Prop::I32(self.config.db_num)));
            }
        }

        // 添加节点到图中
        self.graph.add_node(
            timestamp,
            refno_str,
            properties,
            None,
        ).map_err(|e| anyhow!("添加节点失败: {}", e))?;

        Ok(())
    }

    /// 添加元素属性到属性列表
    fn add_element_attributes(
        &self,
        properties: &mut Vec<(&str, Prop)>,
        attr_map: &NamedAttrMap,
    ) -> Result<()> {
        // 添加基本属性
        if let Some(name) = attr_map.get_name() {
            if !name.is_empty() {
                properties.push(("element_name", Prop::str(name)));
            }
        }

        // 添加元素类型信息
        properties.push(("element_type_from_map", Prop::str(attr_map.get_type())));

        // 提取并添加PE数据
        self.add_pe_data_attributes(properties, attr_map)?;

        Ok(())
    }

    /// 提取PE数据并添加到属性列表
    fn add_pe_data_attributes(
        &self,
        properties: &mut Vec<(&str, Prop)>,
        attr_map: &NamedAttrMap,
    ) -> Result<()> {
        // 生成PE数据
        let pe_data = attr_map.pe(self.config.db_num);

        // 添加PE数据的关键字段
        properties.push(("pe_refno", Prop::str(pe_data.refno.to_string())));
        properties.push(("pe_sesno", Prop::I32(pe_data.sesno)));
        properties.push(("pe_dbnum", Prop::I32(pe_data.dbnum)));

        // 添加PE数据的JSON表示（用于完整数据存储）
        let pe_json = pe_data.gen_sur_json(None);
        properties.push(("pe_data_json", Prop::str(pe_json)));

        // TODO: 添加状态信息（如果NamedAttrMap有get_status方法）
        // if let Some(status) = attr_map.get_status() {
        //     properties.push(("element_status", Prop::str(status)));
        // }

        // 添加位置信息（如果存在）
        if let Some(position) = attr_map.get_position() {
            properties.push(("position_x", Prop::F64(position.x as f64)));
            properties.push(("position_y", Prop::F64(position.y as f64)));
            properties.push(("position_z", Prop::F64(position.z as f64)));
        }

        // 添加方向信息（如果存在）
        if let Some(orientation) = attr_map.get_rotation() {
            properties.push(("orientation_x", Prop::F64(orientation.x as f64)));
            properties.push(("orientation_y", Prop::F64(orientation.y as f64)));
            properties.push(("orientation_z", Prop::F64(orientation.z as f64)));
        }

        if self.config.verbose_logging {
            println!("已添加PE数据属性: refno={}, 类型={}", pe_data.refno, attr_map.get_type());
        }

        Ok(())
    }

    /// 添加关系边
    fn add_relationships(
        &mut self,
        refno: RefU64,
        operation_data: &EleOperationData,
        timestamp: i64,
    ) -> Result<()> {
        match &operation_data.detail {
            EleOperationDetail::Add(element) => {
                // 添加父子关系边
                let owner_str = element.owner.to_string();
                let refno_str = refno.to_string();
                
                // 父 -> 子 边
                if element.owner != RefU64::default() {
                    self.graph.add_edge(
                        timestamp,
                        owner_str,
                        refno_str,
                        [
                            ("relationship_type", Prop::str("parent_child")),
                            ("direction", Prop::str("parent_to_child")),
                            ("session_no", Prop::I32(operation_data.sesno as i32)),
                        ],
                        None,
                    ).map_err(|e| anyhow!("添加父子边失败: {}", e))?;
                }

                // 为子元素添加边（简化处理，跳过子元素迭代）
                // 注意：子元素的处理需要根据实际的 RefU64Vec 结构调整
                // 这里暂时跳过以避免编译错误
            }
            _ => {
                // 对于其他操作类型，暂时不添加关系边
            }
        }

        Ok(())
    }

    /// 获取统计信息
    pub fn get_statistics(&self) -> HashMap<String, i64> {
        let mut stats = HashMap::new();
        
        stats.insert("total_nodes".to_string(), self.graph.count_nodes() as i64);
        stats.insert("total_edges".to_string(), self.graph.count_edges() as i64);
        stats.insert("processed_sessions".to_string(), self.session_timestamp_map.len() as i64);
        stats.insert("processed_elements".to_string(), self.processed_nodes.len() as i64);
        
        stats
    }

    /// 获取图的时间范围
    pub fn get_time_range(&self) -> Option<(i64, i64)> {
        if self.session_timestamp_map.is_empty() {
            return None;
        }

        let timestamps: Vec<i64> = self.session_timestamp_map.values().cloned().collect();
        let min_time = *timestamps.iter().min()?;
        let max_time = *timestamps.iter().max()?;
        
        Some((min_time, max_time))
    }
}