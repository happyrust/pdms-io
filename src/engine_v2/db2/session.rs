use crate::engine_v2::db1::PageCache;
use crate::engine_v2::io_layer::FileHandle;
use crate::engine_v2::types::*;

/// 会话链管理 (对齐 db2_get_session_pgid)
///
/// 会话页 (type=3) 形成从 latest_ses_pgno 到最早会话的反向链。
pub struct SessionManager;

impl SessionManager {
    /// 从会话页解析 SessionPageData
    pub fn parse_session_page(page_no: u32, data: &[u8]) -> Option<SessionPageData> {
        if data.len() < 32 { return None; }

        let page_type = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        if page_type != 3 { return None; }

        let ses_no = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let prev_ses_page = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let timestamp = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);

        let computer_name = Self::read_fixed_string(data, 16, 16);
        let comment = if data.len() >= 64 {
            Self::read_fixed_string(data, 32, 32)
        } else {
            String::new()
        };

        Some(SessionPageData {
            page_no,
            ses_no,
            prev_ses_page,
            timestamp,
            computer_name,
            comment,
            modified_page_range: None,
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

        while current_pgno > 0 {
            let data = cache.get_page(handle, dbno, extent, current_pgno)?;
            let page_id = PageId::new(dbno, current_pgno, extent);

            if let Some(session) = Self::parse_session_page(current_pgno, data) {
                let prev = session.prev_ses_page;
                sessions.push(session);
                cache.unlock_page(&page_id);

                if prev == 0 || prev == current_pgno {
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

    fn read_fixed_string(data: &[u8], offset: usize, max_len: usize) -> String {
        let end = (offset + max_len).min(data.len());
        let slice = &data[offset..end];
        let trimmed = slice.iter()
            .take_while(|&&b| b != 0)
            .copied()
            .collect::<Vec<_>>();
        String::from_utf8_lossy(&trimmed).trim().to_string()
    }
}
