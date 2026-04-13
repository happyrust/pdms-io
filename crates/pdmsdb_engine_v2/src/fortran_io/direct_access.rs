use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

use crate::core::EngineError;

pub struct DirectAccessToken {
    page_size: usize,
}

impl DirectAccessToken {
    pub fn new(page_size: usize) -> Self {
        Self { page_size }
    }

    pub fn read(
        &self,
        file: &mut File,
        page_no: u32,
        buffer: &mut [u8],
    ) -> Result<(), EngineError> {
        let offset = page_no as u64 * self.page_size as u64;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(buffer)?;
        Ok(())
    }

    pub fn write(
        &self,
        file: &mut File,
        page_no: u32,
        data: &[u8],
    ) -> Result<(), EngineError> {
        let offset = page_no as u64 * self.page_size as u64;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)?;
        Ok(())
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }
}
