//! Error types for byteblaster-core.
//!
//! This module defines the error hierarchy used throughout the crate,
//! providing typed errors for configuration, protocol, and I/O failures.

use thiserror::Error;

/// Result type alias using [`CoreError`] as the error type.
pub type CoreResult<T> = Result<T, CoreError>;

/// Primary error type for the byteblaster-core crate.
///
/// This enum represents all possible errors that can occur when using
/// the core library. It is marked as `#[non_exhaustive]` to allow
/// future variants to be added without breaking changes.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// Configuration validation failed.
    #[error("invalid config: {0}")]
    Config(#[from] ConfigError),
    /// Protocol parsing or validation error.
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    /// Underlying I/O operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Client lifecycle operation failed (start/stop).
    #[error("client lifecycle error: {0}")]
    Lifecycle(String),
}

/// Errors related to client configuration validation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// Email address is empty or whitespace-only.
    #[error("email must not be empty")]
    EmptyEmail,
    /// No servers were configured.
    #[error("at least one server is required")]
    NoServers,
}

/// Errors related to protocol parsing and validation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProtocolError {
    /// Frame header does not match expected format.
    #[error("invalid frame header")]
    InvalidHeader,
    /// Body length is out of valid range.
    #[error("invalid body length: {0}")]
    InvalidBodyLength(usize),
    /// Frame type is not supported.
    #[error("unsupported frame")]
    UnsupportedFrame,
    /// Required field is missing from the frame.
    #[error("missing field: {0}")]
    MissingField(&'static str),
    /// Zlib decompression failed.
    #[error("decompression failed: {0}")]
    Decompression(String),
    /// Checksum validation failed.
    #[error("checksum mismatch")]
    ChecksumMismatch,
    /// Frame type is invalid for current state.
    #[error("invalid frame type")]
    InvalidFrameType,
    /// Frame contains invalid UTF-8 sequences.
    #[error("invalid utf8 in frame: {0}")]
    InvalidUtf8(String),
    /// UTF-8 decoding failed.
    #[error("utf8 decode failed: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}
