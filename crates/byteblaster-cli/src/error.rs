//! Error types for the ByteBlaster CLI.
//!
//! This module defines the error types used throughout the CLI application,
//! providing a unified error handling interface that wraps errors from
//! underlying libraries (byteblaster-core) and CLI-specific errors.

use thiserror::Error;

/// Result type alias for CLI operations.
pub type CliResult<T> = std::result::Result<T, CliError>;

#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    AddrParse(#[from] std::net::AddrParseError),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    QbtProtocol(#[from] byteblaster_core::qbt_receiver::QbtProtocolError),
    #[error(transparent)]
    QbtReceiver(#[from] byteblaster_core::qbt_receiver::QbtReceiverError),
    #[error(transparent)]
    WxWireReceiver(#[from] byteblaster_core::wxwire_receiver::WxWireReceiverError),
    #[error(transparent)]
    Ingest(#[from] byteblaster_core::ingest::IngestError),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("runtime failure: {0}")]
    Runtime(String),
}

impl CliError {
    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Self::InvalidArgument(msg.into())
    }

    pub fn runtime(msg: impl Into<String>) -> Self {
        Self::Runtime(msg.into())
    }
}
