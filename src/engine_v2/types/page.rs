/// 页面唯一标识：(dbno, page_id, extent) 三元组
/// 对齐 core.dll 的 db1_is_page_incore 匹配逻辑
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId {
    pub dbno: u32,
    pub page_no: u32,
    pub extent: u32,
}

impl PageId {
    pub fn new(dbno: u32, page_no: u32, extent: u32) -> Self {
        Self {
            dbno,
            page_no,
            extent,
        }
    }
}

impl std::fmt::Display for PageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Page(db={}, pg={}, ext={})",
            self.dbno, self.page_no, self.extent
        )
    }
}

/// 页面类型枚举，对齐 core.dll 页面分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PageType {
    /// 头部页 (page 0)
    Header = 0,
    /// 索引页 (B-树节点)
    Index = 1,
    /// 会话页 (type=3)
    Session = 3,
    /// 元素页 (type=5)，包含元素记录
    Element = 5,
    /// 续页 (type=7)，跨页元素的后续数据
    Continuation = 7,
    /// 未知页类型
    Unknown = 0xFF,
}

impl From<u32> for PageType {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::Header,
            1 => Self::Index,
            3 => Self::Session,
            5 => Self::Element,
            7 => Self::Continuation,
            _ => Self::Unknown,
        }
    }
}

/// 物理帧号描述符 (pfno)
///
/// 对齐 core.dll 的 `dword_6A540EC` 描述符池。
/// 每个 pfno 对应缓存池中的一个槽位。
#[derive(Debug, Clone)]
pub struct PageDescriptor {
    /// 所属的 PageId
    pub id: PageId,
    /// 页面数据 (page_size 字节)
    pub data: Vec<u8>,
    /// 引用计数 (lock_count)，> 0 时不参与 LRU 驱逐
    pub lock_count: u32,
    /// 是否被引用过 (referenced bit, 用于 Clock/LRU 二次机会)
    pub referenced: bool,
    /// 脏页标记，驱逐前必须写回磁盘
    pub dirty: bool,
    /// 最近访问时间戳 (单调递增计数器)
    pub access_tick: u64,
}

impl PageDescriptor {
    pub fn new(id: PageId, data: Vec<u8>) -> Self {
        Self {
            id,
            data,
            lock_count: 0,
            referenced: false,
            dirty: false,
            access_tick: 0,
        }
    }

    pub fn is_locked(&self) -> bool {
        self.lock_count > 0
    }

    pub fn lock(&mut self) {
        self.lock_count += 1;
        self.referenced = true;
    }

    pub fn unlock(&mut self) {
        self.lock_count = self.lock_count.saturating_sub(1);
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn page_type(&self) -> PageType {
        if self.data.len() >= 4 {
            let raw = u32::from_be_bytes([self.data[0], self.data[1], self.data[2], self.data[3]]);
            PageType::from(raw)
        } else {
            PageType::Unknown
        }
    }
}

/// 支持的页面大小
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    B512 = 512,
    B2K = 2048,
    B4K = 4096,
}

impl PageSize {
    pub fn from_raw(v: u32) -> Self {
        match v as usize {
            512 => Self::B512,
            4096 => Self::B4K,
            _ => Self::B2K,
        }
    }

    pub fn bytes(self) -> usize {
        self as usize
    }
}

impl Default for PageSize {
    fn default() -> Self {
        Self::B2K
    }
}
