//! Minimal GOES DCP telemetry bulletin parsing for WMO bulletins without AFOS PIL lines.
//!
//! DCP (Data Collection Platform) bulletins contain GOES (Geostationary Operational
//! Environmental Satellite) telemetry data from remote sensors such as river gauges,
//! weather stations, and seismic monitors.
//!
//! ## File Patterns
//!
//! - MISDCP*.TXT - Standard DCP telemetry
//! - MISA*.TXT - Alternate DCP format
//!
//! WMO headers for DCP: SX* ttaaii codes from KWAL (Wallops Island, VA)

use serde::Serialize;

use crate::WmoHeader;

/// GOES DCP telemetry bulletin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DcpBulletin {
    /// Platform identifier string (typically alphanumeric + numeric sequence)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_id: Option<String>,
    /// Raw telemetry data lines
    pub lines: Vec<String>,
}

/// Parses a DCP bulletin from text content.
///
/// Validates the filename pattern and WMO header, then extracts the platform
/// identifier and telemetry data lines.
///
/// # Arguments
///
/// * `filename` - Original filename (must match MISDCP*.TXT or MISA*.TXT pattern)
/// * `wmo_header` - Parsed WMO header (must have SX* ttaaii from KWAL)
/// * `text` - Raw DCP bulletin text
///
/// # Returns
///
/// `Some(DcpBulletin)` if the file appears to be valid DCP telemetry,
/// `None` if validation fails
pub(crate) fn parse_dcp_bulletin(
    filename: &str,
    wmo_header: &WmoHeader,
    text: &str,
) -> Option<DcpBulletin> {
    if !looks_like_dcp_filename(filename) || !looks_like_dcp_wmo_header(wmo_header) {
        return None;
    }

    let lines = body_lines(text);
    if lines.is_empty() || !looks_like_dcp_payload(&lines) {
        return None;
    }

    Some(DcpBulletin {
        platform_id: lines.first().and_then(|line| extract_platform_id(line)),
        lines,
    })
}

/// Checks if filename matches DCP file patterns.
///
/// Valid patterns: MISDCP*.TXT, MISA*.TXT (case-insensitive)
fn looks_like_dcp_filename(filename: &str) -> bool {
    let upper = filename.to_ascii_uppercase();
    (upper.starts_with("MISDCP") || upper.starts_with("MISA")) && upper.ends_with(".TXT")
}

/// Validates WMO header is from Wallops Island with SX* bulletin type.
fn looks_like_dcp_wmo_header(wmo_header: &WmoHeader) -> bool {
    wmo_header.cccc == "KWAL" && wmo_header.ttaaii.starts_with("SX")
}

/// Extracts non-empty lines from body text, stripping control characters.
fn body_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(strip_control_chars)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .collect()
}

/// Removes non-whitespace control characters from a line.
fn strip_control_chars(line: &str) -> String {
    line.chars()
        .filter(|ch| !ch.is_ascii_control() || ch.is_ascii_whitespace())
        .collect()
}

/// Validates the payload appears to contain DCP telemetry data.
fn looks_like_dcp_payload(lines: &[String]) -> bool {
    let first = match lines.first() {
        Some(first) => first,
        None => return false,
    };

    let Some(platform_id) = extract_platform_id(first) else {
        return false;
    };

    has_inline_telemetry(first, &platform_id)
        || lines
            .iter()
            .skip(1)
            .any(|line| looks_like_telemetry_line(line))
}

/// Extracts the platform ID from the first line of DCP data.
///
/// Platform IDs have the format: `ALPHANUMERIC<space>NUMERIC`
/// where the alphanumeric part is at least 8 chars and numeric part at least 9 digits.
fn extract_platform_id(line: &str) -> Option<String> {
    let line = line.trim_start();
    let mut chars = line.char_indices().peekable();

    let first_end = loop {
        let (idx, ch) = chars.next()?;

        if ch.is_ascii_whitespace() {
            break idx;
        }

        if !ch.is_ascii_alphanumeric() {
            return None;
        }
    };

    let first = &line[..first_end];
    if first.len() < 8 {
        return None;
    }

    let remainder = line[first_end..].trim_start();
    let digit_count = remainder
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .count();
    if digit_count < 9 {
        return None;
    }

    let second = &remainder[..digit_count];
    Some(format!("{first} {second}"))
}

/// Checks if line contains inline telemetry after the platform ID.
fn has_inline_telemetry(line: &str, platform_id: &str) -> bool {
    let remainder = line.strip_prefix(platform_id).unwrap_or(line).trim();
    !remainder.is_empty()
        && remainder
            .chars()
            .all(|ch| ch.is_ascii_graphic() || ch.is_ascii_whitespace())
        && remainder.chars().any(|ch| !ch.is_ascii_digit())
}

