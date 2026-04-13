use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

use crate::core::EngineError;

#[derive(Debug, Clone)]
pub struct HeaderView {
    pub version: u32,
    pub db_num: u32,
    pub latest_ses_pgno: u32,
    pub ext_no: u32,
    pub session_page_no: u32,
    pub page_size: u32,
    pub stored_page_count: u32,
}

impl HeaderView {
    pub fn read_from(file: &mut File) -> Result<Self, EngineError> {
        file.seek(SeekFrom::Start(0))?;
        let mut buf = [0u8; 64];
        file.read_exact(&mut buf)?;
        Self::from_bytes(&buf)
    }

    pub fn from_bytes(buf: &[u8]) -> Result<Self, EngineError> {
        if buf.len() < 64 {
            return Err(EngineError::Format("头部长度不足 64 字节".into()));
        }

        let read_u32 =
            |start: usize| -> u32 { u32::from_be_bytes(buf[start..start + 4].try_into().unwrap()) };

        Ok(Self {
            version: read_u32(0x04),
            db_num: read_u32(0x08),
            latest_ses_pgno: read_u32(0x28),
            ext_no: read_u32(0x2C),
            session_page_no: read_u32(0x30),
            page_size: read_u32(0x34),
            stored_page_count: read_u32(0x38),
        })
    }
}

pub struct HeaderUpdaterV2;

impl HeaderUpdaterV2 {
    pub fn update_latest_ses_pgno(file: &mut File, new_ses_pgno: u32) -> Result<(), EngineError> {
        file.seek(SeekFrom::Start(0x28))?;
        file.write_all(&new_ses_pgno.to_be_bytes())?;
        Ok(())
    }

    pub fn update_stored_page_count(file: &mut File, page_count: u32) -> Result<(), EngineError> {
        file.seek(SeekFrom::Start(0x38))?;
        file.write_all(&page_count.to_be_bytes())?;
        Ok(())
    }

    pub fn update_header(
        file: &mut File,
        ses_pgno: u32,
        page_count: u32,
    ) -> Result<(), EngineError> {
        Self::update_latest_ses_pgno(file, ses_pgno)?;
        Self::update_stored_page_count(file, page_count)?;
        file.flush()?;
        Ok(())
    }
}
