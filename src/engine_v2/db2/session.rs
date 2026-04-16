use crate::engine_v2::db1::PageCache;
use crate::engine_v2::io_layer::FileHandle;
use crate::engine_v2::types::*;

/// 会话链管理 (对齐 db2_get_session_pgid)
///
/// 会话页 (type=3) 形成从 latest_ses_pgno 到最早会话的反向链。
///
/// 会话页布局 (对齐 defines.rs SessionPageData):
/// ```text
/// 0x00-0x03: page_type (=3)
/// 0x04-0x07: last_ses_pageno (前一个会话页)
/// 0x08-0x0B: last_ses_extno
/// 0x0C-0x0F: sesno (会话编号)
/// 0x10-0x13: unknown_0 (0xFFFFFFFF)
/// 0x14-0x17: end_pgno (此会话最后修改页)
/// 0x18-0x1B: end_extno
/// 0x1C-0x1F: index_root_pageno (B-树索引根页)
/// 0x20-0x23: index_root_extno
/// 0x24-0x27: claim_pageno
/// 0x28-0x2B: claim_extno
/// 0x2C-0x33: unknown1,2
/// 0x34-0x37: year
/// 0x38-0x3B: month
/// 0x3C-0x3F: hours
/// 0x40-0x43: seconds
/// 0x44-0x77: unknown_u32[13]
/// 0x78-0x7B: name_words_len
/// 0x7C-...:  name_bytes (name_words_len*4 字节)
/// ```
pub struct SessionManager;

impl SessionManager {
    /// 从会话页解析 SessionPageData
    pub fn parse_session_page(page_no: u32, data: &[u8]) -> Option<SessionPageData> {
        if data.len() < 0x80 { return None; }

        let page_type = i32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        if page_type != 3 { return None; }

        let prev_ses_page = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let _prev_ext = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let ses_no = u32::from_be_bytes([data[0x0C], data[0x0D], data[0x0E], data[0x0F]]);
        let end_pgno = u32::from_be_bytes([data[0x14], data[0x15], data[0x16], data[0x17]]);
        let index_root = u32::from_be_bytes([data[0x1C], data[0x1D], data[0x1E], data[0x1F]]);
        let year = u32::from_be_bytes([data[0x34], data[0x35], data[0x36], data[0x37]]);
        let month = u32::from_be_bytes([data[0x38], data[0x39], data[0x3A], data[0x3B]]);

        let name_words_len = u32::from_be_bytes([data[0x78], data[0x79], data[0x7A], data[0x7B]]);
        let name_start = 0x7C;
        let name_byte_len = (name_words_len.min(128) * 4) as usize;
        let computer_name = if name_start + name_byte_len <= data.len() {
            Self::decode_pdms_string(&data[name_start..name_start + name_byte_len])
        } else {
            String::new()
        };

        Some(SessionPageData {
            page_no,
            ses_no,
            prev_ses_page,
            timestamp: year * 10000 + month,
            computer_name,
            comment: String::new(),
            modified_page_range: Some((0, end_pgno)),
            index_root_pgno: index_root,
        })
    }

    /// 遍历会话链 (latest → prev → ... → 最早)
    pub fn traverse_chain(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        latest_ses_pgno: u32,
    ) -> DbResult<Vec<SessionPageData>> {
        let mut sessions = Vec::new();
        let mut current_pgno = latest_ses_pgno;
        let mut seen = std::collections::HashSet::new();
        let total_pages = handle.total_pages();

        while current_pgno > 0
            && current_pgno < total_pages
            && current_pgno != 0xFFFFFFFF
            && seen.insert(current_pgno)
        {
            let data = cache.get_page(handle, dbno, extent, current_pgno)?;
            let page_id = PageId::new(dbno, current_pgno, extent);

            if let Some(session) = Self::parse_session_page(current_pgno, data) {
                let prev = session.prev_ses_page;
                sessions.push(session);
                cache.unlock_page(&page_id);

                if prev == 0 || prev >= 0x80000000 || prev == current_pgno {
                    break;
                }
                current_pgno = prev;
            } else {
                cache.unlock_page(&page_id);
                break;
            }
        }

        Ok(sessions)
    }

    /// 获取指定会话号对应的页号
    pub fn find_session_page(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        latest_ses_pgno: u32,
        target_sesno: u32,
    ) -> DbResult<Option<u32>> {
        let mut current_pgno = latest_ses_pgno;

        while current_pgno > 0 {
            let data = cache.get_page(handle, dbno, extent, current_pgno)?;
            let page_id = PageId::new(dbno, current_pgno, extent);

            if let Some(session) = Self::parse_session_page(current_pgno, data) {
                cache.unlock_page(&page_id);
                if session.ses_no == target_sesno {
                    return Ok(Some(current_pgno));
                }
                if session.prev_ses_page == 0 || session.prev_ses_page == current_pgno {
                    break;
                }
                current_pgno = session.prev_ses_page;
            } else {
                cache.unlock_page(&page_id);
                break;
            }
        }

        Ok(None)
    }

    /// PDMS 字符串解码：每4字节(big-endian i32)存储1个字符
    /// 格式：name_words_len 个 i32，每个 i32 的低8位是 ASCII 字符
    fn decode_pdms_string(data: &[u8]) -> String {
        let mut result = String::new();
        for chunk in data.chunks(4) {
            if chunk.len() < 4 { break; }
            let val = i32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            if val == 0 { break; }
            let ch = (val & 0xFF) as u8;
            if ch.is_ascii_graphic() || ch == b' ' {
                result.push(ch as char);
            }
        }
        result.trim().to_string()
    }
}