/// Validates a line appears to contain telemetry data (alphanumeric + basic symbols).
fn looks_like_telemetry_line(line: &str) -> bool {
    line.chars()
        .all(|ch| ch.is_ascii_graphic() || ch.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::parse_dcp_bulletin;
    use crate::WmoHeader;

    fn wmo() -> WmoHeader {
        WmoHeader {
            ttaaii: "SXMS50".to_string(),
            cccc: "KWAL".to_string(),
            ddhhmm: "070258".to_string(),
            bbb: None,
        }
    }

    #[test]
    fn parses_misdcp_bulletin() {
        let text =
            "83786162 066025814\n16.23\n003\n137\n071\n088\n12.9\n137\n007\n00000\n 42-0NN  45E\n";
        let bulletin = parse_dcp_bulletin("MISDCPSV.TXT", &wmo(), text)
            .expect("expected DCP bulletin parsing to succeed");

        assert_eq!(bulletin.platform_id.as_deref(), Some("83786162 066025814"));
        assert_eq!(bulletin.lines.len(), 11);
    }

    #[test]
    fn ignores_non_dcp_filename() {
        let text = "83786162 066025814\n16.23\n";
        assert!(parse_dcp_bulletin("mystery.txt", &wmo(), text).is_none());
    }

    #[test]
    fn parses_misa_bulletin_with_control_character_prefix() {
        let text = "\x1eD6805150 066030901 \n05.06 \n008 \n180 \n056 \n098 \n12.8 \n183 \n018 \n00000 \n 39-0NN 141E\n";
        let bulletin = parse_dcp_bulletin(
            "MISA50US.TXT",
            &WmoHeader {
                ttaaii: "SXPA50".to_string(),
                cccc: "KWAL".to_string(),
                ddhhmm: "070309".to_string(),
                bbb: None,
            },
            text,
        )
        .expect("expected MISA bulletin parsing to succeed");

        assert_eq!(bulletin.platform_id.as_deref(), Some("D6805150 066030901"));
        assert_eq!(bulletin.lines.len(), 11);
    }

    #[test]
    fn parses_single_line_misdcp_bulletin_with_inline_telemetry_noise() {
        let text = "2211F77E 066032650bB1F@VT@VT@VT@VT@VT@VT@VT@VT@VT@VT@VT@VT@Fx@Fx@Fx@Fx@Fx@Fx@Fx@Fx@Fx@Fx@Fx@Fx@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@Ta@TaJ 40+0NN  57E%\n";
        let bulletin = parse_dcp_bulletin(
            "MISDCPNI.TXT",
            &WmoHeader {
                ttaaii: "SXMN20".to_string(),
                cccc: "KWAL".to_string(),
                ddhhmm: "070326".to_string(),
                bbb: None,
            },
            text,
        )
        .expect("expected MISDCP inline telemetry bulletin parsing to succeed");

        assert_eq!(bulletin.platform_id.as_deref(), Some("2211F77E 066032650"));
        assert_eq!(bulletin.lines.len(), 1);
    }

    #[test]
    fn parses_single_line_misa_with_tilde_and_backslash_noise() {
        let text = "CE1107B6 068005524`BCT@Go@Gq@Gq@Gr@Gr@Gr@Gs@Gr@Gs@Gr@Gu@Gt~]w~\\T~^F~bF~d@~eS~gq~jl~l]~mo~sA~wyf 39+0NN  25E\n";
        let bulletin = parse_dcp_bulletin(
            "MISA50US.TXT",
            &WmoHeader {
                ttaaii: "SXPA50".to_string(),
                cccc: "KWAL".to_string(),
                ddhhmm: "090055".to_string(),
                bbb: None,
            },
            text,
        )
        .expect("expected MISA bulletin parsing to succeed");

        assert_eq!(bulletin.platform_id.as_deref(), Some("CE1107B6 068005524"));
        assert_eq!(bulletin.lines.len(), 1);
    }

    #[test]
    fn parses_single_line_misdcp_with_underscores_and_question_marks() {
        let text = "50423782 068003840bB1H_??_??_??_??_??_??_??_??@@@@@r@TaJ 47+0NN 175E\n";
        let bulletin = parse_dcp_bulletin(
            "MISDCPHN.TXT",
            &WmoHeader {
                ttaaii: "SXHN40".to_string(),
                cccc: "KWAL".to_string(),
                ddhhmm: "090038".to_string(),
                bbb: None,
            },
            text,
        )
        .expect("expected MISDCPHN bulletin parsing to succeed");

        assert_eq!(bulletin.platform_id.as_deref(), Some("50423782 068003840"));
        assert_eq!(bulletin.lines.len(), 1);
    }

    #[test]
    fn parses_single_line_misdcp_with_quoted_inline_values() {
        let text = "9650D70A 068005040\"A18.34B17.92C18.73D82.73E80.63F84.66G9.70H0.00I10.92J355.59K0.00L824.64M824.67N824.67O11.50P21.30Q0.11R-10.01S2360.16T0.00U1.20 38-0NN 397E\n";
        let bulletin = parse_dcp_bulletin(
            "MISDCPMG.TXT",
            &WmoHeader {
                ttaaii: "SXMG40".to_string(),
                cccc: "KWAL".to_string(),
                ddhhmm: "090050".to_string(),
                bbb: None,
            },
            text,
        )
        .expect("expected MISDCPMG bulletin parsing to succeed");

        assert_eq!(bulletin.platform_id.as_deref(), Some("9650D70A 068005040"));
        assert_eq!(bulletin.lines.len(), 1);
    }
}
