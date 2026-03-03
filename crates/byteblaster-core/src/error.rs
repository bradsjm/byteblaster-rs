use thiserror::Error;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    #[error("invalid config: {0}")]
    Config(#[from] ConfigError),
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("client lifecycle error: {0}")]
    Lifecycle(String),
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("email must not be empty")]
    EmptyEmail,
    #[error("at least one server is required")]
    NoServers,
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProtocolError {
    #[error("invalid frame header")]
    InvalidHeader,
    #[error("invalid body length: {0}")]
    InvalidBodyLength(usize),
    #[error("unsupported frame")]
    UnsupportedFrame,
    #[error("missing field: {0}")]
    MissingField(&'static str),
    #[error("decompression failed: {0}")]
    Decompression(String),
    #[error("checksum mismatch")]
    ChecksumMismatch,
    #[error("invalid frame type")]
    InvalidFrameType,
    #[error("invalid utf8 in frame: {0}")]
    InvalidUtf8(String),
    #[error("utf8 decode failed: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}
