//! Minimal TAF bulletin parsing for WMO bulletins without AFOS PIL lines.

use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TafBulletin {
    pub station: String,
    pub issue_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    pub amendment: bool,
    pub correction: bool,
    pub raw: String,
}

pub(crate) fn parse_taf_bulletin(text: &str) -> Option<TafBulletin> {
    let raw = taf_body(text)?;
    let captures = taf_re().captures(&raw)?;

    Some(TafBulletin {
        station: captures.name("station")?.as_str().to_string(),
        issue_time: captures.name("issue_time")?.as_str().to_string(),
        valid_from: captures
            .name("valid_from")
            .map(|value| value.as_str().to_string()),
        valid_to: captures
            .name("valid_to")
            .map(|value| value.as_str().to_string()),
        amendment: raw.starts_with("TAF AMD"),
        correction: raw.starts_with("TAF COR"),
        raw,
    })
}

fn taf_body(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 2 {
        return None;
    }

    let first_body = lines
        .iter()
        .enumerate()
        .skip(2)
        .find_map(|(index, line)| (!line.trim().is_empty()).then_some(index))?;

    let raw = lines[first_body..]
        .iter()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join(" ");
    let normalized = normalize_taf_prefix(&raw);

    normalized.starts_with("TAF").then_some(normalized)
}

fn normalize_taf_prefix(raw: &str) -> String {
    let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(rest) = normalized.strip_prefix("TAF TAF ") {
        format!("TAF {rest}")
    } else {
        normalized
    }
}

fn taf_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^TAF(?:\s+(?P<qualifier>AMD|COR))?\s+(?P<station>[A-Z0-9]{3,4})\s+(?P<issue_time>\d{6}Z)\s+(?:(?P<valid_from>\d{4})/(?P<valid_to>\d{4})\s+)?",
        )
        .expect("taf regex compiles")
    })
}

#[cfg(test)]
mod tests {
    use super::parse_taf_bulletin;

    #[test]
    fn parses_amended_taf_bulletin() {
        let text =
            "000 \nFTXX01 KWBC 070200\nTAF AMD\nWBCF 070244Z 0703/0803 18012KT P6SM SCT050\n";
        let taf = parse_taf_bulletin(text).expect("expected TAF bulletin parsing to succeed");

        assert_eq!(taf.station, "WBCF");
        assert_eq!(taf.issue_time, "070244Z");
        assert_eq!(taf.valid_from.as_deref(), Some("0703"));
        assert_eq!(taf.valid_to.as_deref(), Some("0803"));
        assert!(taf.amendment);
        assert!(!taf.correction);
    }

    #[test]
    fn parses_bulletin_with_marker_line_before_taf_report() {
        let text = "000 \nFTVN41 KWBC 070303\nTAF\nTAF SVJC 070400Z 0706/0806 07005KT 9999 FEW013 TX33/0718Z\n      TN23/0708Z\n      TEMPO 0706/0710 08004KT CAVOK\n     FM071100 09006KT 9999 FEW013=\n";
        let taf = parse_taf_bulletin(text).expect("expected TAF bulletin parsing to succeed");

        assert_eq!(taf.station, "SVJC");
        assert_eq!(taf.issue_time, "070400Z");
        assert_eq!(taf.valid_from.as_deref(), Some("0706"));
        assert_eq!(taf.valid_to.as_deref(), Some("0806"));
        assert!(!taf.amendment);
        assert!(!taf.correction);
        assert!(taf.raw.starts_with("TAF SVJC 070400Z"));
    }

    #[test]
    fn ignores_non_taf_body() {
        let text = "000 \nSAGL31 BGGH 070200\nMETAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n";
        assert!(parse_taf_bulletin(text).is_none());
    }
}
