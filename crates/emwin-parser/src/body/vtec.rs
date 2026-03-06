//! NWS VTEC (Valid Time Event Code) parsing module.
//!
//! VTEC codes provide standardized timing and event information within NWS
//! warnings, watches, and advisories. Format:
//! `/Status.Action.Office.Phenomena.Significance.ETN.Begin-End/`
//!
//! Example: `/O.NEW.KDMX.TO.W.0123.250301T1200Z-250301T1300Z/`

use chrono::{DateTime, NaiveDateTime, Utc};
use regex::Regex;
use std::sync::OnceLock;

use crate::ProductParseIssue;

/// A parsed VTEC code.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VtecCode {
    /// Status indicator: 'O' (Operational), 'T' (Test), or 'E' (Experimental)
    pub status: char,

    /// Action to be taken for this event
    pub action: VtecAction,

    /// 4-character NWS office identifier (WFO)
    pub office: String,

    /// 2-character phenomenon code (e.g., "TO" for tornado, "SV" for severe thunderstorm)
    pub phenomena: String,

    /// Significance level: 'W' (Warning), 'A' (Watch), 'Y' (Advisory), 'S' (Statement)
    pub significance: char,

    /// Event Tracking Number (ETN) - unique identifier for this event type/office
    pub etn: u32,

    /// Event begin time in UTC
    pub begin: DateTime<Utc>,

    /// Event end time in UTC
    pub end: DateTime<Utc>,
}

/// VTEC action codes indicating what action to take for an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum VtecAction {
    /// New event
    New,
    /// Continuation of existing event
    Continue,
    /// Cancellation of event
    Cancel,
    /// Extension of event time or area
    Extend,
    /// Event upgrade (e.g., Watch to Warning)
    Upgrade,
    /// Event downgrade (e.g., Warning to Advisory)
    Downgrade,
    /// Event expiration
    Expire,
    /// Unknown or unrecognized action
    Unknown,
}

impl VtecAction {
    fn from_str(s: &str) -> Self {
        match s {
            "NEW" => VtecAction::New,
            "CON" => VtecAction::Continue,
            "CAN" => VtecAction::Cancel,
            "EXT" => VtecAction::Extend,
            "UPG" => VtecAction::Upgrade,
            "DGD" => VtecAction::Downgrade,
            "EXP" => VtecAction::Expire,
            _ => VtecAction::Unknown,
        }
    }
}

/// Parses all VTEC codes found in the given text.
///
/// This function searches for VTEC codes throughout the entire text and returns
/// all valid matches found. Invalid or malformed VTEC codes are skipped.
///
/// # Arguments
///
/// * `text` - The text to search for VTEC codes
///
/// # Returns
///
/// A vector of parsed `VtecCode` structs. Returns an empty vector if no valid
/// VTEC codes are found.
///
/// # Examples
///
/// ```
/// use emwin_parser::parse_vtec_codes;
///
/// let text = r#"... /O.NEW.KDMX.TO.W.0123.250301T1200Z-250301T1300Z/ ..."#;
/// let codes = parse_vtec_codes(text);
///
/// assert_eq!(codes.len(), 1);
/// assert_eq!(codes[0].office, "KDMX");
/// assert_eq!(codes[0].phenomena, "TO");
/// ```
pub fn parse_vtec_codes(text: &str) -> Vec<VtecCode> {
    parse_vtec_codes_with_issues(text).0
}

pub fn parse_vtec_codes_with_issues(text: &str) -> (Vec<VtecCode>, Vec<ProductParseIssue>) {
    let mut codes = Vec::new();
    let mut issues = Vec::new();

    for candidate in vtec_candidate_regex().find_iter(text) {
        let raw = candidate.as_str();
        let Some(captures) = vtec_regex().captures(raw) else {
            issues.push(ProductParseIssue::new(
                "vtec_parse",
                "invalid_vtec_format",
                format!("could not parse VTEC code: `{raw}`"),
                Some(raw.to_string()),
            ));
            continue;
        };

        match parse_vtec_capture(&captures, raw) {
            Ok(code) => codes.push(code),
            Err(issue) => issues.push(issue),
        }
    }

    (codes, issues)
}

fn vtec_candidate_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"/[OTE]\.[^/\r\n]+/").expect("vtec candidate regex compiles"))
}

fn vtec_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"/([A-Z])\.([A-Z]+)\.([A-Z]{4})\.([A-Z]{2})\.([A-Z])\.([^./\r\n]+)\.([^- /\r\n]+)-([^/\r\n]+)/",
        )
        .expect("vtec regex compiles")
    })
}

