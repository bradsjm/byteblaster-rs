pub(crate) const DEFAULT_UPSTREAM_SERVERS: [(&str, u16); 4] = [
    ("emwin.weathermessage.com", 2211),
    ("master.weathermessage.com", 2211),
    ("emwin.interweather.net", 1000),
    ("wxmesg.upstateweather.com", 2211),
];

pub(crate) fn default_upstream_servers() -> Vec<(String, u16)> {
    DEFAULT_UPSTREAM_SERVERS
        .iter()
        .map(|(host, port)| ((*host).to_string(), *port))
        .collect()
}
