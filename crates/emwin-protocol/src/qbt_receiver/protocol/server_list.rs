//! Server list parsing for EMWIN protocol.
//!
//! This module handles parsing of server list frames received from
//! the EMWIN servers, including both primary and satellite servers.

use crate::qbt_receiver::error::QbtProtocolError;
use crate::qbt_receiver::protocol::model::{QbtProtocolWarning, QbtServerList};

/// Parses a single server endpoint from a string.
///
/// Expected format: `host:port`
///
/// # Arguments
///
/// * `input` - The server string to parse
///
/// # Returns
///
/// `Some((host, port))` if parsing succeeds, `None` otherwise
///
/// # Example
///
/// ```
/// use emwin_protocol::qbt_receiver::parse_qbt_server;
///
/// let result = parse_qbt_server("example.com:2211");
/// assert_eq!(result, Some(("example.com".to_string(), 2211)));
/// ```
pub fn parse_qbt_server(input: &str) -> Option<(String, u16)> {
    let (host, port) = input.rsplit_once(':')?;
    let parsed_port = port.parse::<u16>().ok()?;
    Some((host.to_string(), parsed_port))
}

/// Parses a complete server list frame.
///
/// Server list frames have the format:
/// `/ServerList/host1:port1|host2:port2\ServerList\/SatServers/sat1:port1+sat2:port2\SatServers\`
///
/// # Arguments
///
/// * `content` - The frame content to parse
///
/// # Returns
///
/// A tuple of (QbtServerList, warnings) on success
///
/// # Errors
///
/// Returns `QbtProtocolError::UnsupportedFrame` if the content doesn't start with `/ServerList/`
pub fn parse_server_list_frame(
    content: &str,
) -> Result<(QbtServerList, Vec<QbtProtocolWarning>), QbtProtocolError> {
    if !content.starts_with("/ServerList/") {
        return Err(QbtProtocolError::UnsupportedFrame);
    }

    let payload = content.trim_end_matches('\0');
    let mut warnings = Vec::new();
    let mut out = QbtServerList::default();

    let full_marker = "\\QbtServerList\\/SatServers/";
    let sat_end_marker = "\\SatServers\\";

    let servers_part;
    let sat_part;

    if let Some(start_idx) = payload.find(full_marker) {
        let sat_start = start_idx + full_marker.len();
        if let Some(end_rel) = payload[sat_start..].find(sat_end_marker) {
            servers_part = &payload["/ServerList/".len()..start_idx];
            sat_part = Some(&payload[sat_start..sat_start + end_rel]);
        } else {
            servers_part = &payload["/ServerList/".len()..];
            sat_part = None;
        }
    } else {
        servers_part = &payload["/ServerList/".len()..];
        sat_part = None;
    }

    out.servers = parse_list_entries(servers_part, '|', &mut warnings);

    if let Some(sat_payload) = sat_part {
        out.sat_servers = parse_list_entries(sat_payload, '+', &mut warnings);
    }

    Ok((out, warnings))
}

/// Parses a list of server entries separated by a delimiter.
fn parse_list_entries(
    input: &str,
    delimiter: char,
    warnings: &mut Vec<QbtProtocolWarning>,
) -> Vec<(String, u16)> {
    input
        .split(delimiter)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| match parse_qbt_server(entry) {
            Some(parsed) => Some(parsed),
            None => {
                warnings.push(QbtProtocolWarning::MalformedServerEntry {
                    entry: entry.to_string(),
                });
                None
            }
        })
        .collect()
}

/// Parses a server list frame, returning an empty list on failure.
///
/// This is a convenience function that discards warnings and returns
/// an empty QbtServerList on parse failure.
pub fn parse_simple_server_list(content: &str) -> QbtServerList {
    parse_server_list_frame(content)
        .map(|(list, _warnings)| list)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_list_simple_parse() {
        let content = "/ServerList/a.example:2211|bad|b.example:1000\0";
        let (list, warnings) = parse_server_list_frame(content).expect("simple list should parse");
        assert_eq!(list.servers.len(), 2);
        assert!(list.sat_servers.is_empty());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn server_list_full_parse() {
        let content = "/ServerList/a.example:2211|b.example:1000\\QbtServerList\\/SatServers/s1:3000+s2:3001\\SatServers\\\0";
        let (list, warnings) = parse_server_list_frame(content).expect("full list should parse");
        assert_eq!(list.servers.len(), 2);
        assert_eq!(list.sat_servers.len(), 2);
        assert!(warnings.is_empty());
    }
}
