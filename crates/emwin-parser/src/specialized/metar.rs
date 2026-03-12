//! Minimal METAR bulletin parsing for WMO collectives without AFOS PIL lines.
//!
//! The parser keeps the existing owned `MetarBulletin` output but replaces the
//! regex-based core parse with explicit token handling. This removes repeated
//! whitespace join/split churn from the collective path while preserving the
//! same issue behavior for invalid report-like segments.

use crate::ProductParseIssue;
use serde::Serialize;

/// Type of METAR report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetarReportKind {
    /// Routine METAR observation
    Metar,
    /// Special (non-routine) observation
    Speci,
}

/// Individual METAR report from a single station.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetarReport {
    /// Type of report (METAR or SPECI)
    pub kind: MetarReportKind,
    /// ICAO station identifier (e.g., "KBOS")
    pub station: String,
    /// Observation time in HHMMSSZ format
    pub observation_time: String,
    /// Complete raw METAR text
    pub raw: String,
}

/// METAR bulletin containing multiple station reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetarBulletin {
    /// Individual METAR reports in the bulletin
    pub reports: Vec<MetarReport>,
}

impl MetarBulletin {
    /// Returns the number of reports in the bulletin.
    pub fn report_count(&self) -> usize {
        self.reports.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedMetarRef {
    kind: MetarReportKind,
    station: String,
    observation_time: String,
}

impl ParsedMetarRef {
    fn into_owned(self, raw: String) -> MetarReport {
        MetarReport {
            kind: self.kind,
            station: self.station,
            observation_time: self.observation_time,
            raw,
        }
    }
}

/// Parses a METAR bulletin from text content.
pub(crate) fn parse_metar_bulletin(text: &str) -> Option<(MetarBulletin, Vec<ProductParseIssue>)> {
    let content = normalize_metar_segment(text);
    let mut reports = Vec::new();
    let mut issues = Vec::new();

    for segment in content.split('=') {
        let normalized = normalize_metar_segment(segment);
        if normalized.is_empty() {
            continue;
        }

        match parse_metar_report_ref(&normalized) {
            Some(parsed) => reports.push(parsed.into_owned(normalized.clone())),
            None if normalized.contains("METAR") || normalized.contains("SPECI") => {
                issues.push(ProductParseIssue::new(
                    "metar_parse",
                    "invalid_metar_report",
                    "could not parse METAR/SPECI report from bulletin token",
                    Some(normalized),
                ));
            }
            None => {}
        }
    }

    (!reports.is_empty()).then_some((MetarBulletin { reports }, issues))
}

/// Normalizes whitespace in a segment by compacting ASCII separators in one pass.
fn normalize_metar_segment(segment: &str) -> String {
    let mut normalized = String::with_capacity(segment.len());
    let mut pending_space = false;

    for ch in segment.chars() {
        if ch.is_ascii_whitespace() {
            pending_space = true;
            continue;
        }

        if pending_space && !normalized.is_empty() {
            normalized.push(' ');
        }
        pending_space = false;
        normalized.push(ch);
    }

    normalized
}

/// Parses a normalized METAR/SPECI segment into owned header fields.
fn parse_metar_report_ref(segment: &str) -> Option<ParsedMetarRef> {
    let tokens = segment.split(' ').collect::<Vec<_>>();
    let (kind, start, inline_station) = find_metar_start(&tokens)?;
    let mut tokens = tokens[start..].iter().copied();
    let _kind_token = tokens.next()?;
    let maybe_station = inline_station.or_else(|| tokens.next())?;
    let station = if maybe_station == "COR" {
        tokens.next()?
    } else {
        maybe_station
    };
    let observation_time = tokens.next()?;

    if !is_metar_station(station) || !is_observation_time(observation_time) {
        return None;
    }

    Some(ParsedMetarRef {
        kind,
        station: station.to_string(),
        observation_time: observation_time.to_string(),
    })
}

fn find_metar_start<'a>(
    tokens: &'a [&'a str],
) -> Option<(MetarReportKind, usize, Option<&'a str>)> {
    for (index, token) in tokens.iter().copied().enumerate() {
        match token {
            "METAR" => return Some((MetarReportKind::Metar, index, None)),
            "SPECI" => return Some((MetarReportKind::Speci, index, None)),
            _ => {}
        }

        if let Some(station) = token.strip_prefix("METAR")
            && is_metar_station(station)
        {
            return Some((MetarReportKind::Metar, index, Some(station)));
        }

        if let Some(station) = token.strip_prefix("SPECI")
            && is_metar_station(station)
        {
            return Some((MetarReportKind::Speci, index, Some(station)));
        }
    }

    None
}

