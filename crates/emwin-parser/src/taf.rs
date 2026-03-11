//! Minimal TAF bulletin parsing for WMO bulletins without AFOS PIL lines.
//!
//! This parser keeps the owned public `TafBulletin` output stable while
//! shifting the parsing work onto explicit preamble/core steps. The preamble is
//! parsed with `winnow` so duplicated `TAF` markers and amendment/correction
//! qualifiers are handled in one place instead of through repeated string
//! rebuilding.

use serde::Serialize;
use winnow::Parser;
use winnow::combinator::alt;
use winnow::error::ContextError;

/// TAF bulletin containing a terminal aerodrome forecast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TafBulletin {
    /// ICAO station identifier (e.g., "KBOS")
    pub station: String,
    /// Issue time in HHMMSSZ format
    pub issue_time: String,
    /// Validity period start (DDHH format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    /// Validity period end (DDHH format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    /// True if this is an amended forecast (TAF AMD)
    pub amendment: bool,
    /// True if this is a corrected forecast (TAF COR)
    pub correction: bool,
    /// Complete raw TAF text
    pub raw: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Preamble {
    amendment: bool,
    correction: bool,
}

impl Preamble {
    fn normalized_prefix(self) -> &'static str {
        match (self.amendment, self.correction) {
            (true, false) => "TAF AMD",
            (false, true) => "TAF COR",
            (false, false) => "TAF",
            (true, true) => unreachable!("TAF preamble cannot be both amended and corrected"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedTafRef<'a> {
    station: &'a str,
    issue_time: &'a str,
    valid_from: Option<&'a str>,
    valid_to: Option<&'a str>,
}

impl ParsedTafRef<'_> {
    fn into_owned(self, preamble: Preamble, raw: String) -> TafBulletin {
        TafBulletin {
            station: self.station.to_string(),
            issue_time: self.issue_time.to_string(),
            valid_from: self.valid_from.map(str::to_string),
            valid_to: self.valid_to.map(str::to_string),
            amendment: preamble.amendment,
            correction: preamble.correction,
            raw,
        }
    }
}

/// Parses a TAF bulletin from text content.
pub(crate) fn parse_taf_bulletin(text: &str) -> Option<TafBulletin> {
    let compact = compact_ascii_whitespace(text);
    let mut input = compact.as_str();
    let preamble = parse_taf_prefix(&mut input)?;
    let report_body = input;
    let parsed = parse_taf_core(&mut input)?;
    let raw = if report_body.is_empty() {
        preamble.normalized_prefix().to_string()
    } else {
        format!("{} {}", preamble.normalized_prefix(), report_body)
    };
    let owned = parsed.into_owned(preamble, raw);

    Some(owned)
}

/// Compacts ASCII whitespace in one pass.
fn compact_ascii_whitespace(text: &str) -> String {
    let mut compacted = String::with_capacity(text.len());
    let mut pending_space = false;

    for ch in text.chars() {
        if ch.is_ascii_whitespace() {
            pending_space = true;
            continue;
        }

        if pending_space && !compacted.is_empty() {
            compacted.push(' ');
        }
        pending_space = false;
        compacted.push(ch);
    }

    compacted
}

/// Parses the TAF preamble and absorbs duplicated marker patterns.
fn parse_taf_prefix(input: &mut &str) -> Option<Preamble> {
    let preamble = alt::<_, Preamble, ContextError, _>((
        "TAF TAF AMD".value(Preamble {
            amendment: true,
            correction: false,
        }),
        "TAF TAF COR".value(Preamble {
            amendment: false,
            correction: true,
        }),
        "TAF AMD TAF AMD".value(Preamble {
            amendment: true,
            correction: false,
        }),
        "TAF COR TAF COR".value(Preamble {
            amendment: false,
            correction: true,
        }),
        "TAF AMD TAF".value(Preamble {
            amendment: true,
            correction: false,
        }),
        "TAF COR TAF".value(Preamble {
            amendment: false,
            correction: true,
        }),
        "TAF TAF".value(Preamble {
            amendment: false,
            correction: false,
        }),
        "TAF AMD".value(Preamble {
            amendment: true,
            correction: false,
        }),
        "TAF COR".value(Preamble {
            amendment: false,
            correction: true,
        }),
        "TAF".value(Preamble {
            amendment: false,
            correction: false,
        }),
    ))
    .parse_next(input)
    .ok()?;

    if input.starts_with(' ') {
        *input = &input[1..];
    }

    Some(preamble)
}

/// Parses the station, issue time, and optional validity window from compacted text.
fn parse_taf_core<'a>(input: &mut &'a str) -> Option<ParsedTafRef<'a>> {
    let station = next_token(input)?;
    let issue_time = next_token(input)?;
    if !is_station_token(station) || !is_issue_time_token(issue_time) {
        return None;
    }

    let validity = input
        .split_once(' ')
        .map(|(candidate, _)| candidate)
        .or((!input.is_empty()).then_some(*input))
        .and_then(parse_validity_range);

    if let Some((valid_from, valid_to)) = validity {
        let consumed = valid_from.len() + valid_to.len() + 1;
        *input = input.get(consumed..).unwrap_or_default();
        if input.starts_with(' ') {
            *input = &input[1..];
        }

        return Some(ParsedTafRef {
            station,
            issue_time,
            valid_from: Some(valid_from),
            valid_to: Some(valid_to),
        });
    }

    Some(ParsedTafRef {
        station,
        issue_time,
        valid_from: None,
        valid_to: None,
    })
}

fn next_token<'a>(input: &mut &'a str) -> Option<&'a str> {
    if input.is_empty() {
        return None;
    }

    if let Some((token, rest)) = input.split_once(' ') {
        *input = rest;
        Some(token)
    } else {
        let token = *input;
        *input = "";
        Some(token)
    }
}

