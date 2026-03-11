//! Parsing for Local Storm Report bulletins.

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

use crate::ProductParseIssue;

/// Structured LSR bulletin.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LsrBulletin {
    pub reports: Vec<LsrReport>,
    pub is_summary: bool,
}

/// One LSR report row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LsrReport {
    pub valid: String,
    pub event_text: String,
    pub city: String,
    pub county: Option<String>,
    pub state: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub source: Option<String>,
    pub remark: Option<String>,
    pub magnitude_value: Option<f64>,
    pub magnitude_units: Option<String>,
    pub magnitude_qualifier: Option<String>,
}

pub(crate) fn parse_lsr_bulletin(
    text: &str,
    reference_time: DateTime<Utc>,
) -> Option<(LsrBulletin, Vec<ProductParseIssue>)> {
    let normalized = text.replace('\r', "");
    let mut reports = Vec::new();
    let mut issues = Vec::new();
    let chunks = split_lsr_chunks(&normalized);
    for chunk in &chunks {
        match parse_lsr_chunk(chunk, reference_time) {
            Some(report) => reports.push(report),
            None => issues.push(ProductParseIssue::new(
                "lsr_parse",
                "invalid_lsr_report",
                "could not parse LSR report block",
                Some(chunk.trim().to_string()),
            )),
        }
    }

    (!reports.is_empty()).then_some((
        LsrBulletin {
            reports,
            is_summary: normalized.to_ascii_uppercase().contains("...SUMMARY"),
        },
        issues,
    ))
}

fn split_lsr_chunks(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut start = None;
    let mut chunks = Vec::new();
    for idx in 0..lines.len() {
        if is_time_line(lines[idx])
            && let Some(existing) = start.replace(idx)
        {
            chunks.push(lines[existing..idx].join("\n"));
        }
    }
    if let Some(existing) = start {
        chunks.push(lines[existing..].join("\n"));
    }
    chunks
}

fn is_time_line(line: &str) -> bool {
    let trimmed = line.trim_end();
    trimmed.len() >= 29
        && trimmed[..4].chars().all(|c| c.is_ascii_digit())
        && trimmed[4..].starts_with(" ")
}

fn parse_lsr_chunk(text: &str, reference_time: DateTime<Utc>) -> Option<LsrReport> {
    let mut lines = text.lines();
    let first = lines.next()?.trim_end();
    let second = lines.next()?.trim_end();
    let remarks = lines
        .take_while(|line| !line.trim().starts_with("&&") && !line.trim().starts_with("$$"))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let first_caps = first_line_re().captures(first)?;
    let second_caps = second_line_re().captures(second)?;
    let time_token = first_caps.name("hhmm")?.as_str();
    let ampm = first_caps.name("ampm")?.as_str();
    let event_text = first_caps.name("event")?.as_str().trim().to_string();
    let city = first_caps.name("city")?.as_str().trim().to_string();
    let lat_token = first_caps.name("lat")?.as_str();
    let lon_token = first_caps.name("lon")?.as_str();
    let valid = parse_lsr_datetime(time_token, ampm, second.get(..10)?.trim(), reference_time)?;
    let (latitude, longitude) = parse_lat_lon(lat_token, lon_token)?;
    let magnitude = second_caps
        .name("mag")
        .map(|m| m.as_str())
        .unwrap_or("")
        .trim();
    let county = second_caps
        .name("county")
        .map(|m| m.as_str().trim())
        .and_then(empty_to_none);
    let state = second_caps
        .name("state")
        .map(|m| m.as_str().trim())
        .and_then(empty_to_none);
    let source = second_caps
        .name("source")
        .map(|m| m.as_str().trim())
        .and_then(empty_to_none);
    let (magnitude_value, magnitude_units, magnitude_qualifier) = parse_magnitude(magnitude);

    Some(LsrReport {
        valid: valid.to_rfc3339(),
        event_text,
        city,
        county,
        state,
        latitude,
        longitude,
        source,
        remark: empty_to_none(remarks.trim()),
        magnitude_value,
        magnitude_units,
        magnitude_qualifier,
    })
}

