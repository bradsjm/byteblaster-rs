//! Configuration types for byteblaster-core.
//!
//! This module defines configuration structures for the protocol decoder
//! and the client runtime, along with policy enums for checksum validation
//! and compression handling.

use std::path::PathBuf;

/// Policy for handling checksum validation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumPolicy {
    /// Drop segments with invalid checksums and emit a warning.
    StrictDrop,
}

/// Policy for handling V2 protocol compression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V2CompressionPolicy {
    /// Only attempt decompression if the data has a valid zlib header.
    RequireZlibHeader,
    /// Always attempt decompression regardless of header.
    TryAlways,
}

/// Configuration for the protocol decoder.
#[derive(Debug, Clone)]
pub struct DecodeConfig {
    /// Checksum validation policy.
    pub checksum_policy: ChecksumPolicy,
    /// Compression handling policy for V2 frames.
    pub compression_policy: V2CompressionPolicy,
    /// Maximum allowed body size for V2 frames (in bytes).
    pub max_v2_body_size: usize,
}

impl Default for DecodeConfig {
    fn default() -> Self {
        Self {
            checksum_policy: ChecksumPolicy::StrictDrop,
            compression_policy: V2CompressionPolicy::RequireZlibHeader,
            max_v2_body_size: 1024,
        }
    }
}

/// Configuration for the ByteBlaster client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// User email address for authentication.
    pub email: String,
    /// List of server endpoints as (host, port) tuples.
    pub servers: Vec<(String, u16)>,
    /// Optional path to persist and load server list.
    pub server_list_path: Option<PathBuf>,
    /// Whether runtime server-list updates should replace configured endpoints.
    pub follow_server_list_updates: bool,
    /// Delay between reconnection attempts (in seconds).
    pub reconnect_delay_secs: u64,
    /// Timeout for establishing connections (in seconds).
    pub connection_timeout_secs: u64,
    /// Timeout for watchdog health checks (in seconds).
    pub watchdog_timeout_secs: u64,
    /// Maximum number of exceptions before triggering watchdog timeout.
    pub max_exceptions: u32,
    /// Decoder configuration.
    pub decode: DecodeConfig,
}

impl ClientConfig {
    /// Validates the configuration.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::EmptyEmail` if email is empty or whitespace.
    /// Returns `ConfigError::NoServers` if no servers are configured.
    pub fn validate(&self) -> Result<(), crate::error::ConfigError> {
        if self.email.trim().is_empty() {
            return Err(crate::error::ConfigError::EmptyEmail);
        }
        if self.servers.is_empty() {
            return Err(crate::error::ConfigError::NoServers);
        }
        Ok(())
    }
}
