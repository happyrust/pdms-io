//! Meilisearch 检索功能模块
//!
//! 提供基于 Meilisearch 的元素检索功能，支持按名称和类型进行模糊搜索

use crate::io::{EleOperationData, EleOperationDetail};
use anyhow::Result;
use meilisearch_sdk::{client::Client, search::SearchResults};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Meilisearch 服务器配置
pub struct MeilisearchConfig {
    /// Meilisearch 服务器 URL
    pub url: String,
    /// API 密钥（可选）
    pub api_key: Option<String>,
    /// 索引名称
    pub index_name: String,
}

impl Default for MeilisearchConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:7700".to_string(),
            api_key: None,
            index_name: "pdms_elements".to_string(),
        }
    }
}

/// 用于 Meilisearch 索引的元素文档结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementDocument {
    /// 文档 ID（使用 refno 作为唯一标识）
    pub id: String,
    /// 参考号
    pub refno: String,
    /// 元素名称
    pub name: String,
    /// 元素类型
    pub element_type: String,
    /// 会话号
    pub sesno: u32,
    /// 操作类型（新增、修改、删除）
    pub operation_type: String,
    /// 所有属性的文本表示（用于全文搜索）
    pub attributes_text: String,
    /// 属性映射（JSON 格式）
    pub attributes: HashMap<String, String>,
    /// 子元素引用号列表
    pub children: Vec<String>,
    /// 创建时间戳
    pub timestamp: String,
}

/// Meilisearch 检索客户端
pub struct ElementSearchClient {
    /// Meilisearch 客户端
    client: Client,
    /// 索引名称
    index_name: String,
}

impl ElementSearchClient {
    /// 创建新的检索客户端
    ///
    /// # 参数
    /// * `config` - Meilisearch 配置
    ///
    /// # 返回值
    /// * `Result<Self>` - 成功返回客户端实例，失败返回错误
    pub fn new(config: MeilisearchConfig) -> Result<Self> {
        let client = Client::new(&config.url, config.api_key)?;

        Ok(Self {
            client,
            index_name: config.index_name,
        })
    }

    /// 初始化索引和设置
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    pub async fn initialize_index(&self) -> Result<()> {
        let index = self.client.index(&self.index_name);

        // 设置可搜索属性
        let searchable_attributes = vec!["name", "element_type", "attributes_text", "refno"];
        index
            .set_searchable_attributes(&searchable_attributes)
            .await?;

        // 设置可过滤属性
        let filterable_attributes = vec!["element_type", "operation_type", "sesno", "refno"];
        index
            .set_filterable_attributes(&filterable_attributes)
            .await?;

        // 设置可排序属性
        let sortable_attributes = vec!["sesno", "timestamp", "name"];
        index.set_sortable_attributes(&sortable_attributes).await?;

        println!("Meilisearch 索引 '{}' 初始化完成", self.index_name);
        Ok(())
    }

    /// 将元素操作数据添加到搜索索引
    ///
    /// # 参数
    /// * `elements` - 元素操作数据列表
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    pub async fn index_elements(&self, elements: &[EleOperationData]) -> Result<()> {
        let index = self.client.index(&self.index_name);

        let documents: Vec<ElementDocument> = elements
            .iter()
            .filter_map(|ele| self.convert_to_document(ele))
            .collect();

        if !documents.is_empty() {
            let task = index.add_documents(&documents, Some("id")).await?;
            println!(
                "已添加 {} 个元素到搜索索引，任务 ID: {}",
                documents.len(),
                task.task_uid
            );
        }

        Ok(())
    }

