use std::thread;
use std::time::Duration;

use crate::core::EngineError;
use crate::fortran_io::direct_access::DirectAccessToken;
use crate::fortran_io::file_ops::{FileOpenMode, FileToken};

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub wait_ms: u64,
    pub switch_mode_on_failure: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            wait_ms: 500,
            switch_mode_on_failure: true,
        }
    }
}

pub fn read_page_with_retry(
    token: &DirectAccessToken,
    file_token: &mut FileToken,
    page_no: u32,
    policy: &RetryPolicy,
) -> Result<Vec<u8>, EngineError> {
    let mut buffer = vec![0u8; token.page_size()];
    let file = file_token.file_mut()?;

    match token.read(file, page_no, &mut buffer) {
        Ok(()) => return Ok(buffer),
        Err(_first_err) => {}
    }

    for attempt in 0..policy.max_retries {
        thread::sleep(Duration::from_millis(policy.wait_ms));

        let file = file_token.file_mut()?;
        match token.read(file, page_no, &mut buffer) {
            Ok(()) => return Ok(buffer),
            Err(_) if attempt + 1 < policy.max_retries && policy.switch_mode_on_failure => {
                let original_mode = file_token.mode();
                let _ = file_token.switch_mode(FileOpenMode::ReadOnly);
                let _ = file_token.switch_mode(original_mode);
            }
            Err(e) => {
                if attempt + 1 == policy.max_retries {
                    return Err(e);
                }
            }
        }
    }

    Err(EngineError::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("页面 {} 读取失败（已重试 {} 次）", page_no, policy.max_retries),
    )))
}