fn parse_vtec_capture(cap: &regex::Captures<'_>, raw: &str) -> Result<VtecCode, ProductParseIssue> {
    let status = cap
        .get(1)
        .and_then(|value| value.as_str().chars().next())
        .ok_or_else(|| {
            ProductParseIssue::new(
                "vtec_parse",
                "invalid_vtec_format",
                format!("could not parse VTEC code: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;
    let action = VtecAction::from_str(cap.get(2).map(|value| value.as_str()).ok_or_else(|| {
        ProductParseIssue::new(
            "vtec_parse",
            "invalid_vtec_format",
            format!("could not parse VTEC code: `{raw}`"),
            Some(raw.to_string()),
        )
    })?);
    let office = cap
        .get(3)
        .map(|value| value.as_str().to_string())
        .ok_or_else(|| {
            ProductParseIssue::new(
                "vtec_parse",
                "invalid_vtec_format",
                format!("could not parse VTEC code: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;
    let phenomena = cap
        .get(4)
        .map(|value| value.as_str().to_string())
        .ok_or_else(|| {
            ProductParseIssue::new(
                "vtec_parse",
                "invalid_vtec_format",
                format!("could not parse VTEC code: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;
    let significance = cap
        .get(5)
        .and_then(|value| value.as_str().chars().next())
        .ok_or_else(|| {
            ProductParseIssue::new(
                "vtec_parse",
                "invalid_vtec_format",
                format!("could not parse VTEC code: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;
    let etn = cap
        .get(6)
        .and_then(|value| value.as_str().parse().ok())
        .ok_or_else(|| {
            ProductParseIssue::new(
                "vtec_parse",
                "invalid_vtec_etn",
                format!("could not parse VTEC ETN from code: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;
    let begin_str = cap.get(7).map(|value| value.as_str()).ok_or_else(|| {
        ProductParseIssue::new(
            "vtec_parse",
            "invalid_vtec_format",
            format!("could not parse VTEC code: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;
    let end_str = cap.get(8).map(|value| value.as_str()).ok_or_else(|| {
        ProductParseIssue::new(
            "vtec_parse",
            "invalid_vtec_format",
            format!("could not parse VTEC code: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;

    let begin = parse_vtec_time(begin_str).ok_or_else(|| {
        ProductParseIssue::new(
            "vtec_parse",
            "invalid_vtec_begin_time",
            format!("could not parse VTEC begin time from code: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;
    let end = parse_vtec_time(end_str).ok_or_else(|| {
        ProductParseIssue::new(
            "vtec_parse",
            "invalid_vtec_end_time",
            format!("could not parse VTEC end time from code: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;

    Ok(VtecCode {
        status,
        action,
        office,
        phenomena,
        significance,
        etn,
        begin,
        end,
    })
}

fn parse_vtec_time(time_str: &str) -> Option<DateTime<Utc>> {
    // VTEC time format: YYMMDDTHHMMZ (12 characters)
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
    fn parse_single_vtec() {
        let text = "/O.NEW.KDMX.TO.W.0123.250301T1200Z-250301T1300Z/";
        let codes = parse_vtec_codes(text);

        assert_eq!(codes.len(), 1);
        assert_eq!(codes[0].status, 'O');
        assert_eq!(codes[0].action, VtecAction::New);
        assert_eq!(codes[0].office, "KDMX");
        assert_eq!(codes[0].phenomena, "TO");
        assert_eq!(codes[0].significance, 'W');
        assert_eq!(codes[0].etn, 123);
    }

    #[test]
    fn parse_multiple_vtec() {
        let text = concat!(
            "/O.NEW.KDMX.TO.W.0123.250301T1200Z-250301T1300Z/",
            " ... ",
            "/O.CON.KTOP.SV.W.0045.250301T1300Z-250301T1400Z/"
        );
        let codes = parse_vtec_codes(text);

        assert_eq!(codes.len(), 2);
        assert_eq!(codes[0].office, "KDMX");
        assert_eq!(codes[1].office, "KTOP");
    }

    #[test]
    fn parse_vtec_time_parsing() {
        let text = "/O.NEW.KDMX.TO.W.0001.250301T1200Z-250305T1800Z/";
        let codes = parse_vtec_codes(text);

        assert_eq!(codes.len(), 1);
        assert_eq!(codes[0].begin.timestamp(), 1740830400);
        assert_eq!(codes[0].end.timestamp(), 1741197600);
    }

    #[test]
    fn parse_vtec_various_actions() {
        let actions = vec![
            ("NEW", VtecAction::New),
            ("CON", VtecAction::Continue),
            ("CAN", VtecAction::Cancel),
            ("EXT", VtecAction::Extend),
            ("UPG", VtecAction::Upgrade),
            ("DGD", VtecAction::Downgrade),
            ("EXP", VtecAction::Expire),
            ("XXX", VtecAction::Unknown),
        ];

        for (action_str, expected) in actions {
            let text = format!(
                "/O.{}.KDMX.TO.W.0123.250301T1200Z-250301T1300Z/",
                action_str
            );
            let codes = parse_vtec_codes(&text);
            assert_eq!(codes[0].action, expected);
        }
    }

    #[test]
    fn parse_vtec_empty() {
        let codes = parse_vtec_codes("");
        assert!(codes.is_empty());
    }

    #[test]
    fn parse_vtec_invalid_skipped() {
        let text = "/INVALID.VTEC.CODE/";
        let codes = parse_vtec_codes(text);
        assert!(codes.is_empty());
    }

    #[test]
    fn parse_vtec_invalid_reports_issue() {
        let text = "/O.NEW.KDMX.TO.W.0123.250301T1200Z-invalid/";
        let (codes, issues) = parse_vtec_codes_with_issues(text);

        assert!(codes.is_empty());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "invalid_vtec_end_time");
    }

    #[test]
    fn parse_vtec_mixed_valid_invalid() {
        let text = concat!(
            "/O.NEW.KDMX.TO.W.0123.250301T1200Z-250301T1300Z/",
            " invalid ",
            "/O.CON.KTOP.SV.W.0045.250301T1300Z-250301T1400Z/"
        );
        let codes = parse_vtec_codes(text);

        assert_eq!(codes.len(), 2);
    }
}
