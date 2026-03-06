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

/// A parsed HVTEC code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HvtecCode {
    /// 5-character NWSLI (National Weather Service Location Identifier)
    /// River gauge or flood forecast point identifier
    pub nwslid: String,

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
/// ```
pub fn parse_hvtec_codes(text: &str) -> Vec<HvtecCode> {
    hvtec_regex()
        .captures_iter(text)
        .filter_map(|cap| parse_hvtec_capture(&cap))
        .collect()
}

fn hvtec_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"/([A-Z0-9]{5})\.(\d)\.([A-Z]+)\.([0-9TZ]+)\.([0-9TZ]+)\.([0-9TZ]+)\.(\w{2})/")
            .expect("hvtec regex compiles")
    })
}

fn parse_hvtec_capture(cap: &regex::Captures<'_>) -> Option<HvtecCode> {
    let nwslid = cap.get(1)?.as_str().to_string();
    let severity = HvtecSeverity::from_str(cap.get(2)?.as_str());
    let cause = HvtecCause::from_str(cap.get(3)?.as_str());
    let begin_str = cap.get(4)?.as_str();
    let crest_str = cap.get(5)?.as_str();
    let end_str = cap.get(6)?.as_str();
    let record = HvtecRecord::from_str(cap.get(7)?.as_str());

    let begin = parse_hvtec_time(begin_str)?;
    let crest = parse_hvtec_time(crest_str)?;
    let end = parse_hvtec_time(end_str)?;

    Some(HvtecCode {
        nwslid,
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
        assert_eq!(codes[0].severity, HvtecSeverity::Level3);
        assert_eq!(codes[0].cause, HvtecCause::ExcessiveRainfall);
        assert_eq!(codes[0].record, HvtecRecord::NoRecord);
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
    fn parse_hvtec_multiple() {
        let text = concat!(
            "/MSRM1.3.ER.250301T1200Z.250301T1800Z.250302T0000Z.NO/",
            " some text ",
            "/ABCD1.2.SM.250301T1000Z.250301T1500Z.250302T0800Z.NR/"
        );
        let codes = parse_hvtec_codes(text);

        assert_eq!(codes.len(), 2);
        assert_eq!(codes[0].nwslid, "MSRM1");
        assert_eq!(codes[1].nwslid, "ABCD1");
    }
}
