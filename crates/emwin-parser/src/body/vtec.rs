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

    /// Human-readable status description when known
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_description: Option<&'static str>,

    /// Action to be taken for this event
    pub action: VtecAction,

    /// Raw 3-character VTEC action code
    pub action_code: String,

    /// Human-readable action description when known
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_description: Option<&'static str>,

    /// 4-character NWS office identifier (WFO)
    pub office: String,

    /// 2-character phenomenon code (e.g., "TO" for tornado, "SV" for severe thunderstorm)
    pub phenomena: String,

    /// Human-readable phenomenon description when known
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phenomena_description: Option<&'static str>,

    /// Significance level: 'W' (Warning), 'A' (Watch), 'Y' (Advisory), 'S' (Statement)
    pub significance: char,

    /// Human-readable significance description when known
    #[serde(skip_serializing_if = "Option::is_none")]
    pub significance_description: Option<&'static str>,

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
    /// Continuation of an existing event
    Continued,
    /// Changed in time
    ChangedInTime,
    /// Changed in area
    ChangedInArea,
    /// Changed in both time and area
    ChangedInTimeAndArea,
    /// Cancellation of an event
    Cancelled,
    /// Event upgrade (e.g., Watch to Warning)
    Upgraded,
    /// Event expiration
    Expired,
    /// Corrected event
    Corrected,
    /// Routine message
    Routine,
    /// Unknown or unrecognized action
    Unknown,
}

impl VtecAction {
    fn from_code(code: &str) -> Self {
        match code {
            "NEW" => VtecAction::New,
            "CON" => VtecAction::Continued,
            "EXT" => VtecAction::ChangedInTime,
            "EXA" => VtecAction::ChangedInArea,
            "EXB" => VtecAction::ChangedInTimeAndArea,
            "CAN" => VtecAction::Cancelled,
            "UPG" => VtecAction::Upgraded,
            "EXP" => VtecAction::Expired,
            "COR" => VtecAction::Corrected,
            "ROU" => VtecAction::Routine,
            _ => VtecAction::Unknown,
        }
    }

    /// Returns the canonical VTEC action code when known.
    pub fn code(self) -> Option<&'static str> {
        match self {
            VtecAction::New => Some("NEW"),
            VtecAction::Continued => Some("CON"),
            VtecAction::ChangedInTime => Some("EXT"),
            VtecAction::ChangedInArea => Some("EXA"),
            VtecAction::ChangedInTimeAndArea => Some("EXB"),
            VtecAction::Cancelled => Some("CAN"),
            VtecAction::Upgraded => Some("UPG"),
            VtecAction::Expired => Some("EXP"),
            VtecAction::Corrected => Some("COR"),
            VtecAction::Routine => Some("ROU"),
            VtecAction::Unknown => None,
        }
    }

    /// Returns a human-readable description of the action when known.
    pub fn description(self) -> Option<&'static str> {
        match self {
            VtecAction::New => Some("New"),
            VtecAction::Continued => Some("Continued"),
            VtecAction::ChangedInTime => Some("Changed in Time"),
            VtecAction::ChangedInArea => Some("Changed in Area"),
            VtecAction::ChangedInTimeAndArea => Some("Changed in Time and Area"),
            VtecAction::Cancelled => Some("Cancelled"),
            VtecAction::Upgraded => Some("Upgraded"),
            VtecAction::Expired => Some("Expired"),
            VtecAction::Corrected => Some("Corrected"),
            VtecAction::Routine => Some("Routine"),
            VtecAction::Unknown => None,
        }
    }
}

fn vtec_phenomena_description(code: &str) -> Option<&'static str> {
    match code {
        "AF" => Some("Ashfall"),
        "AS" => Some("Air Stagnation"),
        "AV" => Some("Avalanche"),
        "BH" => Some("Beach Hazard"),
        "BS" => Some("Blowing Snow"),
        "BW" => Some("Brisk Wind"),
        "BZ" => Some("Blizzard"),
        "CF" => Some("Coastal Flood"),
        "CW" => Some("Cold Weather"),
        "DS" => Some("Dust Storm"),
        "DU" => Some("Blowing Dust"),
        "EC" => Some("Extreme Cold"),
        "EH" => Some("Excessive Heat"),
        "EW" => Some("Extreme Wind"),
        "FA" => Some("Areal Flood"),
        "FF" => Some("Flash Flood"),
        "FG" => Some("Dense Fog"),
        "FL" => Some("Flood"),
        "FR" => Some("Frost"),
        "FW" => Some("Fire Weather"),
        "FZ" => Some("Freeze"),
        "GL" => Some("Gale"),
        "HF" => Some("Hurricane Force Wind"),
        "HI" => Some("Inland Hurricane Wind"),
        "HS" => Some("Heavy Snow"),
        "HT" => Some("Heat"),
        "HU" => Some("Hurricane"),
        "HW" => Some("High Wind"),
        "HY" => Some("Hydrologic"),
        "HZ" => Some("Hard Freeze"),
        "IP" => Some("Sleet"),
        "IS" => Some("Ice Storm"),
        "LB" => Some("Lake Effect Snow and Blowing Snow"),
        "LE" => Some("Lake Effect Snow"),
        "LO" => Some("Low Water"),
        "LS" => Some("Lakeshore Flood"),
        "LW" => Some("Lake Wind"),
        "MA" => Some("Marine"),
        "MF" => Some("Marine Dense Fog"),
        "MH" => Some("Marine Ashfall"),
        "MS" => Some("Marine Dense Smoke"),
        "RB" => Some("Small Craft for Rough Bar"),
        "RP" => Some("Rip Currents"),
        "SB" => Some("Snow And Blowing Snow"),
        "SC" => Some("Small Craft"),
        "SE" => Some("Hazardous Seas"),
        "SI" => Some("Small Craft for Winds"),
        "SM" => Some("Smoke"),
        "SN" => Some("Snow"),
        "SQ" => Some("Snow Squall"),
        "SR" => Some("Storm"),
        "SS" => Some("Storm Surge"),
        "SU" => Some("High Surf"),
        "SV" => Some("Severe Thunderstorm"),
        "SW" => Some("Small Craft for Hazardous Seas"),
        "TI" => Some("Inland Tropical Storm Wind"),
        "TO" => Some("Tornado"),
        "TR" => Some("Tropical Storm"),
        "TS" => Some("Tsunami"),
        "TY" => Some("Typhoon"),
        "UP" => Some("Freezing Spray"),
        "WC" => Some("Wind Chill"),
        "WI" => Some("Wind"),
        "WS" => Some("Winter Storm"),
        "WW" => Some("Winter Weather"),
        "ZF" => Some("Freezing Fog"),
        "ZR" => Some("Freezing Rain"),
        _ => None,
    }
}

