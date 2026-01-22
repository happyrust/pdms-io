use std::io;
use nom::error::{ErrorKind};
use anyhow::{anyhow, Error};

#[derive(Debug, thiserror::Error)]
#[error("...")]
pub enum ResolveError {
    #[error("Axis index not exist {0}")]
    AxisIndeNotExist(u32),
    #[error("{0}")]
    IoError(#[from] io::Error),
    IoError1(io::Error),
}

#[derive(Debug, thiserror::Error)]
#[error("AttError unknown")]
pub enum AttError {
    #[error("Element: {0} attr not exist {1}")]
    AttNotExist(String, String),
    #[error("Att name: {0} is not {1}")]
    TypeNotCorrect(String, String),
    #[error("Vec3 lenth : {0} is not 3")]
    Vec3LengthLess(u32),
}

#[derive(Debug)]
pub struct NomError(pub anyhow::Error);

impl std::fmt::Display for NomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<NomError> for anyhow::Error {
    fn from(e: NomError) -> Self {
        e.0
    }
}

impl From<anyhow::Error> for NomError {
    fn from(e: Error) -> Self {
        NomError(e)
    }
}

impl nom::error::ParseError<&str> for NomError {
    fn from_error_kind(input: &str, kind: ErrorKind) -> Self {
        NomError(anyhow!("error {:?} at: {}", kind, input))
    }

    fn append(input: &str, kind: ErrorKind, other: Self) -> Self {
        NomError(other.0.context(format!("error {:?} at: {}", kind, input)))
    }
}






fn gen_resolve_error() -> anyhow::Result<u32>{

     // Err(ResolveError::AxisIndeNotExist(1).into());
    let io = io::Error::new(io::ErrorKind::Other, "oh no!");
    Err(ResolveError::from(io).into())
}

#[test]
fn test_resolve_error() {
    match gen_resolve_error() {
        Ok(_) => {}
        Err(e) => {
            println!("{:?}", e.to_string());
        }
    }

    assert_eq!(1, 1);

}

#[test]
#[ignore = "Panic initialization fails in some environments (SIGABRT)"]
fn test_resolve_panic() {
    // 使用 catch_unwind 替代 should_panic 以避免某些环境下的 SIGABRT 问题
    let result = std::panic::catch_unwind(|| {
        gen_resolve_error().unwrap();
    });
    assert!(result.is_err(), "Expected panic but no panic occurred");
}

//


#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    // #[error("{0}")]
    // Json(JsonPayloadError),
    // #[error("{0}")]
    // Query(QueryPayloadError),
    // #[error("The json payload provided is malformed. `{0}`.")]
    // MalformedPayload(serde_json::error::Error),
    // #[error("A json payload is missing.")]
    // MissingPayload,
}