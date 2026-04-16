use super::RefNo;

/// B-树索引条目 (16 字节/条)
///
/// 对齐 core.dll 索引页格式:
/// refno_0(4B) + refno_1(4B) + pgno(4B) + packed_offset_flag(4B)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexEntry {
    pub refno: RefNo,
    /// 目标元素所在页号
    pub page_no: u32,
    /// 打包的偏移+标志位
    pub packed: u32,
}

impl IndexEntry {
    pub const SIZE: usize = 16;

    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        assert!(bytes.len() >= Self::SIZE);
        Self {
            refno: RefNo::from_be_bytes(bytes),
            page_no: u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            packed: u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        }
    }

    pub fn to_be_bytes(self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[..8].copy_from_slice(&self.refno.to_be_bytes());
        out[8..12].copy_from_slice(&self.page_no.to_be_bytes());
        out[12..16].copy_from_slice(&self.packed.to_be_bytes());
        out
    }

    pub fn offset(&self) -> u32 {
        self.packed & 0x7FFF_FFFF
    }

    pub fn flag(&self) -> bool {
        (self.packed & 0x8000_0000) != 0
    }
}

/// 元素在数据库中的物理位置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefnoDataLoc {
    pub refno: RefNo,
    pub dbno: u32,
    pub page_no: u32,
    pub offset: u32,
}

/// 索引页头部 (0x1C = 28 字节)
///
/// 对齐 core.dll 索引页结构:
/// type(4B) + noun(4B) + level(4B) + unknowns(12B) + pfno(4B)
pub const INDEX_PAGE_HEADER_SIZE: usize = 0x1C;
pub const INDEX_PAGE_NOUN: u32 = 0x00CC47DF;

#[derive(Debug, Clone)]
pub struct IndexPageHeader {
    pub page_type: u32,
    pub noun: u32,
    /// B-树层级: 0=叶子, >0=内部节点
    pub level: u32,
    pub entry_count: u32,
    pub pfno: u32,
}

impl IndexPageHeader {
    pub fn from_be_bytes(data: &[u8]) -> Self {
        Self {
            page_type: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            noun: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            level: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            entry_count: u32::from_be_bytes([data[16], data[17], data[18], data[19]]),
            pfno: u32::from_be_bytes([data[24], data[25], data[26], data[27]]),
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.level == 0
    }

    pub fn is_index_page(&self) -> bool {
        self.noun == INDEX_PAGE_NOUN
    }
}
