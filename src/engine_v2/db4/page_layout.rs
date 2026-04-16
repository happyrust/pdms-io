/// 元素页面数据布局 (对齐 core.dll 元素页结构)
///
/// ## type=5 元素页
/// ```text
/// ┌────────────────────────────────┐
/// │ 页头 (16B)                      │
/// │   word[0]: page_type = 5       │
/// │   word[1]: magic               │
/// │   word[2]: 元素计数             │
/// │   word[3]: 下一页链接           │
/// ├────────────────────────────────┤
/// │ 元素记录 0 ...                  │
/// │   impl_len(4B) → refno(8B)     │
/// │   → type_hash(4B) → owner(8B)  │
/// │   → 固定属性区 → 可变属性区     │
/// ├────────────────────────────────┤
/// │ 空闲空间 / 填充                 │
/// └────────────────────────────────┘
/// ```
///
/// ## type=7 续页
/// 当元素数据超过单页时，通过续页链接拼接。

use crate::engine_v2::types::RefNo;

const ELEMENT_PAGE_HEADER_SIZE: usize = 16;

/// 元素页头部
#[derive(Debug, Clone)]
pub struct ElementPageHeader {
    pub page_type: u32,
    pub magic: u32,
    pub element_count: u32,
    /// 下一页号 (negative = 最后一页)
    pub next_page: i32,
}

impl ElementPageHeader {
    pub fn from_be_bytes(data: &[u8]) -> Self {
        Self {
            page_type: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            magic: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            element_count: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            next_page: i32::from_be_bytes([data[12], data[13], data[14], data[15]]),
        }
    }

    pub fn is_element_page(&self) -> bool {
        self.page_type == 5
    }

    pub fn is_continuation(&self) -> bool {
        self.page_type == 7
    }

    pub fn has_next(&self) -> bool {
        self.next_page > 0
    }
}

/// 元素记录头部 (位于元素页内部)
#[derive(Debug, Clone)]
pub struct ElementRecordHeader {
    pub impl_len: u32,
    pub refno: RefNo,
    pub type_hash: u32,
    pub owner: RefNo,
}

impl ElementRecordHeader {
    pub const SIZE: usize = 24;

    pub fn from_be_bytes(data: &[u8]) -> Self {
        Self {
            impl_len: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            refno: RefNo::from_be_bytes(&data[4..12]),
            type_hash: u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
            owner: RefNo::from_be_bytes(&data[16..24]),
        }
    }

    pub fn total_record_size(&self) -> usize {
        self.impl_len as usize * 4
    }
}

/// 跨页元素数据读取器
///
/// 对齐 ElementRecordReader 的 type=7 续页拼接逻辑。
pub struct ContinuationReader;

impl ContinuationReader {
    /// 标记字节: 续页段 (0x00000007)
    const CONTINUATION_MARKER: [u8; 4] = [0x00, 0x00, 0x00, 0x07];
    /// 标记字节: 数据结束 (4 个 0x00)
    const END_MARKER: [u8; 4] = [0x00, 0x00, 0x00, 0x00];

    /// 在原始数据中查找元素记录结束位置
    pub fn find_record_end(data: &[u8]) -> Option<usize> {
        if data.len() < 8 { return None; }

        for i in (0..data.len() - 7).step_by(4) {
            if &data[i..i + 4] == &Self::END_MARKER
                && (i + 4 >= data.len() || &data[i + 4..i + 8] == &Self::END_MARKER)
            {
                return Some(i);
            }
        }

        None
    }
}
