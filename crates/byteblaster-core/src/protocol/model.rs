use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ProtocolVersion {
    V1,
    V2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct QbtSegment {
    pub filename: String,
    pub block_number: u32,
    pub total_blocks: u32,
    pub content: Bytes,
    pub checksum: u32,
    pub length: usize,
    pub version: ProtocolVersion,
    pub timestamp_utc: SystemTime,
    pub source: Option<SocketAddr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ServerList {
    pub servers: Vec<(String, u16)>,
    pub sat_servers: Vec<(String, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ProtocolWarning {
    ChecksumMismatch {
        filename: String,
        block_number: u32,
    },
    DecompressionFailed {
        filename: String,
        block_number: u32,
        reason: String,
    },
    DecoderRecovered {
        error: String,
    },
    MalformedServerEntry {
        entry: String,
    },
    TimestampParseFallback {
        raw: String,
    },
    HandlerError {
        message: String,
    },
    BackpressureDrop {
        dropped_since_last_report: u64,
        total_dropped_events: u64,
        decoder_recovery_events: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FrameEvent {
    DataBlock(QbtSegment),
    ServerListUpdate(ServerList),
    Warning(ProtocolWarning),
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AuthMessage {
    pub email: String,
}
