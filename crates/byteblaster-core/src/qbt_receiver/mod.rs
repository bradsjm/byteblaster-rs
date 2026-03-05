pub mod client;
pub mod config;
pub mod error;
pub mod file;
pub mod protocol;
pub mod relay;
pub mod stream;

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
    QbtRelayConfig, QbtRelayHealthSnapshot, QbtRelayMetricsSnapshot, QbtRelayState, run_qbt_relay,
};