fn is_station_token(token: &str) -> bool {
    (3..=4).contains(&token.len()) && token.chars().all(|ch| ch.is_ascii_alphanumeric())
}

fn is_issue_time_token(token: &str) -> bool {
    token.len() == 7 && token.ends_with('Z') && token[..6].chars().all(|ch| ch.is_ascii_digit())
}

fn parse_validity_range(token: &str) -> Option<(&str, &str)> {
    let (valid_from, valid_to) = token.split_once('/')?;
    (valid_from.len() == 4
        && valid_to.len() == 4
        && valid_from.chars().all(|ch| ch.is_ascii_digit())
        && valid_to.chars().all(|ch| ch.is_ascii_digit()))
    .then_some((valid_from, valid_to))
}

#[cfg(test)]
mod tests {
    use super::parse_taf_bulletin;

    #[test]
    fn parses_amended_taf_bulletin() {
        let text = "TAF AMD\nWBCF 070244Z 0703/0803 18012KT P6SM SCT050\n";
        let taf = parse_taf_bulletin(text).expect("expected TAF bulletin parsing to succeed");

        assert_eq!(taf.station, "WBCF");
        assert_eq!(taf.issue_time, "070244Z");
        assert_eq!(taf.valid_from.as_deref(), Some("0703"));
        assert_eq!(taf.valid_to.as_deref(), Some("0803"));
        assert!(taf.amendment);
        assert!(!taf.correction);
    }

    #[test]
    fn parses_corrected_taf_bulletin() {
        let text = "TAF COR KBOS 090520Z 0906/1012 28012KT P6SM FEW250\n";
        let taf = parse_taf_bulletin(text).expect("expected TAF COR parsing to succeed");

        assert_eq!(taf.station, "KBOS");
        assert_eq!(taf.issue_time, "090520Z");
        assert!(taf.correction);
        assert!(!taf.amendment);
    }

    #[test]
    fn parses_bulletin_with_marker_line_before_taf_report() {
        let text = "TAF\nTAF SVJC 070400Z 0706/0806 07005KT 9999 FEW013 TX33/0718Z\n      TN23/0708Z\n      TEMPO 0706/0710 08004KT CAVOK\n     FM071100 09006KT 9999 FEW013=\n";
        let taf = parse_taf_bulletin(text).expect("expected TAF bulletin parsing to succeed");

        assert_eq!(taf.station, "SVJC");
        assert_eq!(taf.issue_time, "070400Z");
        assert_eq!(taf.valid_from.as_deref(), Some("0706"));
        assert_eq!(taf.valid_to.as_deref(), Some("0806"));
        assert!(!taf.amendment);
        assert!(!taf.correction);
        assert!(taf.raw.starts_with("TAF SVJC 070400Z"));
    }

    #[test]
    fn ignores_non_taf_body() {
        let text = "000 \nSAGL31 BGGH 070200\nMETAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n";
        assert!(parse_taf_bulletin(text).is_none());
    }

    #[test]
    fn parses_duplicated_taf_prefix() {
        let text = "TAF\nTAF KDSM 090520Z 0906/1012 28012KT P6SM FEW250\n";
        let taf = parse_taf_bulletin(text).expect("expected duplicated TAF parsing to succeed");

        assert_eq!(taf.station, "KDSM");
        assert_eq!(taf.issue_time, "090520Z");
        assert!(!taf.correction);
        assert!(!taf.amendment);
        assert!(taf.raw.starts_with("TAF KDSM 090520Z"));
    }

    #[test]
    fn parses_duplicated_amended_taf_prefix() {
        let text = "TAF AMD\nTAF AMD MMAS 090101Z 0901/0918 23008KT P6SM SCT100 BKN200\n";
        let taf = parse_taf_bulletin(text).expect("expected duplicated TAF AMD parsing to succeed");

        assert_eq!(taf.station, "MMAS");
        assert_eq!(taf.issue_time, "090101Z");
        assert_eq!(taf.valid_from.as_deref(), Some("0901"));
        assert_eq!(taf.valid_to.as_deref(), Some("0918"));
        assert!(taf.amendment);
        assert!(!taf.correction);
        assert!(taf.raw.starts_with("TAF AMD MMAS 090101Z"));
    }

    #[test]
    fn parses_duplicated_corrected_taf_prefix() {
        let text = "TAF COR\nTAF COR KBOS 090520Z 0906/1012 28012KT P6SM FEW250\n";
        let taf = parse_taf_bulletin(text).expect("expected duplicated TAF COR parsing to succeed");

        assert_eq!(taf.station, "KBOS");
        assert_eq!(taf.issue_time, "090520Z");
        assert!(taf.correction);
        assert!(!taf.amendment);
        assert!(taf.raw.starts_with("TAF COR KBOS 090520Z"));
    }

    #[test]
    fn parses_marker_line_then_corrected_taf_prefix() {
        let text = "TAF\nTAF COR KBOS 090520Z 0906/1012 28012KT P6SM FEW250\n";
        let taf = parse_taf_bulletin(text).expect("expected marker line followed by TAF COR");

        assert_eq!(taf.station, "KBOS");
        assert!(taf.correction);
        assert!(!taf.amendment);
        assert!(taf.raw.starts_with("TAF COR KBOS 090520Z"));
    }

    #[test]
    fn parses_taf_without_validity_range() {
        let text = "TAF KMEM 090520Z 28012KT P6SM FEW250\n";
        let taf = parse_taf_bulletin(text).expect("expected TAF without validity range");

        assert_eq!(taf.station, "KMEM");
        assert_eq!(taf.valid_from, None);
        assert_eq!(taf.valid_to, None);
    }
}
