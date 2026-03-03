use crate::error::ProtocolError;
use crate::protocol::model::{ProtocolWarning, ServerList};

pub fn parse_server(input: &str) -> Option<(String, u16)> {
    let (host, port) = input.rsplit_once(':')?;
    let parsed_port = port.parse::<u16>().ok()?;
    Some((host.to_string(), parsed_port))
}

pub fn parse_server_list_frame(
    content: &str,
) -> Result<(ServerList, Vec<ProtocolWarning>), ProtocolError> {
    if !content.starts_with("/ServerList/") {
        return Err(ProtocolError::UnsupportedFrame);
    }

    let payload = content.trim_end_matches('\0');
    let mut warnings = Vec::new();
    let mut out = ServerList::default();

    let full_marker = "\\ServerList\\/SatServers/";
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

fn parse_list_entries(
    input: &str,
    delimiter: char,
    warnings: &mut Vec<ProtocolWarning>,
) -> Vec<(String, u16)> {
    input
        .split(delimiter)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| match parse_server(entry) {
            Some(parsed) => Some(parsed),
            None => {
                warnings.push(ProtocolWarning::MalformedServerEntry {
                    entry: entry.to_string(),
                });
                None
            }
        })
        .collect()
}

pub fn parse_simple_server_list(content: &str) -> ServerList {
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
        let content = "/ServerList/a.example:2211|b.example:1000\\ServerList\\/SatServers/s1:3000+s2:3001\\SatServers\\\0";
        let (list, warnings) = parse_server_list_frame(content).expect("full list should parse");
        assert_eq!(list.servers.len(), 2);
        assert_eq!(list.sat_servers.len(), 2);
        assert!(warnings.is_empty());
    }
}
