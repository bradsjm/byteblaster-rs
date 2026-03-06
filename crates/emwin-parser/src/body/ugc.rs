//! NWS UGC (Universal Geographic Code) parsing module.
//!
//! UGC codes identify affected geographic areas (counties or zones) within NWS
//! text products. They support range notation for compact representation.
//!
//! UGC format: `[State][Class][Number][>Range][-Continuation]`
//!
//! Examples:
//! - `IAC001` - Iowa County 001
//! - `IAC001>005` - Iowa Counties 001 through 005
//! - `IAC001>005-NEZ010-` - Multiple counties with expiration

use crate::ProductParseIssue;
use crate::time::resolve_day_time_not_before;
use chrono::{DateTime, Utc};
use regex::Regex;
use std::sync::OnceLock;

/// A parsed UGC section containing codes and expiration time.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UgcSection {
    /// Individual UGC codes (expanded from ranges)
    pub codes: Vec<UgcCode>,
    /// Expiration time for this UGC section
    pub expires: DateTime<Utc>,
}

/// A single UGC code representing a county or zone.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UgcCode {
    /// 2-letter state code (e.g., "IA", "NE")
    pub state: String,
    /// Geographic class (County or Zone)
    pub geoclass: UgcClass,
    /// 3-digit county/zone number
    pub number: u16,
}

/// Geographic classification for UGC codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum UgcClass {
    /// County (C)
    County,
    /// Zone (Z)
    Zone,
    /// Fire Zone (F)
    FireZone,
    /// Marine Zone (M)
    Marine,
    /// Unknown classification
    Unknown,
}

impl UgcClass {
    fn from_char(c: char) -> Self {
        match c {
            'C' => UgcClass::County,
            'Z' => UgcClass::Zone,
            'F' => UgcClass::FireZone,
            'M' => UgcClass::Marine,
            _ => UgcClass::Unknown,
        }
    }
}

/// Parses all UGC sections found in the given text.
///
/// This function searches for UGC code blocks throughout the entire text and
/// returns all valid matches found with range expansion applied.
///
/// # Arguments
///
/// * `text` - The text to search for UGC codes
/// * `valid_time` - Reference time for calculating expiration (typically product issue time)
///
/// # Returns
///
/// A vector of parsed `UgcSection` structs. Returns an empty vector if no valid
/// UGC sections are found.
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use emwin_parser::parse_ugc_sections;
///
/// let text = "IAC001>003-041200-\n";
/// let sections = parse_ugc_sections(text, Utc::now());
///
/// assert_eq!(sections.len(), 1);
/// assert_eq!(sections[0].codes.len(), 3); // Expanded from 001>003
/// assert_eq!(sections[0].codes[0].state, "IA");
/// ```
pub fn parse_ugc_sections(text: &str, valid_time: DateTime<Utc>) -> Vec<UgcSection> {
    parse_ugc_sections_with_issues(text, valid_time).0
}

pub fn parse_ugc_sections_with_issues(
    text: &str,
    valid_time: DateTime<Utc>,
) -> (Vec<UgcSection>, Vec<ProductParseIssue>) {
    let mut sections = Vec::new();
    let mut issues = Vec::new();

    for candidate in extract_ugc_candidates(text) {
        match parse_ugc_capture(&candidate, valid_time) {
            Ok(section) => sections.push(section),
            Err(issue) => issues.push(issue),
        }
    }

    (sections, issues)
}

fn ugc_candidate_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[A-Z]{2}[CZFM]").expect("ugc candidate regex compiles"))
}

fn ugc_expiration_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[0-9]{6}-$").expect("ugc expiration regex compiles"))
}

