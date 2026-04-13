use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("I/O 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("格式错误: {0}")]
    Format(String),
    #[error("状态错误: {0}")]
    InvalidState(String),
    #[error("不支持: {0}")]
    Unsupported(String),
    #[error("未找到: {0}")]
    NotFound(String),
}
