//! Minimal PIREP bulletin parsing.

use regex::Regex;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PirepKind {
    Ua,
    Uua,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PirepReport {
    #[serde(rename = "kind")]
    pub report_kind: PirepKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub station: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flight_level_ft: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aircraft_type: Option<String>,
    pub fields: BTreeMap<String, String>,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PirepBulletin {
    pub reports: Vec<PirepReport>,
}

pub(crate) fn parse_pirep_bulletin(text: &str) -> Option<PirepBulletin> {
    let normalized = normalized_body(text);
    let mut reports = Vec::new();

    for token in normalized.split('=') {
        let token = normalize_spaces(token);
        if token.is_empty() {
            continue;
        }
        let Some(report) = parse_report(&token) else {
            continue;
        };
        reports.push(report);
    }

    (!reports.is_empty()).then_some(PirepBulletin { reports })
}

fn parse_report(report: &str) -> Option<PirepReport> {
    let mut parts = report.split('/');
    let header = normalize_spaces(parts.next()?);
    let captures = report_header_re().captures(&header)?;
    let report_kind = match captures.name("kind")?.as_str() {
        "UA" => PirepKind::Ua,
        "UUA" => PirepKind::Uua,
        _ => return None,
    };

    let station = captures
        .name("station")
        .map(|value| value.as_str().to_string());
    let mut fields = BTreeMap::new();

    for part in parts {
        let part = normalize_spaces(part);
        if part.len() < 2 {
            continue;
        }
        let key = part[0..2].to_string();
        let value = part[2..].trim().to_string();
        if value.is_empty() {
            continue;
        }
        fields.insert(key, value);
    }

    let time = fields.get("TM").map(|value| normalize_time(value));
    let location_raw = fields.get("OV").cloned();
    let flight_level_ft = fields.get("FL").and_then(|value| parse_flight_level(value));
    let aircraft_type = fields.get("TP").cloned();

    Some(PirepReport {
        report_kind,
        station,
        time,
        location_raw,
        flight_level_ft,
        aircraft_type,
        fields,
        raw: report.to_string(),
    })
}

fn normalized_body(text: &str) -> String {
    text.lines()
        .map(strip_control_chars)
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_control_chars(line: &str) -> String {
    line.chars()
        .filter(|ch| !ch.is_ascii_control() || ch.is_ascii_whitespace())
        .collect()
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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
    let captures = flight_level_re().captures(digits)?;
    let level = captures.name("level")?.as_str().parse::<u32>().ok()?;
    Some(level.saturating_mul(100))
}

fn report_header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?P<station>[A-Z0-9]{3,4})\s+(?P<kind>UA|UUA)\b")
            .expect("pirep header regex compiles")
    })
}

fn flight_level_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?P<level>\d{3})\b").expect("pirep flight-level regex compiles")
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
    fn rejects_non_pirep_text() {
        assert!(parse_pirep_bulletin("AREA FORECAST DISCUSSION").is_none());
    }
}
