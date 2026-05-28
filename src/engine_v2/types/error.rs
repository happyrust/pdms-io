use std::io;
use std::path::PathBuf;

/// engine_v2 统一错误类型
#[derive(Debug)]
pub enum DbError {
    Io(io::Error),
    /// 页面号超出文件范围
    PageOutOfRange {
        page_no: u32,
        file_pages: u32,
    },
    /// 页面类型不匹配
    PageTypeMismatch {
        expected: u32,
        got: u32,
        page_no: u32,
    },
    /// B-树结构损坏
    IndexCorrupted {
        msg: String,
    },
    /// 缓存池已满且所有页面被锁定
    CacheExhausted,
    /// 文件锁竞争
    FileLocked {
        path: PathBuf,
    },
    /// 重试次数耗尽
    RetryExhausted {
        attempts: u32,
        last_err: io::Error,
    },
    /// 无效的 RefNo
    InvalidRefNo {
        refno_hi: u32,
        refno_lo: u32,
    },
    /// 通用错误
    Other(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::PageOutOfRange {
                page_no,
                file_pages,
            } => write!(
                f,
                "page {page_no} out of range (file has {file_pages} pages)"
            ),
            Self::PageTypeMismatch {
                expected,
                got,
                page_no,
            } => write!(f, "page {page_no}: expected type {expected}, got {got}"),
            Self::IndexCorrupted { msg } => write!(f, "index corrupted: {msg}"),
            Self::CacheExhausted => write!(f, "page cache exhausted (all slots locked)"),
            Self::FileLocked { path } => write!(f, "file locked: {}", path.display()),
            Self::RetryExhausted { attempts, last_err } => {
                write!(f, "retry exhausted after {attempts} attempts: {last_err}")
            }
            Self::InvalidRefNo { refno_hi, refno_lo } => {
                write!(f, "invalid RefNo({refno_hi:#X}:{refno_lo:#X})")
            }
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for DbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::RetryExhausted { last_err, .. } => Some(last_err),
            _ => None,
        }
    }
}

impl From<io::Error> for DbError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub type DbResult<T> = Result<T, DbError>;
