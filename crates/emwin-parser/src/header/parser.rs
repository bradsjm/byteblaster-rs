//! WMO/AFOS header parsing implementation.
//!
//! This module implements the parsing logic for WMO (World Meteorological Organization)
//! headers and AFOS (Automation of Field Operations and Services) Product Identifier Lines (PILs).
//!
//! ## Header Format
//!
//! NWS text products follow this structure:
//! - Line 1: LDM sequence number (e.g., "000 ")
//! - Line 2: WMO header (ttaaii cccc ddhhmm [bbb])
//! - Line 3: AFOS PIL (Product Identifier Line)
//! - Body: Product content
//!
//! Example:
//! ```text
//! 000
//! FXUS61 KBOX 022101
//! AFDBOX
//! AREA FORECAST DISCUSSION
//! ```

use crate::time::resolve_day_time_nearest;
use bstr::ByteSlice;
use chrono::{DateTime, Utc};
use memchr::memchr_iter;
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;
use thiserror::Error;

/// Parsed WMO/AFOS text product header.
///
/// Contains the standard WMO header fields plus the AFOS Product Identifier Line (PIL).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WmoHeader {
    /// WMO product type indicator (6 characters, normalized from 4 to "00")
    pub ttaaii: String,
    /// 4-letter ICAO station code
    pub cccc: String,
    /// Day and time (UTC) in DDHHMM format
    pub ddhhmm: String,
    /// Optional BBB indicator (CORrection, AMEndment, RR, etc.)
    pub bbb: Option<String>,
}

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

/// Borrowed WMO header view over conditioned bulletin text.
///
/// The borrowed form keeps header parsing on the shared backing buffer and only
/// materializes owned strings at the public API boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WmoHeaderRef<'a> {
    pub(crate) ttaaii: &'a str,
    pub(crate) cccc: &'a str,
    pub(crate) ddhhmm: &'a str,
    pub(crate) bbb: Option<&'a str>,
}

/// Borrowed AFOS text-product header view over conditioned bulletin text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextProductHeaderRef<'a> {
    pub(crate) ttaaii: &'a str,
    pub(crate) cccc: &'a str,
    pub(crate) ddhhmm: &'a str,
    pub(crate) bbb: Option<&'a str>,
    pub(crate) afos: &'a str,
}

/// Borrowed parsed AFOS text product view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParsedTextProductRef<'a> {
    pub(crate) header: TextProductHeaderRef<'a>,
    pub(crate) conditioned_text: &'a str,
    pub(crate) body_text: &'a str,
}

/// Borrowed parsed WMO bulletin view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParsedWmoBulletinRef<'a> {
    pub(crate) header: WmoHeaderRef<'a>,
    pub(crate) conditioned_text: &'a str,
    pub(crate) body_text: &'a str,
}

impl WmoHeaderRef<'_> {
    /// Converts the borrowed view into the stable owned public header type.
    pub(crate) fn to_owned(self) -> WmoHeader {
        WmoHeader {
            ttaaii: normalize_ttaaii(self.ttaaii),
            cccc: self.cccc.to_string(),
            ddhhmm: self.ddhhmm.to_string(),
            bbb: self.bbb.map(str::to_string),
        }
    }

    /// Resolves the WMO day/time fields against a reference time.
    pub(crate) fn timestamp(&self, reference_time: DateTime<Utc>) -> Option<DateTime<Utc>> {
        parse_timestamp_fields(self.ddhhmm, reference_time)
    }
}

impl TextProductHeaderRef<'_> {
    /// Converts the borrowed view into the stable owned public header type.
    pub(crate) fn to_owned(self) -> TextProductHeader {
        TextProductHeader {
            ttaaii: normalize_ttaaii(self.ttaaii),
            cccc: self.cccc.to_string(),
            ddhhmm: self.ddhhmm.to_string(),
            bbb: self.bbb.map(str::to_string),
            afos: self.afos.to_string(),
        }
    }

    /// Resolves the WMO day/time fields against a reference time.
    pub(crate) fn timestamp(&self, reference_time: DateTime<Utc>) -> Option<DateTime<Utc>> {
        WmoHeaderRef {
            ttaaii: self.ttaaii,
            cccc: self.cccc,
            ddhhmm: self.ddhhmm,
            bbb: self.bbb,
        }
        .timestamp(reference_time)
    }
}

