//! Minimal SIGMET bulletin parsing.
//!
//! This parser keeps the current public `SigmetBulletin` output while replacing
//! the section split and header parse path with explicit, lower-allocation
//! steps. Convective sections stay line-oriented, while oceanic headers use a
//! small `winnow` parser for the fixed `<CCCC> SIGMET ... VALID ...` prelude.

use std::borrow::Cow;

use serde::Serialize;
use winnow::Parser;
use winnow::error::ContextError;
use winnow::token::take_while;

/// SIGMET bulletin containing multiple SIGMET sections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SigmetBulletin {
    /// Individual SIGMET sections in the bulletin
    pub sections: Vec<SigmetSection>,
}

/// Individual SIGMET section with parsed hazard information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SigmetSection {
    /// Series identifier (e.g., "convective", "SIERRA", "TANGO")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    /// SIGMET alphanumeric identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    /// Hazard type description (e.g., "EMBD TS" for embedded thunderstorms)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hazard: Option<String>,
    /// Valid from time (DDHHMM format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    /// Valid until time (DDHHMM format or HHMMZ)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    /// Originating weather office (e.g., "KZAK")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// Raw states/areas text (convective SIGMETs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub states_raw: Option<String>,
    /// Raw location fixes (e.g., "FROM 10NW EWC-40NNE HNN")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixes_raw: Option<String>,
    /// Complete raw text of this SIGMET section
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedSigmetSectionRef<'a> {
    series: Option<Cow<'a, str>>,
    identifier: Option<Cow<'a, str>>,
    hazard: Option<&'static str>,
    valid_from: Option<Cow<'a, str>>,
    valid_to: Option<Cow<'a, str>>,
    origin: Option<Cow<'a, str>>,
    states_raw: Option<Cow<'a, str>>,
    fixes_raw: Option<Cow<'a, str>>,
}

impl ParsedSigmetSectionRef<'_> {
    fn into_owned(self, raw: String) -> SigmetSection {
        SigmetSection {
            series: self.series.map(Cow::into_owned),
            identifier: self.identifier.map(Cow::into_owned),
            hazard: self.hazard.map(str::to_string),
            valid_from: self.valid_from.map(Cow::into_owned),
            valid_to: self.valid_to.map(Cow::into_owned),
            origin: self.origin.map(Cow::into_owned),
            states_raw: self.states_raw.map(Cow::into_owned),
            fixes_raw: self.fixes_raw.map(Cow::into_owned),
            raw,
        }
    }
}

/// Parses a SIGMET bulletin from text content.
pub(crate) fn parse_sigmet_bulletin(text: &str) -> Option<SigmetBulletin> {
    let sections = split_sigmet_sections(text);
    if sections.is_empty() {
        return None;
    }

    let mut parsed = Vec::new();
    let mut saw_cancellation = false;
    for raw in sections {
        if is_cancellation_section(&raw) {
            saw_cancellation = true;
            continue;
        }

        if let Some(section) = parse_convective_section_ref(&raw)
            .or_else(|| parse_oceanic_section_ref(&raw))
            .map(|section| section.into_owned(raw.clone()))
        {
            parsed.push(section);
        }
    }

    if parsed.is_empty() && !saw_cancellation {
        return None;
    }

    Some(SigmetBulletin { sections: parsed })
}

