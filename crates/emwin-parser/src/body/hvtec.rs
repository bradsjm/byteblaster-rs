//! NWS HVTEC (Hydrologic VTEC) parsing module.
//!
//! HVTEC extends VTEC to provide hydrologic-specific information for flood
//! warnings, including river gauge data, crest times, and record status.
//!
//! HVTEC format: `/NWSLI.Severity.Cause.Begin.Crest.End.Record/`
//!
//! Example: `/MSRM1.ER.ER.250301T1200Z.250301T1800Z.250302T0000Z.NO/`

use chrono::{DateTime, NaiveDateTime, Utc};
use regex::Regex;
use std::sync::OnceLock;

use crate::ProductParseIssue;
use crate::data::{NwslidEntry, nwslid_entry};

/// A parsed HVTEC code.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct HvtecCode {
    /// 5-character NWSLI (National Weather Service Location Identifier)
    /// River gauge or flood forecast point identifier
    pub nwslid: String,

    /// Enriched hydrologic location metadata for the NWSLID, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<NwslidEntry>,

    /// Severity level (1, 2, or 3)
    pub severity: HvtecSeverity,

    /// Immediate cause of flooding
    pub cause: HvtecCause,

    /// Event begin time in UTC
    pub begin: DateTime<Utc>,

    /// Forecast crest time in UTC
    pub crest: DateTime<Utc>,

    /// Event end time in UTC
    pub end: DateTime<Utc>,

    /// Record status indicator
    pub record: HvtecRecord,
}

/// Severity levels for HVTEC flood events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum HvtecSeverity {
    /// Level 1 - Minor flooding
    Level1,
    /// Level 2 - Moderate flooding
    Level2,
    /// Level 3 - Major flooding
    Level3,
    /// Unknown severity
    Unknown,
}

impl HvtecSeverity {
    fn from_str(s: &str) -> Self {
        match s {
            "1" => HvtecSeverity::Level1,
            "2" => HvtecSeverity::Level2,
            "3" => HvtecSeverity::Level3,
            _ => HvtecSeverity::Unknown,
        }
    }
}

/// Immediate cause codes for HVTEC events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum HvtecCause {
    /// Excessive Rainfall (ER)
    ExcessiveRainfall,
    /// Snowmelt (SM)
    Snowmelt,
    /// Dam/Levee Break or Failure (DM)
    DamFailure,
    /// Ice Jam (IJ)
    IceJam,
    /// Other/Unknown cause
    Other,
}

impl HvtecCause {
    fn from_str(s: &str) -> Self {
        match s {
            "ER" => HvtecCause::ExcessiveRainfall,
            "SM" => HvtecCause::Snowmelt,
            "DM" => HvtecCause::DamFailure,
            "IJ" => HvtecCause::IceJam,
            _ => HvtecCause::Other,
        }
    }
}

/// Record status indicators for HVTEC events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum HvtecRecord {
    /// No record - not approaching or exceeding record levels
    NoRecord,
    /// Near record - approaching record levels
    NearRecord,
    /// Record - at or exceeding record levels
    Record,
    /// Unknown record status
    Unknown,
}

impl HvtecRecord {
    fn from_str(s: &str) -> Self {
        match s {
            "NO" => HvtecRecord::NoRecord,
            "NR" => HvtecRecord::NearRecord,
            "OO" => HvtecRecord::Record,
            _ => HvtecRecord::Unknown,
        }
    }
}

/// Parses all HVTEC codes found in the given text.
///
/// This function searches for HVTEC codes throughout the entire text and returns
/// all valid matches found. Invalid or malformed HVTEC codes are skipped.
///
/// # Arguments
///
/// * `text` - The text to search for HVTEC codes
///
/// # Returns
///
/// A vector of parsed `HvtecCode` structs. Returns an empty vector if no valid
/// HVTEC codes are found.
///
/// # Examples
///
/// ```
/// use emwin_parser::parse_hvtec_codes;
///
/// let text = r#"... /MSRM1.3.ER.250301T1200Z.250301T1800Z.250302T0000Z.NO/ ..."#;
/// let codes = parse_hvtec_codes(text);
///
/// assert_eq!(codes.len(), 1);
/// assert_eq!(codes[0].nwslid, "MSRM1");
/// assert_eq!(codes[0].severity, emwin_parser::HvtecSeverity::Level3);
/// assert!(codes[0].location.is_none());
/// ```
pub fn parse_hvtec_codes(text: &str) -> Vec<HvtecCode> {
    parse_hvtec_codes_with_issues(text).0
}

