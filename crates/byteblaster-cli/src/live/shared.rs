use byteblaster_core::parse_server;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_SERVERS: [(&str, u16); 4] = [
    ("emwin.weathermessage.com", 2211),
    ("master.weathermessage.com", 2211),
    ("emwin.interweather.net", 1000),
    ("wxmesg.upstateweather.com", 2211),
];

pub(crate) fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn write_completed_file(
    output_dir: &Path,
    filename: &str,
    data: &[u8],
) -> anyhow::Result<String> {
    let target = output_dir.join(filename);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, data)?;
    Ok(target.to_string_lossy().to_string())
}

pub(crate) fn parse_servers_or_default(
    raw_servers: &[String],
) -> anyhow::Result<Vec<(String, u16)>> {
    if raw_servers.is_empty() {
        return Ok(DEFAULT_SERVERS
            .iter()
            .map(|(host, port)| ((*host).to_string(), *port))
            .collect());
    }

    raw_servers
        .iter()
        .map(|entry| {
            parse_server(entry).ok_or_else(|| {
                anyhow::anyhow!("invalid --server entry: {entry} (expected host:port)")
            })
        })
        .collect()
}
