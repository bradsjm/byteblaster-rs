//! # byteblaster-core
//!
//! Core library for ByteBlaster protocol decoding, client runtime, and file assembly.
//!
//! This crate provides the foundational components for working with the ByteBlaster
//! protocol, including:
//! - Protocol decoding and encoding
//! - Client connection management with reconnect and failover
//! - File assembly from segmented data blocks
//! - Stream abstractions for async data processing

mod client;
mod config;
mod error;
mod file;
mod protocol;
mod stream;

// Public API exports

pub use client::{ByteBlasterClient, Client, ClientBuilder, ClientEvent, ClientTelemetrySnapshot};
pub use config::{ChecksumPolicy, ClientConfig, DecodeConfig, V2CompressionPolicy};
pub use error::{ConfigError, CoreError, CoreResult, ProtocolError};
pub use file::{CompletedFile, FileAssembler, SegmentAssembler};
pub use protocol::checksum::calculate_checksum;
pub use protocol::codec::{FrameDecoder, FrameEncoder, ProtocolDecoder};
pub use protocol::model::{
    AuthMessage, FrameEvent, ProtocolVersion, ProtocolWarning, QbtSegment, ServerList,
};
pub use protocol::server_list::parse_server;

/// Unstable API surface. Items in this module may change without stability guarantees.
pub mod unstable {
    pub use crate::client::reconnect::{EndpointRotator, next_backoff_secs};
    pub use crate::client::watchdog::{HealthObserver, Watchdog};
    pub use crate::protocol::auth::{build_logon_message, xor_ff};
    pub use crate::protocol::server_list::{parse_server_list_frame, parse_simple_server_list};
    pub use crate::stream::file_stream::FileStream;
    pub use crate::stream::segment_stream::SegmentStream;
}
