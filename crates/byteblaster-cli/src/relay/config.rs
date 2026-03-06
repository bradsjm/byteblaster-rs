//! Relay configuration for ByteBlaster TCP relay mode.
//!
//! This module provides configuration structures and parsing logic for
//! the `relay` command, which runs a low-latency TCP passthrough server.
//!
//! ## Configuration
//!
//! Relay behavior is controlled by command-line arguments defined in
//! [`RelayArgs`], which are parsed by `clap` and then converted to a
//! structured [`RelayConfig`].
//!
//! ## Key Parameters
//!
//! - `bind`: TCP address for downstream ByteBlaster clients
//! - `max_clients`: Maximum concurrent downstream connections
//! - `auth_timeout_secs`: Re-authentication window for clients
//! - `client_buffer_bytes`: Per-client backpressure budget
//! - `quality_*`: Parameters for quality-based forwarding control
//!
//! ## Defaults
//!
//! - Bind: `0.0.0.0:2211`
//! - Max clients: `100`
//! - Auth timeout: `720` seconds
//! - QbtReceiver buffer: `65536` bytes
//! - Quality window: `60` seconds

use byteblaster_core::qbt_receiver::{QbtRelayConfig, parse_qbt_server};
use clap::Args;
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Debug, Clone, Args)]
pub struct RelayArgs {
    #[arg(long)]
    pub username: String,
    #[arg(long = "server", value_delimiter = ',')]
    pub servers: Vec<String>,
    #[arg(long, default_value = "0.0.0.0:2211")]
    pub bind: String,
    #[arg(long, default_value_t = 100)]
    pub max_clients: usize,
    #[arg(long, default_value_t = 720)]
    pub auth_timeout_secs: u64,
    #[arg(long, default_value_t = 65_536)]
    pub client_buffer_bytes: usize,
    #[arg(long, default_value = "127.0.0.1:9090")]
    pub metrics_bind: String,
    #[arg(long, default_value_t = 5)]
    pub reconnect_delay_secs: u64,
    #[arg(long, default_value_t = 5)]
    pub connect_timeout_secs: u64,
    #[arg(long, default_value_t = 60)]
    pub quality_window_secs: usize,
    #[arg(long, default_value_t = 0.95)]
    pub quality_pause_threshold: f64,
    #[arg(long, default_value_t = 30)]
    pub metrics_log_interval_secs: u64,
}

#[derive(Debug, Clone)]
pub struct RelayConfig {
    pub metrics_bind_addr: SocketAddr,
    pub relay: QbtRelayConfig,
}

impl RelayConfig {
    pub fn from_args(args: RelayArgs) -> crate::error::CliResult<Self> {
        let servers = if args.servers.is_empty() {
            byteblaster_core::qbt_receiver::default_qbt_upstream_servers()
        } else {
            args.servers
                .iter()
                .map(|raw| {
                    parse_qbt_server(raw).ok_or_else(|| {
                        crate::error::CliError::invalid_argument(format!(
                            "invalid --server entry: {raw}"
                        ))
                    })
                })
                .collect::<crate::error::CliResult<Vec<_>>>()?
        };

        let bind_addr = args.bind.parse::<SocketAddr>().map_err(|err| {
            crate::error::CliError::invalid_argument(format!(
                "invalid --bind address {}: {err}",
                args.bind
            ))
        })?;
        let metrics_bind_addr = args.metrics_bind.parse::<SocketAddr>().map_err(|err| {
            crate::error::CliError::invalid_argument(format!(
                "invalid --metrics-bind address {}: {err}",
                args.metrics_bind
            ))
        })?;
        Ok(Self {
            metrics_bind_addr,
            relay: QbtRelayConfig {
                email: args.username,
                upstream_servers: servers,
                bind_addr,
                max_clients: args.max_clients,
                auth_timeout: Duration::from_secs(args.auth_timeout_secs),
                client_buffer_bytes: args.client_buffer_bytes,
                reconnect_delay: Duration::from_secs(args.reconnect_delay_secs),
                connect_timeout: Duration::from_secs(args.connect_timeout_secs),
                quality_window_secs: args.quality_window_secs,
                quality_pause_threshold: args.quality_pause_threshold,
                metrics_log_interval: Duration::from_secs(args.metrics_log_interval_secs),
            },
        })
    }
}
