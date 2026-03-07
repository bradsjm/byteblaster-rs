//! Minimal METAR bulletin parsing for WMO collectives without AFOS PIL lines.

use crate::ProductParseIssue;
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetarReportKind {
    Metar,
    Speci,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetarReport {
    pub kind: MetarReportKind,
    pub station: String,
    pub observation_time: String,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetarBulletin {
    pub reports: Vec<MetarReport>,
}

impl MetarBulletin {
    pub fn report_count(&self) -> usize {
        self.reports.len()
    }
}

pub(crate) fn parse_metar_bulletin(text: &str) -> Option<(MetarBulletin, Vec<ProductParseIssue>)> {
    let content = metar_body(text);
    let mut reports = Vec::new();
    let mut issues = Vec::new();

    for token in content.split('=') {
        let token = normalize_token(token);
        if token.is_empty() {
            continue;
        }

        match parse_metar_report(&token) {
            Some(report) => reports.push(report),
            None if token.contains("METAR") || token.contains("SPECI") => {
                issues.push(ProductParseIssue::new(
                    "metar_parse",
                    "invalid_metar_report",
                    "could not parse METAR/SPECI report from bulletin token",
                    Some(token),
                ));
            }
            None => {}
        }
    }

    (!reports.is_empty()).then_some((MetarBulletin { reports }, issues))
}

fn metar_body(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 2 {
        return String::new();
    }

    let first_body = lines
        .iter()
        .enumerate()
        .skip(2)
        .find_map(|(index, line)| (!line.trim().is_empty()).then_some(index))
        .unwrap_or(lines.len());

    lines[first_body..].join("\n")
}

fn normalize_token(token: &str) -> String {
    token.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_metar_report(token: &str) -> Option<MetarReport> {
    let captures = metar_re().captures(token)?;
    let kind = match captures.name("kind")?.as_str() {
        "METAR" => MetarReportKind::Metar,
        "SPECI" => MetarReportKind::Speci,
        _ => return None,
    };

    Some(MetarReport {
        kind,
        station: captures.name("station")?.as_str().to_string(),
        observation_time: captures.name("time")?.as_str().to_string(),
        raw: token.to_string(),
    })
}

fn metar_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?P<kind>METAR|SPECI)\s+(?P<station>[A-Z][A-Z0-9]{3})\s+(?P<time>\d{6}Z)\b",
        )
        .expect("metar regex compiles")
    })
}

#[cfg(test)]
mod tests {
    use super::{MetarReportKind, parse_metar_bulletin};

    #[test]
    fn parses_collective_with_single_metar() {
        let text = "000 \nSAGL31 BGGH 070200\nMETAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n";
        let (bulletin, issues) =
            parse_metar_bulletin(text).expect("expected METAR bulletin parsing to succeed");

        assert!(issues.is_empty());
        assert_eq!(bulletin.report_count(), 1);
        assert_eq!(bulletin.reports[0].kind, MetarReportKind::Metar);
        assert_eq!(bulletin.reports[0].station, "BGKK");
        assert_eq!(bulletin.reports[0].observation_time, "070220Z");
    }

    #[test]
    fn ignores_non_metar_body() {
        let text = "000 \nFXUS61 KBOX 022101\nAREA FORECAST DISCUSSION\n";
        assert!(parse_metar_bulletin(text).is_none());
    }
}