impl WmoHeader {
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
        WmoHeaderRef {
            ttaaii: &self.ttaaii,
            cccc: &self.cccc,
            ddhhmm: &self.ddhhmm,
            bbb: self.bbb.as_deref(),
        }
        .timestamp(reference_time)
    }
}

impl TextProductHeader {
    /// Parse the ddhhmm field into a UTC DateTime.
    ///
    /// Uses the provided reference time to determine the nearest plausible month and year.
    pub fn timestamp(&self, reference_time: DateTime<Utc>) -> Option<DateTime<Utc>> {
        TextProductHeaderRef {
            ttaaii: &self.ttaaii,
            cccc: &self.cccc,
            ddhhmm: &self.ddhhmm,
            bbb: self.bbb.as_deref(),
            afos: &self.afos,
        }
        .timestamp(reference_time)
    }
}

/// Errors that can occur during text product parsing.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
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
    let conditioned = condition_text_bytes(bytes)?;
    Ok(parse_text_product_conditioned_ref(&conditioned)?
        .header
        .to_owned())
}

/// Conditions raw bytes into a parseable bulletin text buffer.
pub(crate) fn condition_text_bytes(bytes: &[u8]) -> Result<String, ParserError> {
    let raw = bytes.to_str_lossy();
    condition_text(raw.as_ref())
}

/// Parses conditioned AFOS bulletin text into borrowed header and body views.
pub(crate) fn parse_text_product_conditioned_ref(
    text: &str,
) -> Result<ParsedTextProductRef<'_>, ParserError> {
    let parsed = parse_wmo_bulletin_conditioned_ref(text)?;
    let afos = parse_afos(text)?;
    let body_text = body_after_lines(text, 3);

    Ok(ParsedTextProductRef {
        header: TextProductHeaderRef {
            ttaaii: parsed.header.ttaaii,
            cccc: parsed.header.cccc,
            ddhhmm: parsed.header.ddhhmm,
            bbb: parsed.header.bbb,
            afos,
        },
        conditioned_text: text,
        body_text,
    })
}

/// Parses conditioned WMO bulletin text into borrowed header and body views.
pub(crate) fn parse_wmo_bulletin_conditioned_ref(
    text: &str,
) -> Result<ParsedWmoBulletinRef<'_>, ParserError> {
    let header = parse_wmo(text)?;
    let body_text = body_after_lines(text, 2);

    Ok(ParsedWmoBulletinRef {
        header,
        conditioned_text: text,
        body_text,
    })
}

/// Extracts body text as a subslice by skipping the first N lines.
fn body_after_lines(text: &str, lines_to_skip: usize) -> &str {
    let offset = offset_after_n_lines(text, lines_to_skip).unwrap_or(text.len());
    &text[offset..]
}

/// Locates the byte offset immediately after `lines_to_skip` newline-delimited lines.
fn offset_after_n_lines(text: &str, lines_to_skip: usize) -> Option<usize> {
    if lines_to_skip == 0 {
        return Some(0);
    }

    let mut lines_seen = 0;
    for newline in memchr_iter(b'\n', text.as_bytes()) {
        lines_seen += 1;
        if lines_seen == lines_to_skip {
            return Some(newline + 1);
        }
    }

    Some(text.len())
}

