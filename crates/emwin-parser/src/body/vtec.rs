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

/// A parsed VTEC code.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    vtec_regex()
        .captures_iter(text)
        .filter_map(|cap| parse_vtec_capture(&cap))
        .collect()
}

fn vtec_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"/([A-Z])\.([A-Z]+)\.([A-Z]{4})\.([A-Z]{2})\.([A-Z])\.([0-9]+)\.([0-9TZ]+)-([0-9TZ]+)/",
        )
        .expect("vtec regex compiles")
    })
}

fn parse_vtec_capture(cap: &regex::Captures<'_>) -> Option<VtecCode> {
    let status = cap.get(1)?.as_str().chars().next()?;
    let action = VtecAction::from_str(cap.get(2)?.as_str());
    let office = cap.get(3)?.as_str().to_string();
    let phenomena = cap.get(4)?.as_str().to_string();
    let significance = cap.get(5)?.as_str().chars().next()?;
    let etn = cap.get(6)?.as_str().parse().ok()?;
    let begin_str = cap.get(7)?.as_str();
    let end_str = cap.get(8)?.as_str();

    let begin = parse_vtec_time(begin_str)?;
    let end = parse_vtec_time(end_str)?;

    Some(VtecCode {
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
