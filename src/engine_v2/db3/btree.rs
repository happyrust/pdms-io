use crate::engine_v2::types::{INDEX_PAGE_HEADER_SIZE, IndexEntry, IndexPageHeader, RefNo};

/// 起始标记 RefNo (0x80000001, 0x80000001)
/// B-树非叶子节点的第一个条目，指向最小子树
pub const START_MARKER: RefNo = RefNo {
    hi: 0x80000001,
    lo: 0x80000001,
};

/// 每页最大条目数 (页面大小相关)
pub fn max_entries_per_page(page_size: usize) -> usize {
    (page_size - INDEX_PAGE_HEADER_SIZE) / IndexEntry::SIZE
}

/// B-树节点 (解析后的内存表示)
#[derive(Debug, Clone)]
pub struct BTreeNode {
    pub header: IndexPageHeader,
    pub page_no: u32,
    pub entries: Vec<IndexEntry>,
}

impl BTreeNode {
    /// 从页面原始数据解析
    pub fn from_page_data(page_no: u32, data: &[u8]) -> Self {
        let header = IndexPageHeader::from_be_bytes(data);
        let entry_count = header.entry_count as usize;
        let max = max_entries_per_page(data.len());
        let count = entry_count.min(max);

        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let offset = INDEX_PAGE_HEADER_SIZE + i * IndexEntry::SIZE;
            if offset + IndexEntry::SIZE <= data.len() {
                entries.push(IndexEntry::from_be_bytes(&data[offset..]));
            }
        }

        Self {
            header,
            page_no,
            entries,
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.header.is_leaf()
    }

    /// 过滤掉起始标记，返回有效条目
    pub fn valid_entries(&self) -> impl Iterator<Item = &IndexEntry> {
        self.entries.iter().filter(|e| e.refno != START_MARKER)
    }

    /// 获取起始标记对应的子页面号 (非叶子节点)
    pub fn start_marker_page(&self) -> Option<u32> {
        self.entries
            .iter()
            .find(|e| e.refno == START_MARKER)
            .map(|e| e.page_no)
    }

    /// 二分查找：找到第一个 refno >= target 的条目索引
    pub fn lower_bound(&self, target: &RefNo) -> usize {
        let valid: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.refno != START_MARKER)
            .collect();

        match valid.binary_search_by(|(_, e)| {
            e.refno
                .hi
                .cmp(&target.hi)
                .then_with(|| e.refno.lo.cmp(&target.lo))
        }) {
            Ok(pos) => valid[pos].0,
            Err(pos) => {
                if pos < valid.len() {
                    valid[pos].0
                } else {
                    self.entries.len()
                }
            }
        }
    }
}