    /// 将元素操作数据转换为搜索文档
    fn convert_to_document(&self, element: &EleOperationData) -> Option<ElementDocument> {
        match &element.detail {
            EleOperationDetail::Add(ele_data) => {
                let att_map = ele_data.att_map();

                // 构建属性文本用于全文搜索
                let mut attributes_text = String::new();
                let mut attributes = HashMap::new();

                for (key, value) in att_map.iter() {
                    let value_str = value.get_val_as_string();
                    attributes_text.push_str(&format!("{} {} ", key, value_str));
                    attributes.insert(key.clone(), value_str);
                }

                // 获取元素名称
                let name = att_map
                    .get("NAME")
                    .map(|v| v.get_val_as_string())
                    .unwrap_or_else(|| element.refno.to_string());

                Some(ElementDocument {
                    id: element.refno.to_string(),
                    refno: element.refno.to_string(),
                    name,
                    element_type: att_map.get_type(),
                    sesno: element.sesno,
                    operation_type: element.get_op_type().to_string(),
                    attributes_text,
                    attributes,
                    children: ele_data.children.iter().map(|r| r.to_string()).collect(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                })
            }
            EleOperationDetail::Modified(_modified) => {
                // 对于修改的元素，我们需要更新现有文档
                // 这里简化处理，只记录修改操作
                // Some(ElementDocument {
                //     id: element.refno.to_string(),
                //     refno: element.refno.to_string(),
                //     name: format!("Modified_{}", element.refno),
                //     element_type: modified.noun.clone(),
                //     sesno: element.sesno,
                //     operation_type: element.get_op_type().to_string(),
                //     attributes_text: format!("修改的元素 {}", modified.noun),
                //     attributes: HashMap::new(),
                //     children: Vec::new(),
                //     timestamp: chrono::Utc::now().to_rfc3339(),
                // })
                None
            }
            EleOperationDetail::Deleted => {
                // 对于删除的元素，标记为已删除
                // Some(ElementDocument {
                //     id: element.refno.to_string(),
                //     refno: element.refno.to_string(),
                //     name: format!("Deleted_{}", element.refno),
                //     element_type: "DELETED".to_string(),
                //     sesno: element.sesno,
                //     operation_type: element.get_op_type().to_string(),
                //     attributes_text: "已删除的元素".to_string(),
                //     attributes: HashMap::new(),
                //     children: Vec::new(),
                //     timestamp: chrono::Utc::now().to_rfc3339(),
                // })
                None
            }
            EleOperationDetail::None => None,
        }
    }

    /// 按名称搜索元素
    ///
    /// # 参数
    /// * `name_query` - 名称查询字符串
    /// * `limit` - 返回结果数量限制
    ///
    /// # 返回值
    /// * `Result<Vec<ElementDocument>>` - 搜索结果
    pub async fn search_by_name(
        &self,
        name_query: &str,
        limit: usize,
    ) -> Result<Vec<ElementDocument>> {
        let index = self.client.index(&self.index_name);

        let search_result: SearchResults<ElementDocument> = index
            .search()
            .with_query(name_query)
            .with_attributes_to_highlight(meilisearch_sdk::search::Selectors::Some(&[
                "name",
                "attributes_text",
            ]))
            .with_limit(limit)
            .execute()
            .await?;

        Ok(search_result
            .hits
            .into_iter()
            .map(|hit| hit.result)
            .collect())
    }

    /// 按类型搜索元素
    ///
    /// # 参数
    /// * `type_query` - 类型查询字符串
    /// * `limit` - 返回结果数量限制
    ///
    /// # 返回值
    /// * `Result<Vec<ElementDocument>>` - 搜索结果
    pub async fn search_by_type(
        &self,
        type_query: &str,
        limit: usize,
    ) -> Result<Vec<ElementDocument>> {
        let index = self.client.index(&self.index_name);
        let filter_string = format!("element_type = {}", type_query);

        let search_result: SearchResults<ElementDocument> = index
            .search()
            .with_query(type_query)
            .with_filter(&filter_string)
            .with_limit(limit)
            .execute()
            .await?;

        Ok(search_result
            .hits
            .into_iter()
            .map(|hit| hit.result)
            .collect())
    }

    /// 模糊搜索元素（同时搜索名称和类型）
    ///
    /// # 参数
    /// * `query` - 搜索查询字符串
    /// * `element_type_filter` - 可选的元素类型过滤器
    /// * `limit` - 返回结果数量限制
    ///
    /// # 返回值
    /// * `Result<Vec<ElementDocument>>` - 搜索结果
    pub async fn fuzzy_search(
        &self,
        query: &str,
        element_type_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ElementDocument>> {
        let index = self.client.index(&self.index_name);

        // 预先构建过滤字符串以延长生命周期
        let filter_string =
            element_type_filter.map(|type_filter| format!("element_type = {}", type_filter));

        // 构建搜索请求
        let mut search_builder = index.search();
        let mut search_request = search_builder
            .with_query(query)
            .with_attributes_to_highlight(meilisearch_sdk::search::Selectors::Some(&[
                "name",
                "element_type",
                "attributes_text",
            ]))
            .with_limit(limit);

        // 如果指定了类型过滤器，添加过滤条件
        if let Some(ref filter_str) = filter_string {
            search_request = search_request.with_filter(filter_str);
        }

        let search_result: SearchResults<ElementDocument> = search_request.execute().await?;

        Ok(search_result
            .hits
            .into_iter()
            .map(|hit| hit.result)
            .collect())
    }

    /// 高级搜索，支持多种过滤条件
    ///
    /// # 参数
    /// * `query` - 搜索查询字符串
    /// * `filters` - 过滤条件映射
    /// * `sort` - 排序字段
    /// * `limit` - 返回结果数量限制
    ///
    /// # 返回值
    /// * `Result<Vec<ElementDocument>>` - 搜索结果
    pub async fn advanced_search(
        &self,
        query: &str,
        filters: &HashMap<String, String>,
        sort: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ElementDocument>> {
        let index = self.client.index(&self.index_name);

        // 预先构建过滤字符串以延长生命周期
        let filter_string = if !filters.is_empty() {
            let filter_conditions: Vec<String> = filters
                .iter()
                .map(|(key, value)| format!("{} = {}", key, value))
                .collect();
            Some(filter_conditions.join(" AND "))
        } else {
            None
        };

        // 预先构建排序数组以延长生命周期
        let sort_array = sort.map(|sort_field| vec![sort_field]);

        // 构建搜索请求
        let mut search_builder = index.search();
        let mut search_request = search_builder
            .with_query(query)
            .with_attributes_to_highlight(meilisearch_sdk::search::Selectors::Some(&[
                "name",
                "element_type",
                "attributes_text",
            ]))
            .with_limit(limit);

        // 添加过滤条件
        if let Some(ref filter_str) = filter_string {
            search_request = search_request.with_filter(filter_str);
        }

        // 添加排序
        if let Some(ref sort_vec) = sort_array {
            search_request = search_request.with_sort(sort_vec);
        }

        let search_result: SearchResults<ElementDocument> = search_request.execute().await?;

        Ok(search_result
            .hits
            .into_iter()
            .map(|hit| hit.result)
            .collect())
    }

    /// 获取索引统计信息
    ///
    /// # 返回值
    /// * `Result<IndexStats>` - 索引统计信息
    pub async fn get_index_stats(&self) -> Result<IndexStats> {
        let index = self.client.index(&self.index_name);
        let stats = index.get_stats().await?;

        Ok(IndexStats {
            number_of_documents: stats.number_of_documents,
            is_indexing: stats.is_indexing,
            field_distribution: stats.field_distribution,
        })
    }

    /// 清空索引
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    pub async fn clear_index(&self) -> Result<()> {
        let index = self.client.index(&self.index_name);
        let task = index.delete_all_documents().await?;
        println!("清空索引任务 ID: {}", task.task_uid);
        Ok(())
    }

    /// 执行搜索并返回包装的结果
    ///
    /// # 参数
    /// * `query` - 搜索查询字符串
    /// * `limit` - 返回结果数量限制
    ///
    /// # 返回值
    /// * `Result<SearchResultWrapper>` - 包装的搜索结果
    pub async fn search_with_stats(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<SearchResultWrapper> {
        let index = self.client.index(&self.index_name);

        let search_result: SearchResults<ElementDocument> = index
            .search()
            .with_query(query)
            .with_attributes_to_highlight(meilisearch_sdk::search::Selectors::Some(&[
                "name",
                "element_type",
                "attributes_text",
            ]))
            .with_limit(limit)
            .execute()
            .await?;

        Ok(SearchResultWrapper {
            results: search_result
                .hits
                .into_iter()
                .map(|hit| hit.result)
                .collect(),
            processing_time_ms: search_result.processing_time_ms,
            query: query.to_string(),
            estimated_total_hits: search_result.estimated_total_hits,
        })
    }

    /// 添加文档到索引
    ///
    /// # 参数
    /// * `documents` - 要添加的文档列表
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回 Ok(())，失败返回错误
    pub async fn add_documents(&self, documents: &[ElementDocument]) -> Result<()> {
        let index = self.client.index(&self.index_name);
        let task = index.add_documents(documents, Some("id")).await?;
        println!(
            "✅ 添加了 {} 个文档，任务 ID: {}",
            documents.len(),
            task.task_uid
        );
        Ok(())
    }
}

/// 索引统计信息
#[derive(Debug)]
pub struct IndexStats {
    /// 文档数量
    pub number_of_documents: usize,
    /// 是否正在索引
    pub is_indexing: bool,
    /// 字段分布
    pub field_distribution: HashMap<String, usize>,
}

/// 搜索结果包装器
#[derive(Debug)]
pub struct SearchResultWrapper {
    /// 搜索结果
    pub results: Vec<ElementDocument>,
    /// 搜索耗时（毫秒）
    pub processing_time_ms: usize,
    /// 查询字符串
    pub query: String,
    /// 总命中数
    pub estimated_total_hits: Option<usize>,
}
