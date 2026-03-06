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

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Utc};
use regex::Regex;

/// A parsed UGC section containing codes and expiration time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UgcSection {
    /// Individual UGC codes (expanded from ranges)
    pub codes: Vec<UgcCode>,
    /// Expiration time for this UGC section
    pub expires: DateTime<Utc>,
}

/// A single UGC code representing a county or zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UgcCode {
    /// 2-letter state code (e.g., "IA", "NE")
    pub state: String,
    /// Geographic class (County or Zone)
    pub geoclass: UgcClass,
    /// 3-digit county/zone number
    pub number: u16,
}

/// Geographic classification for UGC codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    // UGC codes appear line-by-line, each ending with 6-digit expiration + "-"
    text.lines()
        .filter_map(|line| parse_ugc_capture(line.trim(), valid_time))
        .collect()
}

/// Extract expiration code from end of UGC line
fn extract_expiration(text: &str) -> Option<(String, String)> {
    // Pattern: codes followed by 6-digit expiration and trailing dash
    let re = Regex::new(r"^([A-Z]{2}[CZFM][0-9]{3}(?:>[0-9]{3})?(?:[-,][A-Z]{2}[CZFM][0-9]{3}(?:>[0-9]{3})?)*)-([0-9]{6})-\s*$").ok()?;
    let caps = re.captures(text)?;
    Some((
        caps.get(1)?.as_str().to_string(),
        caps.get(2)?.as_str().to_string(),
    ))
}

fn parse_ugc_capture(text: &str, valid_time: DateTime<Utc>) -> Option<UgcSection> {
    // Extract codes and expiration from the full line
    let (code_block, expire_code) = extract_expiration(text)?;

    let codes = expand_ugc_block(&code_block)?;
    let expires = parse_expire_time(&expire_code, valid_time)?;

    Some(UgcSection { codes, expires })
}

fn expand_ugc_block(block: &str) -> Option<Vec<UgcCode>> {
    let mut codes = Vec::new();

    // Split on comma or hyphen (continuation)
    for segment in block.split([',', '-']) {
        if segment.is_empty() {
            continue;
        }

        let segment = segment.trim();
        if segment.len() < 6 {
            continue; // Too short to be valid
        }

        // Check for range notation (e.g., "IAC001>005")
        if let Some(gt_pos) = segment.find('>') {
            let base = &segment[..gt_pos];
            let end_num_str = &segment[gt_pos + 1..];

            if base.len() >= 6 && end_num_str.len() == 3 {
                let state = &base[..2];
                let geoclass = UgcClass::from_char(base.chars().nth(2)?);
                let start_num: u16 = base[3..6].parse().ok()?;
                let end_num: u16 = end_num_str.parse().ok()?;

                for num in start_num..=end_num {
                    codes.push(UgcCode {
                        state: state.to_string(),
                        geoclass,
                        number: num,
                    });
                }
            }
        } else {
            // Single UGC code
            if segment.len() >= 6 {
                let state = &segment[..2];
                let geoclass = UgcClass::from_char(segment.chars().nth(2)?);
                let number: u16 = segment[3..6].parse().ok()?;

                codes.push(UgcCode {
                    state: state.to_string(),
                    geoclass,
                    number,
                });
            }
        }
    }

    if codes.is_empty() { None } else { Some(codes) }
}

fn parse_expire_time(expire_code: &str, valid_time: DateTime<Utc>) -> Option<DateTime<Utc>> {
    // Expire format: DDHHMM (day of month, hour, minute)
    if expire_code.len() != 6 {
        return None;
    }

    let day: u32 = expire_code[0..2].parse().ok()?;
    let hour: u32 = expire_code[2..4].parse().ok()?;
    let minute: u32 = expire_code[4..6].parse().ok()?;

    // Determine the correct month/year based on valid_time
    let valid_day = valid_time.day();
    let year = valid_time.year();
    let month = valid_time.month();

    // Handle month/year rollover
    let (target_year, target_month) = if day < valid_day {
        // Expiration day is earlier in the month, assume next month
        if month == 12 {
            (year + 1, 1)
        } else {
            (year, month + 1)
        }
    } else {
        (year, month)
    };

    let date = NaiveDate::from_ymd_opt(target_year, target_month, day)?;
    let time = NaiveTime::from_hms_opt(hour, minute, 0)?;
    let naive = date.and_time(time);

    Some(Utc.from_utc_datetime(&naive))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
