//! ByteBlaster QBT satellite receiver client.
//!
//! This module provides a complete client implementation for receiving products via the
//! ByteBlaster QBT satellite broadcast protocol.
//!
//! ## Architecture
//!
//! The receiver is organized into several components:
//!
//! - **Protocol** (`protocol`): WMO header parsing, checksum validation, compression handling,
//!   server-list parsing, and authentication message encoding/decoding
//! - **Client** (`client`): Connection management with reconnect/backoff, auth ticker, watchdog,
//!   and handler isolation for error resilience
//! - **File assembly** (`file`): Reassembles multi-segment files with duplicate suppression
//!   and inflight entry expiration
//! - **Stream** (`stream`): Adapter types for receiving segments or completed files as streams
//! - **Relay** (`relay`): Low-latency TCP passthrough with metrics and client management
//!
//! ## Example
//!
//! ```rust,no_run
//! use byteblaster_core::qbt_receiver::{
//!     QbtDecodeConfig, QbtReceiver, QbtReceiverClient, QbtReceiverConfig,
//!     default_qbt_upstream_servers,
//! };
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = QbtReceiverConfig {
//!         email: "you@example.com".to_string(),
//!         servers: default_qbt_upstream_servers(),
//!         server_list_path: None,
//!         follow_server_list_updates: true,
//!         reconnect_delay_secs: 5,
//!         connection_timeout_secs: 5,
//!         watchdog_timeout_secs: 49,
//!         max_exceptions: 10,
//!         decode: QbtDecodeConfig::default(),
//!     };
//!     let mut receiver = QbtReceiver::builder(config).build()?;
//!     receiver.start()?;
//!     receiver.stop().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Configurable Policies
//!
//! - **Checksum policy**: Strict (drop on failure) or Lenient (emit warning)
//! - **V2 compression policy**: Strict (drop on failure) or Lenient (emit warning)
//! - **Server list rotation**: Automatic fallback when primary pool is exhausted
//!
//! See [`QbtReceiverConfig`] for all configuration options.

mod client;
mod config;
mod error;
mod file;
mod protocol;
mod relay;
mod stream;

pub use client::{
    QbtReceiver, QbtReceiverBuilder, QbtReceiverClient, QbtReceiverEvent,
    QbtReceiverTelemetrySnapshot,
};
pub use config::{DEFAULT_QBT_UPSTREAM_SERVERS, default_qbt_upstream_servers};
pub use config::{QbtChecksumPolicy, QbtDecodeConfig, QbtReceiverConfig, QbtV2CompressionPolicy};
pub use error::{QbtProtocolError, QbtReceiverConfigError, QbtReceiverError, QbtReceiverResult};
pub use file::{QbtCompletedFile, QbtFileAssembler, QbtSegmentAssembler};
pub use protocol::auth::{
    LOGON_PREFIX, LOGON_SUFFIX, REAUTH_INTERVAL_SECS, build_logon_message, parse_logon_message,
    xor_ff,
};
pub use protocol::checksum::calculate_qbt_checksum;
pub use protocol::codec::{QbtFrameDecoder, QbtFrameEncoder, QbtProtocolDecoder};
pub use protocol::model::{
    QbtAuthMessage, QbtFrameEvent, QbtProtocolVersion, QbtProtocolWarning, QbtSegment,
    QbtServerList,
};
pub use protocol::server_list::parse_qbt_server;
pub use protocol::server_list_wire::{QbtServerListWireScanner, build_server_list_wire};
pub use relay::{
    QbtRelayConfig, QbtRelayError, QbtRelayHealthSnapshot, QbtRelayMetricsSnapshot, QbtRelayResult,
    QbtRelayState, run_qbt_relay,
};

pub mod unstable {
    pub use super::client::reconnect::{EndpointRotator, next_backoff_secs};
    pub use super::client::watchdog::{HealthObserver, Watchdog};
    pub use super::protocol::auth::{build_logon_message, xor_ff};
    pub use super::protocol::server_list::{parse_server_list_frame, parse_simple_server_list};
    pub use super::stream::file_stream::QbtFileStream;
    pub use super::stream::segment_stream::QbtSegmentStream;
}
