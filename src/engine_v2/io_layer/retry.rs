use std::thread;
use std::time::Duration;

use super::FileHandle;
use crate::engine_v2::types::{DbError, DbResult};

/// 重试策略配置
///
/// 替代 core.dll 的 SYWAIT + FHSWIT 重试机制：
/// - err=11 (文件锁冲突) → 等待 0.5s → 重开文件句柄 → 重试
/// - 最多 3 次
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub wait_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            wait_ms: 500,
        }
    }
}

impl RetryPolicy {
    /// 带重试地读取页面
    ///
    /// 失败时：sleep → reopen file handle → retry
    pub fn read_page_with_retry(
        &self,
        handle: &mut FileHandle,
        page_no: u32,
        buf: &mut [u8],
    ) -> DbResult<()> {
        let mut last_err = None;

        for attempt in 0..self.max_attempts {
            match handle.read_page(page_no, buf) {
                Ok(()) => return Ok(()),
                Err(DbError::Io(e)) if is_retryable(&e) => {
                    log::warn!(
                        "page read failed (attempt {}/{}): {} — retrying in {}ms",
                        attempt + 1,
                        self.max_attempts,
                        e,
                        self.wait_ms
                    );
                    thread::sleep(Duration::from_millis(self.wait_ms));

                    if let Err(reopen_err) = handle.reopen() {
                        log::warn!("reopen failed: {reopen_err}");
                    }

                    last_err = Some(e);
                }
                Err(e) => return Err(e),
            }
        }

        Err(DbError::RetryExhausted {
            attempts: self.max_attempts,
            last_err: last_err.unwrap_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Other, "unknown retry failure")
            }),
        })
    }
}

fn is_retryable(e: &std::io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(11) | Some(32) | Some(33) // EAGAIN | ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION
    ) || matches!(
        e.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
    )
}