/// Splits SIGMET text into individual sections in a single pass over the lines.
fn split_sigmet_sections(text: &str) -> Vec<String> {
    let mut sections = Vec::new();
    let mut current = String::new();

    for raw_line in text.lines() {
        let line = strip_control_chars(raw_line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            push_section(&mut sections, &mut current);
            continue;
        }
        if trimmed.starts_with("^^") {
            continue;
        }
        if trimmed.starts_with("OUTLOOK VALID ") {
            push_section(&mut sections, &mut current);
            break;
        }
        if starts_new_section(trimmed) && !current.is_empty() {
            push_section(&mut sections, &mut current);
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(trimmed);
    }

    push_section(&mut sections, &mut current);
    sections
}

fn starts_new_section(line: &str) -> bool {
    line.starts_with("CONVECTIVE SIGMET ")
        || line.starts_with("KZAK SIGMET ")
        || line.starts_with("SIGMET ")
        || starts_with_icao_sigmet(line)
}

fn starts_with_icao_sigmet(line: &str) -> bool {
    let mut parts = line.split_whitespace();
    let Some(origin) = parts.next() else {
        return false;
    };
    let Some(sigmet) = parts.next() else {
        return false;
    };
    origin.len() == 4 && origin.chars().all(|ch| ch.is_ascii_uppercase()) && sigmet == "SIGMET"
}

fn push_section(sections: &mut Vec<String>, current: &mut String) {
    if current.is_empty() {
        return;
    }
    sections.push(std::mem::take(current));
}

/// Parses a convective SIGMET section from line-oriented text.
fn parse_convective_section_ref(raw: &str) -> Option<ParsedSigmetSectionRef<'_>> {
    let mut lines = raw.lines();
    let first = lines.next()?;
    let identifier = first.strip_prefix("CONVECTIVE SIGMET ")?;
    if identifier.is_empty() || identifier.contains(' ') {
        return None;
    }

    let mut valid_to = None;
    let mut states_buffer = String::new();
    let mut fixes_raw = None;
    let mut hazard = hazard_from_descriptor(first);

    for line in lines {
        if let Some(time) = line.strip_prefix("VALID UNTIL ") {
            if time.len() == 5
                && time.ends_with('Z')
                && time[..4].chars().all(|ch| ch.is_ascii_digit())
            {
                valid_to = Some(time);
            }
            continue;
        }
        if let Some(fixes) = line.strip_prefix("FROM ") {
            fixes_raw = Some(Cow::Borrowed(fixes.trim()));
            if hazard.is_none() {
                hazard = hazard_from_descriptor(line);
            }
            continue;
        }
        if hazard.is_none() {
            hazard = hazard_from_descriptor(line);
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !states_buffer.is_empty() {
            states_buffer.push(' ');
        }
        states_buffer.push_str(trimmed);
    }

    Some(ParsedSigmetSectionRef {
        series: Some(Cow::Borrowed("convective")),
        identifier: Some(Cow::Borrowed(identifier)),
        hazard,
        valid_from: None,
        valid_to: valid_to.map(Cow::Borrowed),
        origin: None,
        states_raw: (!states_buffer.is_empty()).then_some(Cow::Owned(states_buffer)),
        fixes_raw,
    })
}

/// Parses an oceanic or international SIGMET section from compacted text.
fn parse_oceanic_section_ref(raw: &str) -> Option<ParsedSigmetSectionRef<'_>> {
    let compact = compact_section(raw);
    let mut input = compact.as_str();

    let origin = take_while::<_, _, ContextError>(4..=4, |ch: char| ch.is_ascii_uppercase())
        .parse_next(&mut input)
        .ok()?;
    if let Some(rest) = input.strip_prefix(" SIGMET ") {
        input = rest;
    } else {
        return None;
    }

    let first = next_token(&mut input)?;
    let second = next_token(&mut input)?;
    let (series, identifier) = if second == "VALID" {
        (None, first)
    } else {
        let third = next_token(&mut input)?;
        if third != "VALID" {
            return None;
        }
        (Some(first), second)
    };

    let validity = next_token(&mut input)?;
    let (valid_from, valid_to) = validity.split_once('/')?;
    if !is_ddhhmm(valid_from) || !is_ddhhmm(valid_to) {
        return None;
    }

    let hazard = compact.split('.').next().and_then(hazard_from_descriptor);
    let fixes_raw = compact
        .split(" FIR ")
        .nth(1)
        .map(str::trim)
        .map(|rest| rest.trim_end_matches('.'))
        .filter(|rest| !rest.is_empty())
        .map(|rest| Cow::Owned(rest.to_string()));

    Some(ParsedSigmetSectionRef {
        series: series.map(|value| Cow::Owned(value.to_string())),
        identifier: Some(Cow::Owned(identifier.to_string())),
        hazard,
        valid_from: Some(Cow::Owned(valid_from.to_string())),
        valid_to: Some(Cow::Owned(valid_to.to_string())),
        origin: Some(Cow::Owned(origin.to_string())),
        states_raw: None,
        fixes_raw,
    })
}

fn compact_section(raw: &str) -> String {
    let mut compact = String::with_capacity(raw.len());
    let mut pending_space = false;

    for ch in raw.chars() {
        if ch.is_ascii_whitespace() {
            pending_space = true;
            continue;
        }

        if pending_space && !compact.is_empty() {
            compact.push(' ');
        }
        pending_space = false;
        compact.push(ch);
    }

    compact
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

fn strip_control_chars(line: &str) -> String {
    line.chars()
        .filter(|ch| !ch.is_ascii_control() || ch.is_ascii_whitespace())
        .collect()
}

fn hazard_from_descriptor(line: &str) -> Option<&'static str> {
    let upper = line.to_ascii_uppercase();
    ["SEV EMBD TS", "EMBD TS", "SEV TS", "TS"]
        .iter()
        .find_map(|needle| upper.contains(needle).then_some(*needle))
}

fn is_cancellation_section(raw: &str) -> bool {
    raw.to_ascii_uppercase().contains("CANCEL SIGMET")
}

