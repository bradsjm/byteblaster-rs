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

use crate::default_servers::default_upstream_servers;
use anyhow::{Context, Result};
use byteblaster_core::qbt_receiver::parse_qbt_server;
use clap::Args;
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Debug, Clone, Args)]
pub struct RelayArgs {
    #[arg(long)]
    pub email: String,
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
    pub email: String,
    pub upstream_servers: Vec<(String, u16)>,
    pub bind_addr: SocketAddr,
    pub max_clients: usize,
    pub auth_timeout: Duration,
    pub client_buffer_bytes: usize,
    pub metrics_bind_addr: SocketAddr,
    pub reconnect_delay: Duration,
    pub connect_timeout: Duration,
    pub quality_window_secs: usize,
    pub quality_pause_threshold: f64,
    pub metrics_log_interval: Duration,
}

impl RelayConfig {
    pub fn from_args(args: RelayArgs) -> Result<Self> {
        let servers = if args.servers.is_empty() {
            default_upstream_servers()
        } else {
            args.servers
                .iter()
                .map(|raw| {
                    parse_qbt_server(raw)
                        .ok_or_else(|| anyhow::anyhow!("invalid --server entry: {raw}"))
                })
                .collect::<Result<Vec<_>>>()?
        };

        let bind_addr = args
            .bind
            .parse::<SocketAddr>()
            .with_context(|| format!("invalid --bind address: {}", args.bind))?;
        let metrics_bind_addr = args
            .metrics_bind
            .parse::<SocketAddr>()
            .with_context(|| format!("invalid --metrics-bind address: {}", args.metrics_bind))?;
        let quality_pause_threshold = args.quality_pause_threshold.clamp(0.0, 1.0);

        Ok(Self {
            email: args.email,
            upstream_servers: servers,
            bind_addr,
            max_clients: args.max_clients.max(1),
            auth_timeout: Duration::from_secs(args.auth_timeout_secs.max(1)),
            client_buffer_bytes: args.client_buffer_bytes.max(1),
            metrics_bind_addr,
            reconnect_delay: Duration::from_secs(args.reconnect_delay_secs.max(1)),
            connect_timeout: Duration::from_secs(args.connect_timeout_secs.max(1)),
            quality_window_secs: args.quality_window_secs.max(1),
            quality_pause_threshold,
            metrics_log_interval: Duration::from_secs(args.metrics_log_interval_secs.max(1)),
        })
    }
}