/// Parses WMO header fields from text.
///
/// Extracts ttaaii, cccc, ddhhmm, and optional BBB indicator.
fn parse_wmo(text: &str) -> Result<WmoHeaderRef<'_>, ParserError> {
    let search_window = text.get(..100).unwrap_or(text);
    let captures =
        wmo_re()
            .captures(search_window)
            .ok_or_else(|| ParserError::InvalidWmoHeader {
                line: nth_line(text, 1).unwrap_or_default().to_string(),
            })?;

    Ok(WmoHeaderRef {
        ttaaii: captures
            .name("ttaaii")
            .map(|m| m.as_str())
            .unwrap_or_default(),
        cccc: captures
            .name("cccc")
            .map(|m| m.as_str())
            .unwrap_or_default(),
        ddhhmm: captures
            .name("ddhhmm")
            .map(|m| m.as_str())
            .unwrap_or_default(),
        bbb: captures.name("bbb").map(|m| m.as_str()),
    })
}

/// Parses AFOS PIL from line 3 of the conditioned text.
fn parse_afos(text: &str) -> Result<&str, ParserError> {
    let line3 = nth_line(text, 2).ok_or(ParserError::MissingAfosLine)?;
    let captures = afos_re()
        .captures(line3)
        .ok_or_else(|| ParserError::MissingAfos {
            line: line3.to_string(),
        })?;
    let afos =
        captures
            .get(1)
            .map(|m| m.as_str().trim())
            .ok_or_else(|| ParserError::MissingAfos {
                line: line3.to_string(),
            })?;
    Ok(afos)
}

/// Conditions raw text for parsing by normalizing control characters and headers.
///
/// Performs the following transformations:
/// - Removes carriage returns and null bytes
/// - Strips SOH (Start of Header) character and preceding content
/// - Strips ETX (End of Text) character
/// - Adds LDM sequence line if missing
/// - Ensures trailing newline
fn condition_text(input: &str) -> Result<String, ParserError> {
    let mut sanitized = String::with_capacity(input.len() + 5);
    for ch in input.chars() {
        if ch != '\r' && ch != '\0' {
            sanitized.push(ch);
        }
    }

    let mut text = sanitized.trim();
    if text.is_empty() {
        return Err(ParserError::EmptyInput);
    }

    if text.starts_with('\u{1}') {
        text = text
            .split_once('\n')
            .map(|(_, rest)| rest)
            .unwrap_or_default();
    }

    if text.ends_with('\u{3}') {
        text = &text[..text.len() - '\u{3}'.len_utf8()];
    }

    let needs_ldm = !ldm_sequence_re().is_match(text);
    let mut conditioned = String::with_capacity(text.len() + usize::from(needs_ldm) * 5 + 1);
    if needs_ldm {
        conditioned.push_str("000 \n");
    }
    conditioned.push_str(text);

    let line2 = nth_line(&conditioned, 1).ok_or(ParserError::MissingWmoLine)?;
    if !wmo_re().is_match(line2) {
        return Err(ParserError::InvalidWmoHeader {
            line: line2.to_string(),
        });
    }

    if !conditioned.ends_with('\n') {
        conditioned.push('\n');
    }

    Ok(conditioned)
}

fn parse_timestamp_fields(ddhhmm: &str, reference_time: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if ddhhmm.len() != 6 {
        return None;
    }

    let day: u32 = ddhhmm[0..2].parse().ok()?;
    let hour: u32 = ddhhmm[2..4].parse().ok()?;
    let minute: u32 = ddhhmm[4..6].parse().ok()?;

    if day == 0 || day > 31 || hour > 23 || minute > 59 {
        return None;
    }

    resolve_day_time_nearest(reference_time, day, hour, minute)
}

fn normalize_ttaaii(ttaaii: &str) -> String {
    if ttaaii.len() == 4 {
        let mut normalized = String::with_capacity(6);
        normalized.push_str(ttaaii);
        normalized.push_str("00");
        normalized
    } else {
        ttaaii.to_string()
    }
}

fn nth_line(text: &str, index: usize) -> Option<&str> {
    text.lines().nth(index)
}

/// Regex for LDM sequence line detection.
fn ldm_sequence_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d\d\d\s?").expect("ldm sequence regex compiles"))
}