fn first_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?P<hhmm>\d{4})\s+(?P<ampm>AM|PM)\s+(?P<event>.{1,17}?)\s{2,}(?P<city>.{1,24}?)\s{2,}(?P<lat>\d+\.\d+[NS])\s+(?P<lon>\d+\.\d+[EW])$",
        )
        .expect("valid LSR first line regex")
    })
}

fn second_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?P<date>\d{2}/\d{2}/\d{4})\s+(?P<mag>.{0,17}?)\s{2,}(?P<county>.{1,19}?)\s{2,}(?P<state>[A-Z]{2})\s{2,}(?P<source>.+?)\s*$",
        )
        .expect("valid LSR second line regex")
    })
}

fn parse_lsr_datetime(
    hhmm: &str,
    ampm: &str,
    date: &str,
    _reference_time: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let naive =
        NaiveDateTime::parse_from_str(&format!("{date} {hhmm} {ampm}"), "%m/%d/%Y %I%M %p").ok()?;
    Some(Utc.from_utc_datetime(&naive))
}

fn parse_lat_lon(lat: &str, lon: &str) -> Option<(f64, f64)> {
    let latitude = lat.get(..lat.len().checked_sub(1)?)?.parse::<f64>().ok()?;
    let lat = if lat.ends_with('S') {
        -latitude
    } else {
        latitude
    };
    let longitude = lon.get(..lon.len().checked_sub(1)?)?.parse::<f64>().ok()?;
    let lon = if lon.ends_with('E') {
        longitude
    } else {
        -longitude
    };
    Some((lat, lon))
}

fn magnitude_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?:(?P<qual>[EMULTG><=]+)\s+)?(?P<value>-?\d+(?:\.\d+)?)\s*(?P<units>[A-Z/]+)?$",
        )
        .expect("valid LSR magnitude regex")
    })
}

fn parse_magnitude(raw: &str) -> (Option<f64>, Option<String>, Option<String>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (None, None, None);
    }
    let Some(caps) = magnitude_re().captures(trimmed) else {
        return (None, Some(trimmed.to_string()), None);
    };
    let value = caps
        .name("value")
        .and_then(|m| m.as_str().parse::<f64>().ok());
    let units = caps.name("units").map(|m| m.as_str().to_string());
    let qualifier = caps.name("qual").map(|m| m.as_str().trim().to_string());
    (value, units, qualifier)
}

fn empty_to_none(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_lsr_bulletin;
    use chrono::Utc;

    #[test]
    fn parses_local_lsr_report() {
        let text = "\
0150 AM     HAIL             BROOKSVILLE             34.40N 87.70W
03/10/2026  1.00 IN          WINSTON             AL  PUBLIC
QUARTER SIZE HAIL REPORTED
&&";
        let (bulletin, issues) = parse_lsr_bulletin(text, Utc::now()).expect("lsr bulletin");
        assert_eq!(bulletin.reports.len(), 1);
        assert!(issues.is_empty());
        assert_eq!(bulletin.reports[0].city, "BROOKSVILLE");
        assert_eq!(bulletin.reports[0].state.as_deref(), Some("AL"));
    }

    #[test]
    fn malformed_lsr_block_reports_issue_but_keeps_valid_report() {
        let text = "\
0150 AM     HAIL             BROOKSVILLE             34.40N 87.70W
03/10/2026  1.00 IN          WINSTON             AL  PUBLIC
0145 AM     HAIL             NOWHERE                 34.00N 87.00W
03/10/2026
&&";
        let (bulletin, issues) = parse_lsr_bulletin(text, Utc::now()).expect("lsr bulletin");
        assert_eq!(bulletin.reports.len(), 1);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "invalid_lsr_report");
    }
}
