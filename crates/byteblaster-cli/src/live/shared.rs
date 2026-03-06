use crate::default_servers::default_upstream_servers;
use byteblaster_core::qbt_receiver::parse_qbt_server;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn parse_servers_or_default(
    raw_servers: &[String],
) -> crate::error::CliResult<Vec<(String, u16)>> {
    if raw_servers.is_empty() {
        return Ok(default_upstream_servers());
    }

    raw_servers
        .iter()
        .map(|entry| {
            parse_qbt_server(entry).ok_or_else(|| {
                crate::error::CliError::invalid_argument(format!(
                    "invalid --server entry: {entry} (expected host:port)"
                ))
            })
        })
        .collect()
}