fn extract_ugc_candidates(text: &str) -> Vec<String> {
    let normalized = text.replace('\r', "");
    let lines: Vec<&str> = normalized.lines().collect();
    let mut candidates = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index].trim();
        if !ugc_candidate_regex().is_match(line) {
            index += 1;
            continue;
        }

        let mut combined = line.to_string();
        let mut cursor = index + 1;

        while !ugc_expiration_regex().is_match(&compact_ugc_text(&combined))
            && cursor < lines.len()
            && cursor.saturating_sub(index) < 8
        {
            let next = lines[cursor].trim();
            if next.is_empty() {
                break;
            }
            combined.push_str(next);
            cursor += 1;
        }

        let compact = compact_ugc_text(&combined);
        if ugc_full_regex().is_match(&compact) {
            candidates.push(compact);
            index = cursor;
        } else {
            index += 1;
        }
    }

    candidates
}

fn compact_ugc_text(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Extract expiration code from end of UGC line
fn extract_expiration(text: &str) -> Option<(String, String)> {
    let caps = ugc_full_regex().captures(text)?;
    Some((
        caps.get(1)?.as_str().to_string(),
        caps.get(2)?.as_str().to_string(),
    ))
}

fn ugc_full_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^([A-Z0-9CZFM>,\-]+)-([0-9]{6})-\s*$").expect("ugc full regex compiles")
    })
}

fn parse_ugc_capture(
    text: &str,
    valid_time: DateTime<Utc>,
) -> Result<UgcSection, ProductParseIssue> {
    let (code_block, expire_code) = extract_expiration(text).ok_or_else(|| {
        ProductParseIssue::new(
            "ugc_parse",
            "invalid_ugc_format",
            format!("could not parse UGC section: `{text}`"),
            Some(text.to_string()),
        )
    })?;

    let codes = expand_ugc_block(&code_block).ok_or_else(|| {
        ProductParseIssue::new(
            "ugc_parse",
            "invalid_ugc_codes",
            format!("could not parse UGC codes from section: `{text}`"),
            Some(text.to_string()),
        )
    })?;
    let expires = parse_expire_time(&expire_code, valid_time).ok_or_else(|| {
        ProductParseIssue::new(
            "ugc_parse",
            "invalid_ugc_expiration",
            format!("could not parse UGC expiration from section: `{text}`"),
            Some(text.to_string()),
        )
    })?;

    Ok(UgcSection { codes, expires })
}

fn expand_ugc_block(block: &str) -> Option<Vec<UgcCode>> {
    let mut codes = Vec::new();
    let mut current_prefix: Option<(String, UgcClass)> = None;

    // Split on comma or hyphen (continuation)
    for segment in block.split([',', '-']) {
        if segment.is_empty() {
            continue;
        }

        let segment = segment.trim();
        let (state, geoclass, numeric) =
            if let Some(captures) = ugc_full_segment_regex().captures(segment) {
                let state = captures.get(1)?.as_str().to_string();
                let geoclass = UgcClass::from_char(captures.get(2)?.as_str().chars().next()?);
                let numeric = captures.get(3)?.as_str();
                current_prefix = Some((state.clone(), geoclass));
                (state, geoclass, numeric)
            } else if ugc_shorthand_segment_regex().is_match(segment) {
                if let Some((state, geoclass)) = current_prefix.clone() {
                    (state, geoclass, segment)
                } else {
                    return None;
                }
            } else {
                return None;
            };

        if let Some((start_num, end_num)) = parse_ugc_numeric_range(numeric) {
            for num in start_num..=end_num {
                codes.push(UgcCode {
                    state: state.clone(),
                    geoclass,
                    number: num,
                });
            }
        } else {
            return None;
        }
    }

    if codes.is_empty() { None } else { Some(codes) }
}

fn ugc_full_segment_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^([A-Z]{2})([CZFM])(\d{3}(?:>\d{3})?)$")
            .expect("ugc full segment regex compiles")
    })
}

fn ugc_shorthand_segment_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\d{3}(?:>\d{3})?$").expect("ugc shorthand segment regex compiles")
    })
}