fn is_ddhhmm(token: &str) -> bool {
    token.len() == 6 && token.chars().all(|ch| ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::parse_sigmet_bulletin;

    #[test]
    fn parses_convective_sigmet_sections() {
        let text = "CONVECTIVE SIGMET 54E\nVALID UNTIL 1355Z\nPA VA NC WV OH TN KY\nFROM 10NW EWC-40NNE HNN-20ESE VXV\nLINE EMBD TS 35 NM WIDE MOV FROM 24020KT. TOPS TO FL340.\n";
        let bulletin = parse_sigmet_bulletin(text).expect("sigmet bulletin should parse");

        assert_eq!(bulletin.sections.len(), 1);
        assert_eq!(bulletin.sections[0].identifier.as_deref(), Some("54E"));
        assert_eq!(bulletin.sections[0].valid_to.as_deref(), Some("1355Z"));
        assert_eq!(bulletin.sections[0].hazard.as_deref(), Some("EMBD TS"));
    }

    #[test]
    fn parses_oceanic_sigmet_sections() {
        let text = "KZAK SIGMET SIERRA 2 VALID 211100/211500 PHFO-\nOAKLAND OCEANIC FIR EMBD TS WI N12E156 - N09E158 - N10E153 -N12E156.\n";
        let bulletin = parse_sigmet_bulletin(text).expect("sigmet bulletin should parse");

        assert_eq!(bulletin.sections.len(), 1);
        assert_eq!(bulletin.sections[0].origin.as_deref(), Some("KZAK"));
        assert_eq!(bulletin.sections[0].series.as_deref(), Some("SIERRA"));
        assert_eq!(bulletin.sections[0].identifier.as_deref(), Some("2"));
        assert_eq!(bulletin.sections[0].valid_from.as_deref(), Some("211100"));
        assert_eq!(bulletin.sections[0].valid_to.as_deref(), Some("211500"));
    }

    #[test]
    fn parses_icao_origin_sigmet_sections() {
        let text = "WAAF SIGMET 05 VALID 090100/090700 WAAA-\nWAAF UJUNG PANDANG FIR VA ERUPTION MT IBU=\n";
        let bulletin = parse_sigmet_bulletin(text).expect("icao-origin sigmet should parse");

        assert_eq!(bulletin.sections.len(), 1);
        assert_eq!(bulletin.sections[0].origin.as_deref(), Some("WAAF"));
        assert_eq!(bulletin.sections[0].identifier.as_deref(), Some("05"));
    }

    #[test]
    fn cancellation_yields_empty_sections() {
        let text = "KZAK SIGMET PAPA 3 VALID 162358/162355 PHFO-OAKLAND OCEANIC FIR\nCANCEL SIGMET PAPA 2 VALID 161955/162355. TS HAVE DIMINISHED.\n";
        let bulletin = parse_sigmet_bulletin(text).expect("bulletin should still parse");
        assert!(bulletin.sections.is_empty());
    }

    #[test]
    fn outlook_terminates_section_collection() {
        let text = "CONVECTIVE SIGMET 54E\nVALID UNTIL 1355Z\nPA VA\nFROM 10NW EWC-40NNE HNN\nOUTLOOK VALID 151355-151755\nIGNORED\n";
        let bulletin = parse_sigmet_bulletin(text).expect("sigmet bulletin should parse");

        assert_eq!(bulletin.sections.len(), 1);
        assert!(!bulletin.sections[0].raw.contains("OUTLOOK"));
    }

    #[test]
    fn parses_multiple_sections() {
        let text = "CONVECTIVE SIGMET 54E\nVALID UNTIL 1355Z\nPA VA\nFROM 10NW EWC-40NNE HNN\n\nKZAK SIGMET SIERRA 2 VALID 211100/211500 PHFO-\nOAKLAND OCEANIC FIR EMBD TS WI N12E156.\n";
        let bulletin = parse_sigmet_bulletin(text).expect("expected multiple sections");

        assert_eq!(bulletin.sections.len(), 2);
    }

    #[test]
    fn invalid_sections_are_rejected() {
        assert!(parse_sigmet_bulletin("NOT A SIGMET").is_none());
    }

    #[test]
    fn control_characters_do_not_break_parse() {
        let text = "KZAK SIGMET SIERRA 2 VALID 211100/211500 PHFO-\nOAKLAND OCEANIC FIR EMBD TS WI N12E156.\u{3}\n";
        let bulletin = parse_sigmet_bulletin(text).expect("control-char sigmet should parse");

        assert_eq!(bulletin.sections.len(), 1);
        assert_eq!(bulletin.sections[0].origin.as_deref(), Some("KZAK"));
    }
}
