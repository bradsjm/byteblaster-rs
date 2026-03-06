//! Relay configuration for EMWIN TCP relay mode.
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
//! - `bind`: TCP address for downstream EMWIN clients
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

use clap::Args;
use emwin_protocol::qbt_receiver::{QbtRelayConfig, parse_qbt_server};
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
            emwin_protocol::qbt_receiver::default_qbt_upstream_servers()
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

#[cfg(test)]
mod tests {
    use super::{RelayArgs, RelayConfig};

    #[test]
    fn defaults_to_workspace_server_list_when_no_server_is_provided() {
        let config = RelayConfig::from_args(RelayArgs {
            username: "relay@example.com".to_string(),
            servers: Vec::new(),
            bind: "127.0.0.1:2211".to_string(),
            max_clients: 100,
            auth_timeout_secs: 720,
            client_buffer_bytes: 65_536,
            metrics_bind: "127.0.0.1:9090".to_string(),
            reconnect_delay_secs: 5,
            connect_timeout_secs: 5,
            quality_window_secs: 60,
            quality_pause_threshold: 0.95,
            metrics_log_interval_secs: 30,
        })
        .expect("default relay args should parse");

        assert!(!config.relay.upstream_servers.is_empty());
        assert_eq!(config.relay.email, "relay@example.com");
    }

    #[test]
    fn rejects_invalid_metrics_bind_address() {
        let err = RelayConfig::from_args(RelayArgs {
            username: "relay@example.com".to_string(),
            servers: vec!["127.0.0.1:2211".to_string()],
            bind: "127.0.0.1:2211".to_string(),
            max_clients: 100,
            auth_timeout_secs: 720,
            client_buffer_bytes: 65_536,
            metrics_bind: "bad".to_string(),
            reconnect_delay_secs: 5,
            connect_timeout_secs: 5,
            quality_window_secs: 60,
            quality_pause_threshold: 0.95,
            metrics_log_interval_secs: 30,
        })
        .expect_err("invalid metrics bind must fail");

        assert!(err.to_string().contains("invalid --metrics-bind address"));
    }
}