fn parse_ugc_numeric_range(numeric: &str) -> Option<(u16, u16)> {
    if let Some((start, end)) = numeric.split_once('>') {
        let start_num = parse_ugc_number(start)?;
        let end_num = parse_ugc_number(end)?;
        Some((start_num, end_num))
    } else {
        let number = parse_ugc_number(numeric)?;
        Some((number, number))
    }
}

fn parse_ugc_number(text: &str) -> Option<u16> {
    if text.len() != 3 {
        return None;
    }
    text.parse().ok()
}

fn parse_expire_time(expire_code: &str, valid_time: DateTime<Utc>) -> Option<DateTime<Utc>> {
    // Expire format: DDHHMM (day of month, hour, minute)
    if expire_code.len() != 6 {
        return None;
    }

    let day: u32 = expire_code[0..2].parse().ok()?;
    let hour: u32 = expire_code[2..4].parse().ok()?;
    let minute: u32 = expire_code[4..6].parse().ok()?;

    resolve_day_time_not_before(valid_time, day, hour, minute)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn test_valid_time() -> DateTime<Utc> {
        // 2025-03-05 12:00:00 UTC
        Utc.with_ymd_and_hms(2025, 3, 5, 12, 0, 0).unwrap()
    }

    #[test]
    fn parse_single_ugc() {
        let text = "IAC001-051200-\n";
        let sections = parse_ugc_sections(text, test_valid_time());

        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].codes.len(), 1);
        assert_eq!(sections[0].codes[0].state, "IA");
        assert_eq!(sections[0].codes[0].geoclass, UgcClass::County);
        assert_eq!(sections[0].codes[0].number, 1);
    }

    #[test]
    fn parse_ugc_range() {
        let text = "IAC001>003-051200-\n";
        let sections = parse_ugc_sections(text, test_valid_time());

        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].codes.len(), 3);
        assert_eq!(sections[0].codes[0].number, 1);
        assert_eq!(sections[0].codes[1].number, 2);
        assert_eq!(sections[0].codes[2].number, 3);
    }

    #[test]
    fn parse_ugc_multiple() {
        let text = "IAC001-IAC003-IAC005-051200-\n";
        let sections = parse_ugc_sections(text, test_valid_time());

        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].codes.len(), 3);
    }

    #[test]
    fn parse_ugc_zone_class() {
        let text = "IAZ001>003-051200-\n";
        let sections = parse_ugc_sections(text, test_valid_time());

        assert_eq!(sections[0].codes[0].geoclass, UgcClass::Zone);
    }

    #[test]
    fn parse_ugc_mixed_states() {
        let text = "IAC001>003-NEC005-051200-\n";
        let sections = parse_ugc_sections(text, test_valid_time());

        assert_eq!(sections.len(), 1);
        // 001, 002, 003 from IA range + 005 from NE = 4 total
        assert_eq!(sections[0].codes.len(), 4);
        assert_eq!(sections[0].codes[0].state, "IA");
        assert_eq!(sections[0].codes[3].state, "NE");
    }

    #[test]
    fn parse_ugc_expiration() {
        let text = "IAC001-051200-\n";
        let sections = parse_ugc_sections(text, test_valid_time());

        let expected = Utc.with_ymd_and_hms(2025, 3, 5, 12, 0, 0).unwrap();
        assert_eq!(sections[0].expires, expected);
    }

    #[test]
    fn parse_ugc_expiration_next_month() {
        // If valid_time is March 30 and expiration is day 01, it should roll to April
        let valid_time = Utc.with_ymd_and_hms(2025, 3, 30, 12, 0, 0).unwrap();
        let text = "IAC001-010800-\n";
        let sections = parse_ugc_sections(text, valid_time);

        let expected = Utc.with_ymd_and_hms(2025, 4, 1, 8, 0, 0).unwrap();
        assert_eq!(sections[0].expires, expected);
    }

    #[test]
    fn parse_ugc_empty() {
        let sections = parse_ugc_sections("", test_valid_time());
        assert!(sections.is_empty());
    }

    #[test]
    fn parse_ugc_invalid_skipped() {
        let text = "INVALID-051200-\n";
        let sections = parse_ugc_sections(text, test_valid_time());
        assert!(sections.is_empty());
    }

    #[test]
    fn parse_ugc_invalid_reports_issue() {
        let text = "IAC001-991299-\n";
        let (sections, issues) = parse_ugc_sections_with_issues(text, test_valid_time());

        assert!(sections.is_empty());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "invalid_ugc_expiration");
    }

    #[test]
    fn parse_wrapped_ugc_with_shorthand_segments() {
        let text = "DCZ001-MDZ004>007-009>011-013-014-016>018-\r\nVAZ036>042-050>057-170200-\n";
        let sections = parse_ugc_sections(text, test_valid_time());

        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].codes.first().unwrap().state, "DC");
        assert_eq!(sections[0].codes.first().unwrap().number, 1);
        assert_eq!(sections[0].codes.last().unwrap().state, "VA");
        assert_eq!(sections[0].codes.last().unwrap().number, 57);
        assert_eq!(sections[0].codes.len(), 28);
    }

    #[test]
    fn parse_ugc_with_wrap_inside_segment() {
        let text = "MDZ004>0\r\n07-009>011-170200-\n";
        let sections = parse_ugc_sections(text, test_valid_time());

        assert_eq!(sections.len(), 1);
        let numbers: Vec<u16> = sections[0].codes.iter().map(|code| code.number).collect();
        assert_eq!(numbers, vec![4, 5, 6, 7, 9, 10, 11]);
    }

    #[test]
    fn parse_wrapped_ugc_with_commas_and_shorthand_segments() {
        let text = "KSZ008-009,020>022-\r\n034>040-054>056-170200-\n";
        let sections = parse_ugc_sections(text, test_valid_time());

        assert_eq!(sections.len(), 1);
        let numbers: Vec<u16> = sections[0].codes.iter().map(|code| code.number).collect();
        assert_eq!(
            numbers,
            vec![8, 9, 20, 21, 22, 34, 35, 36, 37, 38, 39, 40, 54, 55, 56]
        );
        assert!(
            sections[0]
                .codes
                .iter()
                .all(|code| code.state == "KS" && code.geoclass == UgcClass::Zone)
        );
    }

    #[test]
    fn parse_fire_weather_ugc() {
        let text = "COF214-216-170200-\n";
        let sections = parse_ugc_sections(text, test_valid_time());

        assert_eq!(sections.len(), 1);
        let numbers: Vec<u16> = sections[0].codes.iter().map(|code| code.number).collect();
        assert_eq!(numbers, vec![214, 216]);
        assert!(
            sections[0]
                .codes
                .iter()
                .all(|code| code.state == "CO" && code.geoclass == UgcClass::FireZone)
        );
    }

    #[test]
    fn parse_marine_ugc() {
        let text = "GMZ730-750-170200-\n";
        let sections = parse_ugc_sections(text, test_valid_time());

        assert_eq!(sections.len(), 1);
        let numbers: Vec<u16> = sections[0].codes.iter().map(|code| code.number).collect();
        assert_eq!(numbers, vec![730, 750]);
        assert!(
            sections[0]
                .codes
                .iter()
                .all(|code| code.state == "GM" && code.geoclass == UgcClass::Zone)
        );
    }

    #[test]
    fn parse_ugc_sentinel_expiration_reports_issue() {
        let text = "IAC001-000000-\nIAC003-123456-\n";
        let (sections, issues) = parse_ugc_sections_with_issues(text, test_valid_time());

        assert!(sections.is_empty());
        assert_eq!(issues.len(), 2);
        assert!(
            issues
                .iter()
                .all(|issue| issue.code == "invalid_ugc_expiration")
        );
    }
}
