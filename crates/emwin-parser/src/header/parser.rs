use crate::time::resolve_day_time_nearest;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;
use thiserror::Error;

/// Parsed WMO/AFOS text product header.
///
/// Contains the standard WMO header fields plus the AFOS Product Identifier Line (PIL).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextProductHeader {
    /// WMO product type indicator (6 characters, normalized from 4 to "00")
    pub ttaaii: String,
    /// 4-letter ICAO station code
    pub cccc: String,
    /// Day and time (UTC) in DDHHMM format
    pub ddhhmm: String,
    /// Optional BBB indicator (CORrection, AMEndment, RR, etc.)
    pub bbb: Option<String>,
    /// Product Identifier Line (PIL), typically 6 characters
    pub afos: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedTextProduct {
    pub(crate) header: TextProductHeader,
    pub(crate) conditioned_text: String,
}

impl TextProductHeader {
    /// Parse the ddhhmm field into a UTC DateTime.
    ///
    /// Uses the provided reference time to determine the nearest plausible month and year.
    ///
    /// # Arguments
    ///
    /// * `reference_time` - Reference UTC time (typically current time)
    ///
    /// # Returns
    ///
    /// `Some(DateTime<Utc>)` if ddhhmm is valid, `None` otherwise
    pub fn timestamp(&self, reference_time: DateTime<Utc>) -> Option<DateTime<Utc>> {
        if self.ddhhmm.len() != 6 {
            return None;
        }

        let day: u32 = self.ddhhmm[0..2].parse().ok()?;
        let hour: u32 = self.ddhhmm[2..4].parse().ok()?;
        let minute: u32 = self.ddhhmm[4..6].parse().ok()?;

        // Validate ranges
        if day == 0 || day > 31 || hour > 23 || minute > 59 {
            return None;
        }

        resolve_day_time_nearest(reference_time, day, hour, minute)
    }
}

/// Errors that can occur during text product parsing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParserError {
    #[error("text payload is empty after conditioning")]
    EmptyInput,
    #[error("conditioned text does not contain a WMO header line")]
    MissingWmoLine,
    #[error("could not parse WMO header line: `{line}`")]
    InvalidWmoHeader { line: String },
    #[error("conditioned text does not contain an AFOS line")]
    MissingAfosLine,
    #[error("could not parse AFOS PIL from line: `{line}`")]
    MissingAfos { line: String },
}

/// Parses a WMO/AFOS text product header from raw bytes.
///
/// This function performs text conditioning (SOH/ETX stripping, null byte removal, LDM insertion)
/// before parsing the WMO header and AFOS PIL.
///
/// # Arguments
///
/// * `bytes` - Raw product text as bytes
///
/// # Returns
///
/// A `Result` containing the parsed [`TextProductHeader`] or a [`ParserError`]
///
/// # Errors
///
/// Returns an error if:
/// - Text is empty after conditioning
/// - No WMO header line is found
/// - WMO header format is invalid
/// - No AFOS line is found
/// - AFOS PIL cannot be parsed
///
/// # Example
///
/// ```
/// use emwin_parser::parse_text_product;
///
/// let raw_text = b"000 \nFXUS61 KBOX 022101\nAFDBOX\nAREA FORECAST DISCUSSION\n";
/// let header = parse_text_product(raw_text)?;
///
/// assert_eq!(header.afos, "AFDBOX");
/// assert_eq!(header.cccc, "KBOX");
/// assert_eq!(header.ttaaii, "FXUS61");
/// # Ok::<(), emwin_parser::ParserError>(())
/// ```
pub fn parse_text_product(bytes: &[u8]) -> Result<TextProductHeader, ParserError> {
    Ok(parse_text_product_conditioned(bytes)?.header)
}

pub(crate) fn parse_text_product_conditioned(
    bytes: &[u8],
) -> Result<ParsedTextProduct, ParserError> {
    let raw = String::from_utf8_lossy(bytes).replace('\0', "");
    let conditioned = condition_text(&raw)?;
    let (ttaaii, cccc, ddhhmm, bbb) = parse_wmo(&conditioned)?;
    let afos = parse_afos(&conditioned)?;

    Ok(ParsedTextProduct {
        header: TextProductHeader {
            ttaaii,
            cccc,
            ddhhmm,
            bbb,
            afos,
        },
        conditioned_text: conditioned,
    })
}

