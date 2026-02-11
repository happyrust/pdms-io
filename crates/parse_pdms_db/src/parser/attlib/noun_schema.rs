use aios_core::tool::db_tool::db1_hash;
use serde::{Deserialize, Serialize};
use std::fmt;

use super::{AttrDataType, AttrDefiType, AttributeMeta, AttlibData};

/// NOUN 的完整属性 Schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NounSchema {
    pub noun_name: String,
    pub noun_hash: u32,
    pub attributes: Vec<AttributeMeta>,
}

impl NounSchema {
    /// 从 AttlibData 中查询指定 NOUN 的属性 schema
    pub fn from_attlib(attlib: &AttlibData, noun_name: &str) -> Option<Self> {
        let noun_hash = db1_hash(noun_name) as u32;
        let attrs = attlib.get_noun_attributes(noun_name)?;

        Some(NounSchema {
            noun_name: noun_name.to_uppercase(),
            noun_hash,
            attributes: attrs.into_iter().cloned().collect(),
        })
    }

    /// 按数据类型过滤属性
    pub fn filter_by_type(&self, data_type: AttrDataType) -> Vec<&AttributeMeta> {
        self.attributes
            .iter()
            .filter(|a| a.data_type == data_type)
            .collect()
    }

    /// 按存储方式过滤 (DAB vs Pseudo)
    pub fn filter_by_defi(&self, defi: AttrDefiType) -> Vec<&AttributeMeta> {
        self.attributes
            .iter()
            .filter(|a| a.defi == defi)
            .collect()
    }

    /// 按名称查找属性
    pub fn find_attribute(&self, attr_name: &str) -> Option<&AttributeMeta> {
        let upper = attr_name.to_uppercase();
        self.attributes.iter().find(|a| a.name == upper)
    }

    /// 属性数量
    pub fn attribute_count(&self) -> usize {
        self.attributes.len()
    }

    /// 生成可读的 schema 摘要
    pub fn summary(&self) -> String {
        let mut s = format!(
            "=== NounSchema: {} (hash=0x{:08X}) ===\n",
            self.noun_name, self.noun_hash
        );
        s.push_str(&format!("属性总数: {}\n", self.attributes.len()));

        // 按类型统计
        let mut type_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut defi_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for attr in &self.attributes {
            *type_counts
                .entry(attr.data_type.to_string())
                .or_default() += 1;
            *defi_counts.entry(attr.defi.to_string()).or_default() += 1;
        }

        s.push_str("按数据类型:\n");
        let mut type_items: Vec<_> = type_counts.iter().collect();
        type_items.sort_by(|a, b| b.1.cmp(a.1));
        for (t, c) in type_items {
            s.push_str(&format!("  {:<16} {}\n", t, c));
        }

        s.push_str("按存储方式:\n");
        let mut defi_items: Vec<_> = defi_counts.iter().collect();
        defi_items.sort_by(|a, b| b.1.cmp(a.1));
        for (d, c) in defi_items {
            s.push_str(&format!("  {:<16} {}\n", d, c));
        }

        s.push_str("\n属性列表:\n");
        for attr in &self.attributes {
            s.push_str(&format!("  {}\n", attr));
        }

        s
    }
}

impl fmt::Display for NounSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NounSchema({}, hash=0x{:08X}, {} attributes)",
            self.noun_name,
            self.noun_hash,
            self.attributes.len()
        )
    }
}
