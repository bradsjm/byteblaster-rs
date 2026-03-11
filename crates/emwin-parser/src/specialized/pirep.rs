//! Minimal PIREP bulletin parsing.
//!
//! The report header remains regex-driven because that is the narrowest and
//! clearest fixed-format part of the grammar. The rest of the parser now keeps
//! report normalization and field extraction explicit so it no longer rebuilds
//! the same string repeatedly.

use regex::Regex;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Type of PIREP report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PirepKind {
    /// Routine pilot report
    Ua,
    /// Urgent pilot report
    Uua,
}

/// Individual PIREP report from a pilot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PirepReport {
    /// Type of report (UA or UUA)
    #[serde(rename = "kind")]
    pub report_kind: PirepKind,
    /// Reporting station/airport (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub station: Option<String>,
    /// Report time in HHMM format (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// Raw location text from /OV field (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_raw: Option<String>,
    /// Flight level in feet (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flight_level_ft: Option<u32>,
    /// Aircraft type from /TP field (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aircraft_type: Option<String>,
    /// All parsed fields by their 2-letter codes
    pub fields: BTreeMap<String, String>,
    /// Complete raw PIREP text
    pub raw: String,
}

/// PIREP bulletin containing multiple pilot reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PirepBulletin {
    /// Individual PIREP reports in the bulletin
    pub reports: Vec<PirepReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedPirepRef<'a> {
    report_kind: PirepKind,
    station: Option<&'a str>,
    time: Option<String>,
    location_raw: Option<String>,
    flight_level_ft: Option<u32>,
    aircraft_type: Option<String>,
    fields: BTreeMap<String, String>,
}

impl ParsedPirepRef<'_> {
    fn into_owned(self, raw: String) -> PirepReport {
        PirepReport {
            report_kind: self.report_kind,
            station: self.station.map(str::to_string),
            time: self.time,
            location_raw: self.location_raw,
            flight_level_ft: self.flight_level_ft,
            aircraft_type: self.aircraft_type,
            fields: self.fields,
            raw,
        }
    }
}

/// Parses a PIREP bulletin from text content.
pub(crate) fn parse_pirep_bulletin(text: &str) -> Option<PirepBulletin> {
    let mut reports = Vec::new();

    for report in pirep_reports(text) {
        let Some(parsed) = parse_pirep_report_ref(&report) else {
            continue;
        };
        reports.push(parsed.into_owned(report.clone()));
    }

    (!reports.is_empty()).then_some(PirepBulletin { reports })
}

/// Splits normalized text into individual `=`-delimited PIREP reports.
fn pirep_reports(text: &str) -> Vec<String> {
    let normalized = compact_ascii_whitespace(text);
    normalized
        .split('=')
        .map(str::trim)
        .filter(|report| !report.is_empty())
        .map(str::to_string)
        .collect()
}

fn compact_ascii_whitespace(text: &str) -> String {
    let mut compacted = String::with_capacity(text.len());
    let mut pending_space = false;

    for ch in text.chars() {
        if ch.is_ascii_control() && !ch.is_ascii_whitespace() {
            continue;
        }
        if ch.is_ascii_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !compacted.is_empty() {
            compacted.push(' ');
        }
        pending_space = false;
        compacted.push(ch);
    }

    compacted
}

/// Parses an individual normalized PIREP report.
fn parse_pirep_report_ref(report: &str) -> Option<ParsedPirepRef<'_>> {
    let (header, body) = report.split_once('/')?;
    let captures = report_header_re().captures(header.trim())?;
    let report_kind = match captures.name("kind")?.as_str() {
        "UA" => PirepKind::Ua,
        "UUA" => PirepKind::Uua,
        _ => return None,
    };
    let station = captures.name("station").map(|value| value.as_str());
    let fields = parse_pirep_fields(body);

    let time = fields.get("TM").map(|value| normalize_time(value));
    let location_raw = fields.get("OV").cloned();
    let flight_level_ft = fields.get("FL").and_then(|value| parse_flight_level(value));
    let aircraft_type = fields.get("TP").cloned();

    Some(ParsedPirepRef {
        report_kind,
        station,
        time,
        location_raw,
        flight_level_ft,
        aircraft_type,
        fields,
    })
}

/// Parses slash-delimited PIREP fields into an owned field map.
fn parse_pirep_fields(report: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();

    for part in report.split('/') {
        let trimmed = part.trim();
        if trimmed.len() < 3 {
            continue;
        }
        let key = &trimmed[..2];
        if !key.chars().all(|ch| ch.is_ascii_uppercase()) {
            continue;
        }
        let value = trimmed[2..].trim();
        if value.is_empty() {
            continue;
        }
        fields.insert(key.to_string(), value.to_string());
    }

    fields
}

fn normalize_time(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .take(4)
        .collect()
}

fn parse_flight_level(value: &str) -> Option<u32> {
    let digits = value.strip_prefix("FL").unwrap_or(value);
    let digits = digits
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    (digits.len() == 3)
        .then(|| digits.parse::<u32>().ok())
        .flatten()
        .map(|level| level.saturating_mul(100))
}

fn report_header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?P<station>[A-Z0-9]{3,4})\s+(?P<kind>UA|UUA)\b")
            .expect("pirep header regex compiles")
    })
}

#[cfg(test)]
mod tests {
    use super::{PirepKind, parse_pirep_bulletin};

    #[test]
    fn parses_multiple_pireps() {
        let text = "DEN UA /OV 35 SW=\nKGTF UUA /OV GTF209006/TM 1925/FL050/TP E145=\n";
        let bulletin = parse_pirep_bulletin(text).expect("pirep bulletin should parse");

        assert_eq!(bulletin.reports.len(), 2);
        assert_eq!(bulletin.reports[0].report_kind, PirepKind::Ua);
        assert_eq!(bulletin.reports[1].report_kind, PirepKind::Uua);
        assert_eq!(bulletin.reports[1].time.as_deref(), Some("1925"));
        assert_eq!(bulletin.reports[1].flight_level_ft, Some(5_000));
        assert_eq!(bulletin.reports[1].aircraft_type.as_deref(), Some("E145"));
    }

    #[test]
    fn parses_single_pirep() {
        let text = "DEN UA /OV 35 SW /TM 1925 /FL050 /TP E145=\n";
        let bulletin = parse_pirep_bulletin(text).expect("single pirep should parse");

        assert_eq!(bulletin.reports.len(), 1);
        assert_eq!(bulletin.reports[0].station.as_deref(), Some("DEN"));
        assert_eq!(bulletin.reports[0].location_raw.as_deref(), Some("35 SW"));
    }

    #[test]
    fn wrapped_report_lines_are_normalized() {
        let text = "DEN UA /OV 35 SW\n/TM 1925 /FL050\n/TP E145=\n";
        let bulletin = parse_pirep_bulletin(text).expect("wrapped pirep should parse");

        assert_eq!(bulletin.reports[0].time.as_deref(), Some("1925"));
        assert_eq!(bulletin.reports[0].aircraft_type.as_deref(), Some("E145"));
    }

    #[test]
    fn invalid_header_is_rejected() {
        assert!(parse_pirep_bulletin("UA /OV 35 SW=").is_none());
    }

    #[test]
    fn rejects_non_pirep_text() {
        assert!(parse_pirep_bulletin("AREA FORECAST DISCUSSION").is_none());
    }
}