fn parse_wmo(text: &str) -> Result<(String, String, String, Option<String>), ParserError> {
    let search_window = text.get(..100).unwrap_or(text);
    let captures =
        wmo_re()
            .captures(search_window)
            .ok_or_else(|| ParserError::InvalidWmoHeader {
                line: text.lines().nth(1).unwrap_or_default().to_string(),
            })?;

    let mut ttaaii = captures
        .name("ttaaii")
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    if ttaaii.len() == 4 {
        ttaaii.push_str("00");
    }

    let cccc = captures
        .name("cccc")
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let ddhhmm = captures
        .name("ddhhmm")
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let bbb = captures.name("bbb").map(|m| m.as_str().to_string());

    Ok((ttaaii, cccc, ddhhmm, bbb))
}

fn parse_afos(text: &str) -> Result<String, ParserError> {
    let line3 = text.lines().nth(2).ok_or(ParserError::MissingAfosLine)?;
    let captures = afos_re()
        .captures(line3)
        .ok_or_else(|| ParserError::MissingAfos {
            line: line3.to_string(),
        })?;
    let afos = captures
        .get(1)
        .map(|m| m.as_str().trim().to_string())
        .ok_or_else(|| ParserError::MissingAfos {
            line: line3.to_string(),
        })?;
    Ok(afos)
}

fn condition_text(input: &str) -> Result<String, ParserError> {
    let mut text = input.replace('\r', "").trim().to_string();
    if text.is_empty() {
        return Err(ParserError::EmptyInput);
    }

    if text.starts_with('\u{1}') {
        text = text
            .split_once('\n')
            .map(|(_, rest)| rest.to_string())
            .unwrap_or_default();
    }

    if !ldm_sequence_re().is_match(&text) {
        text = format!("000 \n{text}");
    }

    let line2 = text.lines().nth(1).ok_or(ParserError::MissingWmoLine)?;
    if !wmo_re().is_match(line2) {
        return Err(ParserError::InvalidWmoHeader {
            line: line2.to_string(),
        });
    }

    if text.ends_with('\u{3}') {
        text.pop();
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }

    Ok(text)
}

fn ldm_sequence_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d\d\d\s?").expect("ldm sequence regex compiles"))
}

fn wmo_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?m)^(?P<ttaaii>[A-Z0-9]{4,6}) (?P<cccc>[A-Z]{4}) (?P<ddhhmm>[0-3][0-9][0-2][0-9][0-5][0-9])\s*(?P<bbb>[ACR][ACMORT][A-Z])?\s*$",
        )
        .expect("wmo regex compiles")
    })
}

fn afos_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^([A-Z0-9]{4,6})\s*\t*$").expect("afos regex compiles"))
}

#[cfg(test)]
mod tests {
    use super::{ParserError, parse_text_product};
    use chrono::{TimeZone, Utc};

    fn fixture(wmo_line: &str, afos_line: &str, body: &str) -> String {
        format!("000 \n{wmo_line}\n{afos_line}\n{body}\n")
    }

    #[test]
    fn wmo_header_variations_parse() {
        let cases = [
            ("FTUS43 KOAX 102320    ", None),
            ("FTUS43 KOAX 102320  COR ", Some("COR")),
            ("FTUS43 KOAX 102320  COR  ", Some("COR")),
            ("FTUS43 KOAX 102320", None),
        ];

        for (wmo, expected_bbb) in cases {
            let text = fixture(wmo, "AFDOAX", "...body...");
            let parsed = parse_text_product(text.as_bytes()).expect("wmo should parse");
            assert_eq!(parsed.ttaaii, "FTUS43");
            assert_eq!(parsed.cccc, "KOAX");
            assert_eq!(parsed.ddhhmm, "102320");
            assert_eq!(parsed.bbb.as_deref(), expected_bbb);
            assert_eq!(parsed.afos, "AFDOAX");
        }
    }

    #[test]
    fn afos_and_wmo_parse_success() {
        let text = fixture("FXUS61 KBOX 022101", "AFDBOX", "AREA FORECAST DISCUSSION");
        let parsed = parse_text_product(text.as_bytes()).expect("header should parse");

        assert_eq!(parsed.afos, "AFDBOX");
        assert_eq!(parsed.cccc, "KBOX");
        assert_eq!(parsed.ttaaii, "FXUS61");
    }