pub fn parse_hvtec_codes_with_issues(text: &str) -> (Vec<HvtecCode>, Vec<ProductParseIssue>) {
    let mut codes = Vec::new();
    let mut issues = Vec::new();

    for candidate in hvtec_candidate_regex().find_iter(text) {
        let raw = candidate.as_str();
        let Some(captures) = hvtec_regex().captures(raw) else {
            issues.push(ProductParseIssue::new(
                "hvtec_parse",
                "invalid_hvtec_format",
                format!("could not parse HVTEC code: `{raw}`"),
                Some(raw.to_string()),
            ));
            continue;
        };

        match parse_hvtec_capture(&captures, raw) {
            Ok(code) => codes.push(code),
            Err(issue) => issues.push(issue),
        }
    }

    (codes, issues)
}

fn hvtec_candidate_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"/[A-Z0-9]{5}\.[^/\r\n]+/").expect("hvtec candidate regex compiles")
    })
}

fn hvtec_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"/([A-Z0-9]{5})\.([^./\r\n]+)\.([A-Z]+)\.([^./\r\n]+)\.([^./\r\n]+)\.([^./\r\n]+)\.(\w{2})/")
            .expect("hvtec regex compiles")
    })
}

fn parse_hvtec_capture(
    cap: &regex::Captures<'_>,
    raw: &str,
) -> Result<HvtecCode, ProductParseIssue> {
    let nwslid = cap
        .get(1)
        .map(|value| value.as_str().to_string())
        .ok_or_else(|| {
            ProductParseIssue::new(
                "hvtec_parse",
                "invalid_hvtec_format",
                format!("could not parse HVTEC code: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;
    let location = nwslid_entry(&nwslid).copied();
    let severity =
        HvtecSeverity::from_str(cap.get(2).map(|value| value.as_str()).ok_or_else(|| {
            ProductParseIssue::new(
                "hvtec_parse",
                "invalid_hvtec_format",
                format!("could not parse HVTEC code: `{raw}`"),
                Some(raw.to_string()),
            )
        })?);
    let cause = HvtecCause::from_str(cap.get(3).map(|value| value.as_str()).ok_or_else(|| {
        ProductParseIssue::new(
            "hvtec_parse",
            "invalid_hvtec_format",
            format!("could not parse HVTEC code: `{raw}`"),
            Some(raw.to_string()),
        )
    })?);
    let begin_str = cap.get(4).map(|value| value.as_str()).ok_or_else(|| {
        ProductParseIssue::new(
            "hvtec_parse",
            "invalid_hvtec_format",
            format!("could not parse HVTEC code: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;
    let crest_str = cap.get(5).map(|value| value.as_str()).ok_or_else(|| {
        ProductParseIssue::new(
            "hvtec_parse",
            "invalid_hvtec_format",
            format!("could not parse HVTEC code: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;
    let end_str = cap.get(6).map(|value| value.as_str()).ok_or_else(|| {
        ProductParseIssue::new(
            "hvtec_parse",
            "invalid_hvtec_format",
            format!("could not parse HVTEC code: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;
    let record =
        HvtecRecord::from_str(cap.get(7).map(|value| value.as_str()).ok_or_else(|| {
            ProductParseIssue::new(
                "hvtec_parse",
                "invalid_hvtec_format",
                format!("could not parse HVTEC code: `{raw}`"),
                Some(raw.to_string()),
            )
        })?);

    let begin = parse_hvtec_time(begin_str).ok_or_else(|| {
        ProductParseIssue::new(
            "hvtec_parse",
            "invalid_hvtec_begin_time",
            format!("could not parse HVTEC begin time from code: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;
    let crest = parse_hvtec_time(crest_str).ok_or_else(|| {
        ProductParseIssue::new(
            "hvtec_parse",
            "invalid_hvtec_crest_time",
            format!("could not parse HVTEC crest time from code: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;
    let end = parse_hvtec_time(end_str).ok_or_else(|| {
        ProductParseIssue::new(
            "hvtec_parse",
            "invalid_hvtec_end_time",
            format!("could not parse HVTEC end time from code: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;

    Ok(HvtecCode {
        nwslid,
        location,
        severity,
        cause,
        begin,
        crest,
        end,
        record,
    })
}

fn parse_hvtec_time(time_str: &str) -> Option<DateTime<Utc>> {
    // HVTEC time format: YYMMDDTHHMMZ (12 characters)
    if time_str.len() != 12 {
        return None;
    }

    let naive = NaiveDateTime::parse_from_str(time_str, "%y%m%dT%H%MZ").ok()?;
    Some(naive.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_hvtec() {
        let text = "/MSRM1.3.ER.250301T1200Z.250301T1800Z.250302T0000Z.NO/";
        let codes = parse_hvtec_codes(text);

        assert_eq!(codes.len(), 1);
        assert_eq!(codes[0].nwslid, "MSRM1");
        assert!(codes[0].location.is_none());
        assert_eq!(codes[0].severity, HvtecSeverity::Level3);
        assert_eq!(codes[0].cause, HvtecCause::ExcessiveRainfall);
        assert_eq!(codes[0].record, HvtecRecord::NoRecord);
    }

    #[test]
    fn parse_hvtec_enriches_known_nwslid() {
        let text = "/CHFA2.3.ER.250301T1200Z.250301T1800Z.250302T0000Z.NO/";
        let codes = parse_hvtec_codes(text);

        assert_eq!(codes.len(), 1);
        let location = codes[0].location.expect("expected known location");
        assert_eq!(location.nwslid, "CHFA2");
        assert_eq!(location.state_code, "AK");
        assert_eq!(location.stream_name, "Chena River");
        assert_eq!(location.proximity, "at");
        assert_eq!(location.place_name, "Fairbanks");
        assert_eq!(location.latitude, 64.8458);
        assert_eq!(location.longitude, -147.7011);
    }

    #[test]
    fn parse_hvtec_severity_levels() {
        for (level, expected) in [
            ("1", HvtecSeverity::Level1),
            ("2", HvtecSeverity::Level2),
            ("3", HvtecSeverity::Level3),
        ] {
            let text = format!(
                "/MSRM1.{}.ER.250301T1200Z.250301T1800Z.250302T0000Z.NO/",
                level
            );
            let codes = parse_hvtec_codes(&text);
            assert_eq!(codes[0].severity, expected);
        }
    }

    #[test]
    fn parse_hvtec_causes() {
        let causes = vec![
            ("ER", HvtecCause::ExcessiveRainfall),
            ("SM", HvtecCause::Snowmelt),
            ("DM", HvtecCause::DamFailure),
            ("IJ", HvtecCause::IceJam),
        ];

        for (cause_str, expected) in causes {
            let text = format!(
                "/MSRM1.3.{}.250301T1200Z.250301T1800Z.250302T0000Z.NO/",
                cause_str
            );
            let codes = parse_hvtec_codes(&text);
            assert_eq!(codes[0].cause, expected);
        }
    }

    #[test]
    fn parse_hvtec_record_status() {
        let records = vec![
            ("NO", HvtecRecord::NoRecord),
            ("NR", HvtecRecord::NearRecord),
            ("OO", HvtecRecord::Record),
        ];

        for (record_str, expected) in records {
            let text = format!(
                "/MSRM1.3.ER.250301T1200Z.250301T1800Z.250302T0000Z.{}/",
                record_str
            );
            let codes = parse_hvtec_codes(&text);
            assert_eq!(codes[0].record, expected);
        }
    }

    #[test]
    fn parse_hvtec_times() {
        let text = "/MSRM1.3.ER.250301T1200Z.250301T1800Z.250302T0000Z.NO/";
        let codes = parse_hvtec_codes(text);

        assert_eq!(codes[0].begin.timestamp(), 1740830400);
        assert_eq!(codes[0].crest.timestamp(), 1740852000);
        assert_eq!(codes[0].end.timestamp(), 1740873600);
    }

    #[test]
    fn parse_hvtec_empty() {
        let codes = parse_hvtec_codes("");
        assert!(codes.is_empty());
    }

    #[test]
    fn parse_hvtec_invalid_skipped() {
        let text = "/INVALID.HVTEC.CODE/";
        let codes = parse_hvtec_codes(text);
        assert!(codes.is_empty());
    }

    #[test]
    fn parse_hvtec_invalid_reports_issue() {
        let text = "/MSRM1.3.ER.250301T1200Z.invalid.250302T0000Z.NO/";
        let (codes, issues) = parse_hvtec_codes_with_issues(text);

        assert!(codes.is_empty());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "invalid_hvtec_crest_time");
    }

    #[test]
    fn parse_hvtec_multiple() {
        let text = concat!(
            "/MSRM1.3.ER.250301T1200Z.250301T1800Z.250302T0000Z.NO/",
            " some text ",
            "/CHFA2.2.SM.250301T1000Z.250301T1500Z.250302T0800Z.NR/"
        );
        let codes = parse_hvtec_codes(text);

        assert_eq!(codes.len(), 2);
        assert_eq!(codes[0].nwslid, "MSRM1");
        assert_eq!(codes[1].nwslid, "CHFA2");
        assert!(codes[0].location.is_none());
        assert_eq!(
            codes[1]
                .location
                .as_ref()
                .map(|location| location.place_name),
            Some("Fairbanks")
        );
    }
}