fn vtec_status_description(code: char) -> Option<&'static str> {
    match code {
        'O' => Some("Operational"),
        'T' => Some("Test"),
        'E' => Some("Experimental"),
        _ => None,
    }
}

fn vtec_significance_description(code: char) -> Option<&'static str> {
    match code {
        'W' => Some("Warning"),
        'A' => Some("Watch"),
        'Y' => Some("Advisory"),
        'S' => Some("Statement"),
        _ => None,
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
/// assert_eq!(codes[0].status_description, Some("Operational"));
/// assert_eq!(codes[0].phenomena_description, Some("Tornado"));
/// assert_eq!(codes[0].significance_description, Some("Warning"));
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
    let status_description = vtec_status_description(status);
    let action_code = cap.get(2).map(|value| value.as_str()).ok_or_else(|| {
        ProductParseIssue::new(
            "vtec_parse",
            "invalid_vtec_format",
            format!("could not parse VTEC code: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;
    let action = VtecAction::from_code(action_code);
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
    let action_description = action.description();
    let phenomena_description = vtec_phenomena_description(&phenomena);
    let significance_description = vtec_significance_description(significance);

    Ok(VtecCode {
        status,
        status_description,
        action,
        action_code: action_code.to_string(),
        action_description,
        office,
        phenomena,
        phenomena_description,
        significance,
        significance_description,
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
        assert_eq!(codes[0].status_description, Some("Operational"));
        assert_eq!(codes[0].action, VtecAction::New);
        assert_eq!(codes[0].action_code, "NEW");
        assert_eq!(codes[0].action_description, Some("New"));
        assert_eq!(codes[0].office, "KDMX");
        assert_eq!(codes[0].phenomena, "TO");
        assert_eq!(codes[0].phenomena_description, Some("Tornado"));
        assert_eq!(codes[0].significance, 'W');
        assert_eq!(codes[0].significance_description, Some("Warning"));
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
            ("CON", VtecAction::Continued),
            ("EXT", VtecAction::ChangedInTime),
            ("EXA", VtecAction::ChangedInArea),
            ("EXB", VtecAction::ChangedInTimeAndArea),
            ("CAN", VtecAction::Cancelled),
            ("UPG", VtecAction::Upgraded),
            ("EXP", VtecAction::Expired),
            ("COR", VtecAction::Corrected),
            ("ROU", VtecAction::Routine),
            ("XXX", VtecAction::Unknown),
        ];

        for (action_str, expected) in actions {
            let text = format!(
                "/O.{}.KDMX.TO.W.0123.250301T1200Z-250301T1300Z/",
                action_str
            );
            let codes = parse_vtec_codes(&text);
            assert_eq!(codes[0].action, expected);
            assert_eq!(codes[0].action_code, action_str);
        }
    }

    #[test]
    fn parse_vtec_unknown_phenomena_has_no_description() {
        let text = "/O.NEW.KDMX.XX.W.0123.250301T1200Z-250301T1300Z/";
        let codes = parse_vtec_codes(text);

        assert_eq!(codes.len(), 1);
        assert_eq!(codes[0].phenomena, "XX");
        assert_eq!(codes[0].phenomena_description, None);
    }

    #[test]
    fn parse_vtec_status_and_significance_descriptions() {
        let text = "/T.NEW.KDMX.TO.A.0123.250301T1200Z-250301T1300Z/";
        let codes = parse_vtec_codes(text);

        assert_eq!(codes.len(), 1);
        assert_eq!(codes[0].status, 'T');
        assert_eq!(codes[0].status_description, Some("Test"));
        assert_eq!(codes[0].significance, 'A');
        assert_eq!(codes[0].significance_description, Some("Watch"));
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