    #[test]
    fn missing_afos_returns_error() {
        let text = fixture("FXUS61 KBOX 022101", "   ", "body");
        let err = parse_text_product(text.as_bytes()).expect_err("missing afos should fail");
        assert!(matches!(err, ParserError::MissingAfos { .. }));
    }

    #[test]
    fn afos_with_numeric_prefix_is_valid() {
        let text = fixture("WHUS74 KBRO 010000", "3MWBRO", "body");
        let parsed = parse_text_product(text.as_bytes()).expect("numeric afos should parse");
        assert_eq!(parsed.afos, "3MWBRO");
    }

    #[test]
    fn afos_with_trailing_tab_is_valid() {
        let text = fixture("WUUS56 PGUM 240602", "FFWGUM\t", "body");
        let parsed = parse_text_product(text.as_bytes()).expect("afos with tab should parse");
        assert_eq!(parsed.afos, "FFWGUM");
    }

    #[test]
    fn null_bytes_are_removed_before_parse() {
        let text = fixture("FXUS61 KBOX 022101", "AFD\0BOX", "body");
        let parsed = parse_text_product(text.as_bytes()).expect("null bytes should be removed");
        assert_eq!(parsed.afos, "AFDBOX");
    }

    #[test]
    fn correction_flag_in_wmo_does_not_break_parse() {
        let text = fixture("NOUS42 KDMX 041442 COR", "MANANN", "body");
        let parsed = parse_text_product(text.as_bytes()).expect("correction should parse");
        assert_eq!(parsed.bbb.as_deref(), Some("COR"));
        assert_eq!(parsed.afos, "MANANN");
    }

    #[test]
    fn missing_ldm_sequence_is_inserted() {
        let text = "FXUS61 KBOX 022101\nAFDBOX\nbody\n";
        let parsed = parse_text_product(text.as_bytes()).expect("missing ldm sequence handled");
        assert_eq!(parsed.ttaaii, "FXUS61");
        assert_eq!(parsed.afos, "AFDBOX");
    }

    #[test]
    fn strips_soh_and_etx_before_parse() {
        let text = "\u{1}123\n000 \nFXUS61 KBOX 022101\nAFDBOX\nbody\u{3}";
        let parsed = parse_text_product(text.as_bytes()).expect("soh/etx should be stripped");
        assert_eq!(parsed.afos, "AFDBOX");
    }

    #[test]
    fn four_character_ttaaii_is_normalized() {
        let text = fixture("ABCD KBOX 022101", "AFDBOX", "body");
        let parsed = parse_text_product(text.as_bytes()).expect("ttaaii should normalize");
        assert_eq!(parsed.ttaaii, "ABCD00");
    }

    #[test]
    fn malformed_wmo_is_strict_error() {
        let text = fixture("FXUS61 KBOX 229999", "AFDBOX", "body");
        let err = parse_text_product(text.as_bytes()).expect_err("invalid wmo should fail");
        assert!(matches!(err, ParserError::InvalidWmoHeader { .. }));
    }

    #[test]
    fn timestamp_uses_current_month_when_closest() {
        let text = fixture("FXUS61 KBOX 051200", "AFDBOX", "body");
        let parsed = parse_text_product(text.as_bytes()).expect("header should parse");
        let reference = Utc.with_ymd_and_hms(2025, 3, 6, 0, 0, 0).unwrap();

        let timestamp = parsed
            .timestamp(reference)
            .expect("timestamp should resolve");

        assert_eq!(
            timestamp,
            Utc.with_ymd_and_hms(2025, 3, 5, 12, 0, 0).unwrap()
        );
    }

    #[test]
    fn timestamp_rolls_back_to_previous_month_when_closest() {
        let text = fixture("FXUS61 KBOX 281200", "AFDBOX", "body");
        let parsed = parse_text_product(text.as_bytes()).expect("header should parse");
        let reference = Utc.with_ymd_and_hms(2025, 3, 1, 0, 0, 0).unwrap();

        let timestamp = parsed
            .timestamp(reference)
            .expect("timestamp should resolve");

        assert_eq!(
            timestamp,
            Utc.with_ymd_and_hms(2025, 2, 28, 12, 0, 0).unwrap()
        );
    }
}