fn is_metar_station(token: &str) -> bool {
    token.len() == 4
        && token.starts_with(|ch: char| ch.is_ascii_uppercase())
        && token.chars().all(|ch| ch.is_ascii_alphanumeric())
}

fn is_observation_time(token: &str) -> bool {
    token.len() == 7 && token.ends_with('Z') && token[..6].chars().all(|ch| ch.is_ascii_digit())
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
    fn parses_multiple_reports_in_bulletin() {
        let text = "METAR BGKK 070220Z AUTO VRB02KT 9999= SPECI KDSM 070254Z 33007KT 10SM CLR=";
        let (bulletin, issues) =
            parse_metar_bulletin(text).expect("expected multiple METAR reports");

        assert!(issues.is_empty());
        assert_eq!(bulletin.report_count(), 2);
        assert_eq!(bulletin.reports[1].kind, MetarReportKind::Speci);
        assert_eq!(bulletin.reports[1].station, "KDSM");
    }

    #[test]
    fn rejects_non_metar_body() {
        let text = "000 \nFXUS61 KBOX 022101\nAREA FORECAST DISCUSSION\n";
        assert!(parse_metar_bulletin(text).is_none());
    }

    #[test]
    fn parses_corrected_metar_report() {
        let text = "METAR COR UGKO 090030Z 24007KT 9999 SCT030 BKN061 03/01 Q1029 NOSIG=";
        let (bulletin, issues) =
            parse_metar_bulletin(text).expect("expected corrected METAR report");

        assert_eq!(bulletin.report_count(), 1);
        assert_eq!(bulletin.reports[0].station, "UGKO");
        assert!(issues.is_empty());
    }

    #[test]
    fn parses_corrected_speci_report() {
        let text = "SPECI COR KDSM 070254Z 33007KT 10SM CLR M09/M14 A3017=";
        let (bulletin, issues) =
            parse_metar_bulletin(text).expect("expected corrected SPECI report");

        assert_eq!(bulletin.report_count(), 1);
        assert_eq!(bulletin.reports[0].station, "KDSM");
        assert!(issues.is_empty());
    }

    #[test]
    fn invalid_metar_token_emits_issue() {
        let text = "METAR BAD 070254Z=METAR KDSM 070254Z AUTO CLR=";
        let (_, issues) = parse_metar_bulletin(text)
            .expect("expected issue-bearing bulletin with one valid report");

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "invalid_metar_report");
    }

    #[test]
    fn raw_report_uses_compacted_whitespace() {
        let text = "METAR   BGKK   070220Z   AUTO   VRB02KT=";
        let (bulletin, _) = parse_metar_bulletin(text).expect("expected METAR bulletin");

        assert_eq!(bulletin.reports[0].raw, "METAR BGKK 070220Z AUTO VRB02KT");
    }

    #[test]
    fn parses_compact_metar_prefix() {
        let text = "METARSBUF 112000Z AUTO 13006KT CAVOK 32/19 Q1009=";
        let (bulletin, issues) =
            parse_metar_bulletin(text).expect("expected compact METAR bulletin");

        assert!(issues.is_empty());
        assert_eq!(bulletin.report_count(), 1);
        assert_eq!(bulletin.reports[0].station, "SBUF");
        assert_eq!(bulletin.reports[0].observation_time, "112000Z");
    }
}
