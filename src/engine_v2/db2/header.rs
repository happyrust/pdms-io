use crate::engine_v2::io_layer::FileHandle;
use crate::engine_v2::types::*;

/// 文件头部读写 (对齐 db2_modify_header_page)
///
/// PdmsHeader 布局: 偏移 0x00~0x3F，共 64 字节。
pub struct HeaderManager;

impl HeaderManager {
    /// 从数据库文件读取头部
    pub fn read(handle: &mut FileHandle) -> DbResult<DbHeader> {
        let mut buf = vec![0u8; handle.page_size()];
        handle.read_page(0, &mut buf)?;
        Ok(Self::parse(&buf))
    }

    /// 解析头部数据
    pub fn parse(data: &[u8]) -> DbHeader {
        DbHeader {
            version: i32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            db_num: i32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            flags: i32::from_be_bytes([data[0x18], data[0x19], data[0x1A], data[0x1B]]),
            creation_time: u32::from_be_bytes([data[0x20], data[0x21], data[0x22], data[0x23]]),
            latest_ses_pgno: u32::from_be_bytes([data[0x28], data[0x29], data[0x2A], data[0x2B]]),
            ext_no: u32::from_be_bytes([data[0x2C], data[0x2D], data[0x2E], data[0x2F]]),
            session_page_no: u32::from_be_bytes([data[0x30], data[0x31], data[0x32], data[0x33]]),
            page_size: u32::from_be_bytes([data[0x34], data[0x35], data[0x36], data[0x37]]),
            stored_page_count: u32::from_be_bytes([data[0x38], data[0x39], data[0x3A], data[0x3B]]),
        }
    }

    /// 修改头部页 (对齐 db2_modify_header_page)
    pub fn write(handle: &mut FileHandle, header: &DbHeader) -> DbResult<()> {
        let mut buf = vec![0u8; handle.page_size()];
        handle.read_page(0, &mut buf)?;

        buf[4..8].copy_from_slice(&header.version.to_be_bytes());
        buf[8..12].copy_from_slice(&header.db_num.to_be_bytes());
        buf[0x18..0x1C].copy_from_slice(&header.flags.to_be_bytes());
        buf[0x20..0x24].copy_from_slice(&header.creation_time.to_be_bytes());
        buf[0x28..0x2C].copy_from_slice(&header.latest_ses_pgno.to_be_bytes());
        buf[0x2C..0x30].copy_from_slice(&header.ext_no.to_be_bytes());
        buf[0x30..0x34].copy_from_slice(&header.session_page_no.to_be_bytes());
        buf[0x34..0x38].copy_from_slice(&header.page_size.to_be_bytes());
        buf[0x38..0x3C].copy_from_slice(&header.stored_page_count.to_be_bytes());

        handle.write_page(0, &buf)
    }

    /// 获取库级整型属性 (对齐 db2_get_db_int_att)
    pub fn get_int_att(data: &[u8], offset: usize) -> i32 {
        if offset + 4 <= data.len() {
            i32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ])
        } else {
            0
        }
    }

    /// 设置库级整型属性 (对齐 db2_set_db_int_att)
    pub fn set_int_att(data: &mut [u8], offset: usize, value: i32) {
        if offset + 4 <= data.len() {
            data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        }
    }
}
