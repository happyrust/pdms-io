/// 会话页数据 (type=3)
///
/// 对齐 core.dll 的 db2_get_session_pgid 结构。
/// 每个会话页记录一次数据库修改操作的元信息。
#[derive(Debug, Clone)]
pub struct SessionPageData {
    pub page_no: u32,
    /// 会话序号
    pub ses_no: u32,
    /// 前一个会话页号 (形成反向链)
    pub prev_ses_page: u32,
    /// 时间戳 (PDMS 内部格式)
    pub timestamp: u32,
    /// 计算机名
    pub computer_name: String,
    /// 注释
    pub comment: String,
    /// 此会话修改的页面范围 (start_page..=end_page)
    pub modified_page_range: Option<(u32, u32)>,
}

/// 文件头部信息
///
/// 对齐 core.dll 的 PdmsHeader (偏移 0x00~0x3F)。
#[derive(Debug, Clone)]
pub struct DbHeader {
    pub version: i32,
    pub db_num: i32,
    pub flags: i32,
    pub creation_time: u32,
    /// 最新会话页号 (latest_ses_pgno)，会话链遍历起点
    pub latest_ses_pgno: u32,
    pub ext_no: u32,
    pub session_page_no: u32,
    /// 页面大小 (运行时检测: 512/2048/4096)
    pub page_size: u32,
    pub stored_page_count: u32,
}

impl DbHeader {
    pub fn detected_page_size(&self) -> usize {
        match self.page_size as usize {
            512 => 512,
            4096 => 4096,
            _ => 2048,
        }
    }
}
