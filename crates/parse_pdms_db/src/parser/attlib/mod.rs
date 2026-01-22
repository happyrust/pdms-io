use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use nom::{
    number::complete::be_u32,
    IResult,
    multi::count,
};
use anyhow::{Result, Context};

const PAGE_SIZE: usize = 2048;
const RECORD_DELIMITER: u32 = 0xFFFFFFFF;

/// 属性记录结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeRecord {
    pub id: u32,
    pub name: String,
    pub type_code: i32,
    pub type_name: String,
    pub description: String,
    pub short_name: String,
    pub ui_name: String,
    pub category: String,
}

/// 完整的属性库数据
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AttlibData {
    pub attributes: Vec<AttributeRecord>,
    /// 名称到属性索引的映射
    pub name_map: HashMap<String, usize>,
    /// Noun 哈希到属性列表的映射 (来自 ATNAIN)
    pub noun_attr_map: HashMap<u32, Vec<u32>>,
    pub atgtix: Vec<AtgtixEntry>,
    pub atgtdf: Vec<AtgtdfEntry>,
    pub atgtsx: Vec<AtgtsxEntry>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AtgtixEntry {
    pub code: u32,
    pub page: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AtgtsxEntry {
    pub key: u32,
    pub v1: u32,
    pub v2: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtgtdfEntry {
    pub id_or_hash: u32,
    pub tag_or_type: u32,
    pub kind: u32,
    pub ext_index: u32,
}

impl AttlibData {
    pub fn new() -> Self {
        Self::default()
    }

    /// 从文件路径加载并解析 attlib.dat
    pub fn parse_attlib_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = File::open(path.as_ref())
            .with_context(|| format!("Failed to open attlib file: {:?}", path.as_ref()))?;

        // 读取目录页 (Page 1)
        let dir_page = Self::read_page(&mut file, 1)?;
        let attr_records_start_page = dir_page.get(1).copied().unwrap_or(0) as usize;
        let mut candidates: Vec<usize> = dir_page
            .iter()
            .copied()
            .filter(|&v| v > 0 && v != RECORD_DELIMITER)
            .map(|v| v as usize)
            .collect();
        candidates.sort_unstable();
        candidates.dedup();

        let atgtix_start_page = Self::guess_atgtix_start_page(&mut file, &candidates).unwrap_or(None);
        let atgtdf_table_start_page = Self::guess_atgtdf_start_page(&mut file, &candidates).unwrap_or(None);
        let atgtsx_start_page = Self::guess_atgtsx_start_page(&mut file, &candidates).unwrap_or(None);
        let atnain_start_page = dir_page.get(3).copied().map(|v| v as usize).unwrap_or(0);

        // 收集所有记录
        let raw_records = if attr_records_start_page > 0 {
            Self::collect_all_records(&mut file, attr_records_start_page)?
        } else {
            Vec::new()
        };

        // 解析记录
        let mut data = AttlibData::new();
        for rec in raw_records {
            if let Some(attr) = Self::process_record_data(&rec) {
                let idx = data.attributes.len();
                data.name_map.insert(attr.name.clone(), idx);
                data.attributes.push(attr);
            }
        }

        if let Some(start) = atgtix_start_page {
            data.atgtix = Self::parse_atgtix(&mut file, start, 8192)?;
        }
        if let Some(start) = atgtsx_start_page {
            data.atgtsx = Self::parse_atgtsx(&mut file, start, 8192)?;
        }
        if let Some(start) = atgtdf_table_start_page {
            let (entries, _ext) = Self::parse_atgtdf(&mut file, start, 100, 200)?;
            data.atgtdf = entries;
        }

        // 解析 ATNAIN Section (Noun-Attribute 映射)
        if atnain_start_page > 0 {
            Self::parse_atnain(&mut file, atnain_start_page, &mut data)?;
        }

        Ok(data)
    }

    /// 解析 ATNAIN Section - Noun 到属性的映射
    fn parse_atnain(file: &mut File, start_page: usize, data: &mut AttlibData) -> Result<()> {
        // ATNAIN 格式: [NounHash, AttrIndex, TypeCode] 三元组
        for page_idx in start_page..start_page + 30 {
            let page_data = match Self::read_page(file, page_idx) {
                Ok(d) => d,
                Err(_) => break,
            };

            // 遍历三元组
            for i in (0..page_data.len() - 2).step_by(3) {
                let noun_hash = page_data[i];
                let attr_idx = page_data[i + 1];
                let _type_code = page_data[i + 2];

                // 跳过无效数据
                if noun_hash == 0 || noun_hash == RECORD_DELIMITER {
                    continue;
                }
                if attr_idx == 0 || attr_idx > data.attributes.len() as u32 {
                    continue;
                }

                // 添加映射
                data.noun_attr_map
                    .entry(noun_hash)
                    .or_default()
                    .push(attr_idx);
            }
        }
        Ok(())
    }

    fn guess_atgtix_start_page(file: &mut File, candidates: &[usize]) -> Result<Option<usize>> {
        let mut tested = HashSet::new();
        for &p in candidates {
            if !tested.insert(p) {
                continue;
            }
            let r = Self::parse_atgtix(file, p, 32);
            if let Ok(v) = r {
                if !v.is_empty() {
                    return Ok(Some(p));
                }
            }
        }
        Ok(None)
    }

    fn guess_atgtsx_start_page(file: &mut File, candidates: &[usize]) -> Result<Option<usize>> {
        let mut tested = HashSet::new();
        for &p in candidates {
            if !tested.insert(p) {
                continue;
            }
            let r = Self::parse_atgtsx(file, p, 32);
            if let Ok(v) = r {
                if !v.is_empty() {
                    return Ok(Some(p));
                }
            }
        }
        Ok(None)
    }

    fn guess_atgtdf_start_page(file: &mut File, candidates: &[usize]) -> Result<Option<usize>> {
        let mut tested = HashSet::new();
        for &p in candidates {
            if !tested.insert(p) {
                continue;
            }
            let r = Self::parse_atgtdf(file, p, 32, 64);
            if let Ok((v, _)) = r {
                if !v.is_empty() {
                    return Ok(Some(p));
                }
            }
        }
        Ok(None)
    }

    fn parse_atgtix(file: &mut File, start_page: usize, max_entries: u32) -> Result<Vec<AtgtixEntry>> {
        let mut out = Vec::new();
        let mut page_idx = start_page;
        while (out.len() as u32) < max_entries {
            let page = match Self::read_page(file, page_idx) {
                Ok(d) => d,
                Err(_) => break,
            };

            for i in (0..page.len()).step_by(2) {
                if i + 1 >= page.len() {
                    break;
                }
                let w0 = page[i];
                if w0 == 0 {
                    break;
                }
                if w0 == RECORD_DELIMITER {
                    return Ok(out);
                }
                if w0 < 531_442 || w0 > 387_951_929 {
                    break;
                }
                let disp = page[i + 1];
                out.push(AtgtixEntry {
                    code: w0,
                    page: disp / 512,
                    offset: disp % 512,
                });
                if (out.len() as u32) >= max_entries {
                    break;
                }
            }
            page_idx += 1;
        }
        Ok(out)
    }

    fn parse_atgtsx(file: &mut File, start_page: usize, max_entries: u32) -> Result<Vec<AtgtsxEntry>> {
        let mut out = Vec::new();
        let mut page_idx = start_page;
        while (out.len() as u32) < max_entries {
            let page = match Self::read_page(file, page_idx) {
                Ok(d) => d,
                Err(_) => break,
            };

            for i in (0..page.len()).step_by(3) {
                if i + 2 >= page.len() {
                    break;
                }
                let w0 = page[i];
                if w0 == 0 {
                    break;
                }
                if w0 == RECORD_DELIMITER {
                    return Ok(out);
                }
                out.push(AtgtsxEntry {
                    key: w0,
                    v1: page[i + 1],
                    v2: page[i + 2],
                });
                if (out.len() as u32) >= max_entries {
                    break;
                }
            }
            page_idx += 1;
        }
        Ok(out)
    }

    fn parse_atgtdf(
        file: &mut File,
        start_page: usize,
        max_records: u32,
        max_ext: u32,
    ) -> Result<(Vec<AtgtdfEntry>, Vec<u32>)> {
        let mut out = Vec::new();
        let mut ext = Vec::new();
        let mut ext_index: u32 = 0;
        let mut page_idx = start_page;

        while (out.len() as u32) < max_records {
            let page = match Self::read_page(file, page_idx) {
                Ok(d) => d,
                Err(_) => break,
            };

            let mut i = 0usize;
            while i + 2 < page.len() {
                let w0 = page[i];
                if w0 == 0 {
                    break;
                }
                if w0 == RECORD_DELIMITER {
                    return Ok((out, ext));
                }
                if w0 < 531_442 || w0 > 387_951_929 {
                    break;
                }
                let w1 = page[i + 1];
                let kind = page[i + 2];
                i += 3;

                let mut entry_ext_index = 0u32;
                if kind == 2 {
                    ext_index += 1;
                    entry_ext_index = ext_index;
                    if ext_index >= max_ext {
                        break;
                    }

                    if w1 == 4 {
                        if i >= page.len() {
                            break;
                        }
                        let n = page[i] as usize;
                        i += 1;
                        if ext_index + n as u32 >= max_ext {
                            break;
                        }
                        ext.push(n as u32);
                        for _ in 0..n {
                            if i >= page.len() {
                                break;
                            }
                            ext_index += 1;
                            ext.push(page[i]);
                            i += 1;
                        }
                    } else {
                        if i >= page.len() {
                            break;
                        }
                        ext.push(page[i]);
                        i += 1;
                    }
                }

                out.push(AtgtdfEntry {
                    id_or_hash: w0,
                    tag_or_type: w1,
                    kind,
                    ext_index: entry_ext_index,
                });
                if (out.len() as u32) >= max_records {
                    break;
                }
            }
            page_idx += 1;
        }

        Ok((out, ext))
    }

    /// 读取指定页码的数据
    fn read_page(file: &mut File, page_num: usize) -> Result<Vec<u32>> {
        let offset = page_num * PAGE_SIZE;
        file.seek(SeekFrom::Start(offset as u64))?;

        let mut buf = vec![0u8; PAGE_SIZE];
        file.read_exact(&mut buf)?;
        
        let (_, page_data) = Self::parse_page(&buf)
            .map_err(|e| anyhow::anyhow!("Failed to parse page {}: {:?}", page_num, e))?;
        Ok(page_data)
    }

    /// 解析 2048 字节分页数据为 u32 列表
    fn parse_page(input: &[u8]) -> IResult<&[u8], Vec<u32>> {
        use nom::Parser;
        count(be_u32, 512).parse(input)
    }

    /// 收集所有以 0xFFFFFFFF 分隔的记录
    fn collect_all_records(file: &mut File, start_page: usize) -> Result<Vec<Vec<u32>>> {
        let mut records = Vec::new();
        let mut current_rec = Vec::new();
        let mut found_first_delimiter = false;
        
        for page_idx in start_page..start_page + 1500 {
            let page_data = match Self::read_page(file, page_idx) {
                Ok(data) => data,
                Err(_) => break,
            };
            
            for val in page_data {
                if val == RECORD_DELIMITER {
                    if found_first_delimiter && !current_rec.is_empty() {
                        records.push(std::mem::take(&mut current_rec));
                    }
                    found_first_delimiter = true;
                } else if found_first_delimiter {
                    current_rec.push(val);
                }
            }
        }
        
        // 添加最后一条记录
        if !current_rec.is_empty() {
            records.push(current_rec);
        }
        
        Ok(records)
    }

    /// 提取 PDMS 风格字符串 (u32 长度前缀)
    fn extract_string(data: &[u32], start_idx: usize) -> (Option<String>, usize) {
        if start_idx >= data.len() {
            return (None, start_idx);
        }
        let length = data[start_idx] as usize;
        if length > 500 || start_idx + 1 + length > data.len() {
            return (None, start_idx + 1);
        }

        let mut s = String::with_capacity(length);
        for &code in &data[start_idx + 1..start_idx + 1 + length] {
            if (0x20..=0x7E).contains(&code) {
                s.push(code as u8 as char);
            } else {
                s.push('.');
            }
        }
        (Some(s), start_idx + 1 + length)
    }

    /// 解析单个属性记录的数据块
    fn process_record_data(data: &[u32]) -> Option<AttributeRecord> {
        let mut idx = 0;
        // 跳过前导零
        while idx < data.len() && data[idx] == 0 {
            idx += 1;
        }
        if idx >= data.len() { return None; }

        let attr_id = data[idx];
        idx += 1;

        // 提取名称
        let (name, next_idx) = Self::extract_string(data, idx);
        let name = name?;
        idx = next_idx;

        // 提取类型代码
        let mut type_code = 0;
        if idx < data.len() {
            let raw_code = data[idx];
            if raw_code <= 20 {
                type_code = raw_code as i32;
            } else {
                type_code = -1;
            }
            idx += 1;
        }

        let type_name = match type_code {
            1 => "Dimensionless",
            2 => "Distance/Area/Bore",
            3 => "Temperature/Pressure",
            4 => "Volume",
            5 => "Angle",
            6 => "Mass",
            -1 => "Invalid",
            _ => "Unknown",
        }.to_string();

        let mut record = AttributeRecord {
            id: attr_id,
            name,
            type_code,
            type_name,
            description: String::new(),
            short_name: String::new(),
            ui_name: String::new(),
            category: String::new(),
        };

        // 提取后续字符串 (Description, ShortName, UIName, Category)
        let mut strings = Vec::new();
        while idx < data.len() {
            let val = data[idx];
            if val > 0 && val < 500 {
                let is_str = (0..val as usize).all(|k| {
                    idx + 1 + k < data.len() && (0x20..=0x7E).contains(&data[idx + 1 + k])
                });
                
                if is_str {
                    let (s, next_idx) = Self::extract_string(data, idx);
                    if let Some(s) = s {
                        strings.push(s);
                        idx = next_idx;
                        continue;
                    }
                }
            }
            idx += 1;
        }

        if !strings.is_empty() { record.description = strings[0].clone(); }
        if strings.len() >= 2 { record.short_name = strings[1].clone(); }
        if strings.len() >= 3 { record.ui_name = strings[2].clone(); }
        if strings.len() >= 5 { record.category = strings.last().unwrap().clone(); }

        Some(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_attlib() {
        let path = "data/attlib.dat";
        if std::path::Path::new(path).exists() {
            let data = AttlibData::parse_attlib_file(path).unwrap();
            println!("Parsed {} attributes", data.attributes.len());
            println!("Noun-Attr mappings: {} nouns", data.noun_attr_map.len());
            
            assert!(data.attributes.len() > 5000, "Should parse 5000+ attributes");
            
            // 验证已知属性
            if let Some(&idx) = data.name_map.get("XLENGTH") {
                let attr = &data.attributes[idx];
                assert_eq!(attr.type_code, 2, "XLENGTH should have type_code 2");
            }
            
            // 验证 noun_attr_map 有内容
            assert!(!data.noun_attr_map.is_empty(), "noun_attr_map should not be empty");
            
            // 打印一些示例映射
            for (noun_hash, attr_indices) in data.noun_attr_map.iter().take(3) {
                println!("Noun 0x{:08X} -> {} attributes", noun_hash, attr_indices.len());
            }
        }
    }
}