/// Regex for WMO header parsing (line 2).
fn wmo_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?m)^(?P<ttaaii>[A-Z0-9]{4,6}) (?P<cccc>[A-Z]{4}) (?P<ddhhmm>[0-3][0-9][0-2][0-9][0-5][0-9])\s*(?P<bbb>[ACR][ACMORT][A-Z])?\s*$",
        )
        .expect("wmo regex compiles")
    })
}

/// Regex for AFOS PIL parsing (line 3).
fn afos_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^([A-Z0-9]{4,6})\s*\t*$").expect("afos regex compiles"))
}

#[cfg(test)]
mod tests {
    use super::{
        ParserError, TextProductHeaderRef, WmoHeaderRef, body_after_lines, condition_text,
        parse_text_product, parse_text_product_conditioned_ref, parse_wmo_bulletin_conditioned_ref,
    };
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

    #[test]
    fn body_after_lines_returns_borrowed_body_slice() {
        let text = "000 \nFXUS61 KBOX 022101\nAFDBOX\nBODY\nSECOND\n";

        assert_eq!(body_after_lines(text, 3), "BODY\nSECOND\n");
        assert_eq!(body_after_lines(text, 10), "");
    }

    #[test]
    fn borrowed_header_refs_convert_to_owned_headers() {
        let wmo = WmoHeaderRef {
            ttaaii: "ABCD",
            cccc: "KBOX",
            ddhhmm: "022101",
            bbb: Some("COR"),
        };
        let text = TextProductHeaderRef {
            ttaaii: "ABCD",
            cccc: "KBOX",
            ddhhmm: "022101",
            bbb: Some("COR"),
            afos: "AFDBOX",
        };

        assert_eq!(wmo.to_owned().ttaaii, "ABCD00");
        assert_eq!(text.to_owned().ttaaii, "ABCD00");
        assert_eq!(text.to_owned().afos, "AFDBOX");
    }

    #[test]
    fn borrowed_timestamp_resolution_matches_owned_behavior() {
        let reference = Utc.with_ymd_and_hms(2025, 3, 6, 0, 0, 0).unwrap();
        let wmo = WmoHeaderRef {
            ttaaii: "FXUS61",
            cccc: "KBOX",
            ddhhmm: "051200",
            bbb: None,
        };
        let text = TextProductHeaderRef {
            ttaaii: "FXUS61",
            cccc: "KBOX",
            ddhhmm: "051200",
            bbb: None,
            afos: "AFDBOX",
        };

        assert_eq!(
            wmo.timestamp(reference),
            parse_text_product(fixture("FXUS61 KBOX 051200", "AFDBOX", "body").as_bytes())
                .expect("header should parse")
                .timestamp(reference)
        );
        assert_eq!(
            text.timestamp(reference),
            parse_text_product(fixture("FXUS61 KBOX 051200", "AFDBOX", "body").as_bytes())
                .expect("header should parse")
                .timestamp(reference)
        );
    }

    #[test]
    fn condition_text_strips_controls_and_inserts_ldm() {
        let conditioned =
            condition_text("\u{1}ignore me\nFXUS61 KBOX 022101\nAFDBOX\nbody\r\n\u{3}\0")
                .expect("conditioning should succeed");

        assert_eq!(conditioned, "000 \nFXUS61 KBOX 022101\nAFDBOX\nbody\n");
    }

    #[test]
    fn conditioned_ref_parsers_preserve_error_shapes() {
        let invalid = condition_text("000 \nINVALID HEADER\nAFDBOX\nbody\n")
            .expect_err("invalid WMO should fail conditioning");
        assert!(matches!(invalid, ParserError::InvalidWmoHeader { .. }));

        let missing_afos = parse_text_product_conditioned_ref("000 \nFXUS61 KBOX 022101\n")
            .expect_err("missing afos should fail");
        assert!(matches!(missing_afos, ParserError::MissingAfosLine));

        let parsed_wmo = parse_wmo_bulletin_conditioned_ref(
            "000 \nSAGL31 BGGH 070200\nMETAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n",
        )
        .expect("wmo bulletin should parse");
        assert_eq!(
            parsed_wmo.body_text,
            "METAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n"
        );
    }
}
