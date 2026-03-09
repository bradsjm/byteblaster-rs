//! Minimal TAF bulletin parsing for WMO bulletins without AFOS PIL lines.
//!
//! TAF (Terminal Aerodrome Forecast) bulletins contain weather forecasts for
//! specific airports. This module parses TAF reports when the standard AFOS PIL
//! is not available, extracting station, validity times, and amendment status.

use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

/// TAF bulletin containing a terminal aerodrome forecast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TafBulletin {
    /// ICAO station identifier (e.g., "KBOS")
    pub station: String,
    /// Issue time in HHMMSSZ format
    pub issue_time: String,
    /// Validity period start (DDHH format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    /// Validity period end (DDHH format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    /// True if this is an amended forecast (TAF AMD)
    pub amendment: bool,
    /// True if this is a corrected forecast (TAF COR)
    pub correction: bool,
    /// Complete raw TAF text
    pub raw: String,
}

/// Parses a TAF bulletin from text content.
///
/// Extracts the station identifier, issue time, validity period, and
/// amendment/correction status from TAF text.
///
/// # Arguments
///
/// * `text` - Raw TAF bulletin text
///
/// # Returns
///
/// `Some(TafBulletin)` if a valid TAF was parsed, `None` otherwise
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

/// Extracts and normalizes the TAF body from text.
///
/// Joins lines with spaces and removes duplicate "TAF TAF" prefixes.
fn taf_body(text: &str) -> Option<String> {
    let raw = text
        .lines()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join(" ");
    let normalized = normalize_taf_prefix(&raw);

    normalized.starts_with("TAF").then_some(normalized)
}

/// Removes duplicate TAF prefix if present.
///
/// Some bulletins have "TAF TAF" which should be normalized to "TAF".
fn normalize_taf_prefix(raw: &str) -> String {
    let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    if let Some((prefix_len, prefix)) = duplicate_taf_prefix(&tokens) {
        let remainder = tokens[prefix_len..].join(" ");
        if remainder.is_empty() {
            prefix.to_string()
        } else {
            format!("{prefix} {remainder}")
        }
    } else {
        normalized
    }
}

fn duplicate_taf_prefix<'a>(tokens: &[&'a str]) -> Option<(usize, &'a str)> {
    if tokens.len() >= 2 && tokens[0] == "TAF" && tokens[1] == "TAF" {
        return Some((2, "TAF"));
    }
    if tokens.len() >= 4 && tokens[0] == "TAF" && tokens[2] == "TAF" && tokens[1] == tokens[3] {
        return match tokens[1] {
            "AMD" => Some((4, "TAF AMD")),
            "COR" => Some((4, "TAF COR")),
            _ => None,
        };
    }
    if tokens.len() >= 3 && tokens[0] == "TAF" && tokens[1] == "AMD" && tokens[2] == "TAF" {
        return Some((3, "TAF AMD"));
    }
    if tokens.len() >= 3 && tokens[0] == "TAF" && tokens[1] == "COR" && tokens[2] == "TAF" {
        return Some((3, "TAF COR"));
    }
    None
}

/// Returns the compiled TAF parsing regex.
///
/// Pattern: TAF [AMD|COR] <station> <issue_time> [<valid_from>/<valid_to>]
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
        let text = "TAF AMD\nWBCF 070244Z 0703/0803 18012KT P6SM SCT050\n";
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
        let text = "TAF\nTAF SVJC 070400Z 0706/0806 07005KT 9999 FEW013 TX33/0718Z\n      TN23/0708Z\n      TEMPO 0706/0710 08004KT CAVOK\n     FM071100 09006KT 9999 FEW013=\n";
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

    #[test]
    fn parses_duplicated_amended_taf_prefix() {
        let text = "TAF AMD\nTAF AMD MMAS 090101Z 0901/0918 23008KT P6SM SCT100 BKN200\n";
        let taf = parse_taf_bulletin(text).expect("expected duplicated TAF AMD parsing to succeed");

        assert_eq!(taf.station, "MMAS");
        assert_eq!(taf.issue_time, "090101Z");
        assert_eq!(taf.valid_from.as_deref(), Some("0901"));
        assert_eq!(taf.valid_to.as_deref(), Some("0918"));
        assert!(taf.amendment);
        assert!(!taf.correction);
    }

    #[test]
    fn parses_duplicated_corrected_taf_prefix() {
        let text = "TAF COR\nTAF COR KBOS 090520Z 0906/1012 28012KT P6SM FEW250\n";
        let taf = parse_taf_bulletin(text).expect("expected duplicated TAF COR parsing to succeed");

        assert_eq!(taf.station, "KBOS");
        assert_eq!(taf.issue_time, "090520Z");
        assert!(taf.correction);
        assert!(!taf.amendment);
    }
}
