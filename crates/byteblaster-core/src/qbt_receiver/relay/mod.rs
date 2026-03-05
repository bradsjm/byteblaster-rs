//! QBT relay runtime for low-latency passthrough delivery.

mod auth;
mod runtime;
mod state;

use thiserror::Error;

pub use runtime::run as run_qbt_relay;
pub use state::{QbtRelayHealthSnapshot, QbtRelayMetricsSnapshot, QbtRelayState};

use std::net::SocketAddr;
use std::time::Duration;

/// Relay runtime configuration owned by `byteblaster-core`.
#[derive(Debug, Clone)]
pub struct QbtRelayConfig {
    pub email: String,
    pub upstream_servers: Vec<(String, u16)>,
    pub bind_addr: SocketAddr,
    pub max_clients: usize,
    pub auth_timeout: Duration,
    pub client_buffer_bytes: usize,
    pub reconnect_delay: Duration,
    pub connect_timeout: Duration,
    pub quality_window_secs: usize,
    pub quality_pause_threshold: f64,
    pub metrics_log_interval: Duration,
}

impl QbtRelayConfig {
    pub fn validate(&self) -> QbtRelayResult<()> {
        if self.email.trim().is_empty() {
            return Err(QbtRelayError::Config("email must not be empty".to_string()));
        }
        if self.upstream_servers.is_empty() {
            return Err(QbtRelayError::Config(
                "at least one upstream server is required".to_string(),
            ));
        }
        Ok(())
    }
}

pub type QbtRelayResult<T> = Result<T, QbtRelayError>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum QbtRelayError {
    #[error("invalid relay config: {0}")]
    Config(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("relay task join failed: {0}")]
    TaskJoin(String),
}
