//! Structured SIGMET bulletin parsing.

use serde::Serialize;
use winnow::Parser;
use winnow::error::ContextError;
use winnow::token::take_while;

use crate::{LatLonPolygon, parse_latlon_polygons};

/// SIGMET bulletin containing multiple SIGMET sections.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SigmetBulletin {
    /// Individual SIGMET sections in the bulletin
    pub sections: Vec<SigmetSection>,
}

/// Individual SIGMET section with normalized validity and geometry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SigmetSection {
    /// Section kind such as `convective` or `international`
    pub kind: String,
    /// Series identifier (e.g., `SIERRA`, `TANGO`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    /// SIGMET alphanumeric identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    /// Hazard type description (e.g., `EMBD TS`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hazard: Option<String>,
    /// Valid from time (DDHHMM)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    /// Valid until time (DDHHMM or HHMMZ)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    /// Originating weather office (e.g., `KZAK`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// Raw states/areas text from convective products
    #[serde(skip_serializing_if = "Option::is_none")]
    pub states_raw: Option<String>,
    /// Raw location/fix expression
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixes_raw: Option<String>,
    /// Polygon geometry only when directly encoded in the bulletin
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<LatLonPolygon>,
    /// True when the section is a cancellation
    pub is_cancellation: bool,
    /// Complete raw text of this SIGMET section
    pub raw: String,
}

/// Parses a SIGMET bulletin from text content.
pub(crate) fn parse_sigmet_bulletin(text: &str) -> Option<SigmetBulletin> {
    let sections = split_sigmet_sections(text);
    if sections.is_empty() {
        return None;
    }

    let parsed = sections
        .into_iter()
        .filter_map(|raw| {
            parse_cancellation_section(&raw)
                .or_else(|| parse_convective_section(&raw))
                .or_else(|| parse_oceanic_section(&raw))
        })
        .collect::<Vec<_>>();

    (!parsed.is_empty()).then_some(SigmetBulletin { sections: parsed })
}

fn parse_cancellation_section(raw: &str) -> Option<SigmetSection> {
    let upper = raw.to_ascii_uppercase();
    if !upper.contains("CANCEL SIGMET") && !upper.contains("CNL SIGMET") {
        return None;
    }

    Some(SigmetSection {
        kind: "cancellation".to_string(),
        series: None,
        identifier: extract_identifier(&upper),
        hazard: None,
        valid_from: None,
        valid_to: None,
        origin: None,
        states_raw: None,
        fixes_raw: None,
        geometry: None,
        is_cancellation: true,
        raw: raw.to_string(),
    })
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

fn parse_convective_section(raw: &str) -> Option<SigmetSection> {
    let mut lines = raw.lines();
    let first = lines.next()?;
    let identifier = first.strip_prefix("CONVECTIVE SIGMET ")?;
    if identifier.is_empty() || identifier.contains(' ') {
        return None;
    }

    let mut valid_to = None;
    let mut states_buffer = String::new();
    let mut fixes_raw = None::<String>;
    let mut hazard = hazard_from_descriptor(first).map(str::to_string);

    for line in lines {
        if let Some(time) = line.strip_prefix("VALID UNTIL ") {
            if is_hhmmz(time) {
                valid_to = Some(time.to_string());
            }
            continue;
        }
        if let Some(fixes) = line.strip_prefix("FROM ") {
            fixes_raw = Some(fixes.trim().to_string());
            if hazard.is_none() {
                hazard = hazard_from_descriptor(line).map(str::to_string);
            }
            continue;
        }
        if hazard.is_none() {
            hazard = hazard_from_descriptor(line).map(str::to_string);
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

    Some(SigmetSection {
        kind: "convective".to_string(),
        series: Some("convective".to_string()),
        identifier: Some(identifier.to_string()),
        hazard,
        valid_from: None,
        valid_to,
        origin: None,
        states_raw: (!states_buffer.is_empty()).then_some(states_buffer),
        fixes_raw,
        geometry: None,
        is_cancellation: false,
        raw: raw.to_string(),
    })
}

fn parse_oceanic_section(raw: &str) -> Option<SigmetSection> {
    let compact = compact_section(raw);
    let mut input = compact.as_str();

    let origin = take_while::<_, _, ContextError>(4..=4, |ch: char| ch.is_ascii_uppercase())
        .parse_next(&mut input)
        .ok()?;
    input = input.strip_prefix(" SIGMET ")?;

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

    let fixes_raw = compact
        .split(" FIR ")
        .nth(1)
        .map(str::trim)
        .map(|rest| rest.trim_end_matches('.').to_string())
        .filter(|rest| !rest.is_empty());
    let geometry = parse_latlon_polygons(raw).into_iter().next();

    Some(SigmetSection {
        kind: "international".to_string(),
        series: series.map(str::to_string),
        identifier: Some(identifier.to_string()),
        hazard: compact
            .split('.')
            .next()
            .and_then(hazard_from_descriptor)
            .map(str::to_string),
        valid_from: Some(valid_from.to_string()),
        valid_to: Some(valid_to.to_string()),
        origin: Some(origin.to_string()),
        states_raw: None,
        fixes_raw,
        geometry,
        is_cancellation: false,
        raw: raw.to_string(),
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
    if upper.contains("SEV EMBD TS") {
        return Some("SEV EMBD TS");
    }
    if upper.contains("EMBD TS") {
        return Some("EMBD TS");
    }
    if upper.contains("SEV TS") {
        return Some("SEV TS");
    }
    if upper.contains(" TS") || upper.starts_with("TS ") || upper.contains("TS.") {
        return Some("TS");
    }
    if upper.contains("TURB") {
        return Some("TURB");
    }
    if upper.contains("ICE") {
        return Some("ICE");
    }
    if upper.contains("VA CLD") || upper.contains("VA ERUPTION") {
        return Some("VA");
    }
    None
}

fn extract_identifier(upper: &str) -> Option<String> {
    let mut parts = upper.split_whitespace();
    while let Some(token) = parts.next() {
        if token == "SIGMET" {
            return parts.next().map(str::to_string);
        }
    }
    None
}

fn is_ddhhmm(token: &str) -> bool {
    token.len() == 6 && token.chars().all(|ch| ch.is_ascii_digit())
}

fn is_hhmmz(token: &str) -> bool {
    token.len() == 5 && token.ends_with('Z') && token[..4].chars().all(|ch| ch.is_ascii_digit())
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
        assert_eq!(bulletin.sections[0].hazard.as_deref(), Some("EMBD TS"));
        assert!(bulletin.sections[0].geometry.is_none());
    }

    #[test]
    fn parses_international_sigmet_sections() {
        let text = "WAAF SIGMET 05 VALID 090100/090700 WAAA-\nWAAF UJUNG PANDANG FIR VA ERUPTION MT IBU PSN N0129 E12738 VA CLD\nOBS AT 0040Z WI N0129 E12737 - N0131 E12738 - N0129 E12751 - N0117\nE12744 - N0129 E12737 SFC/FL070 MOV SE 10KT NC=\n";
        let bulletin = parse_sigmet_bulletin(text).expect("sigmet bulletin should parse");

        assert_eq!(bulletin.sections.len(), 1);
        assert_eq!(bulletin.sections[0].origin.as_deref(), Some("WAAF"));
        assert_eq!(bulletin.sections[0].identifier.as_deref(), Some("05"));
        assert_eq!(bulletin.sections[0].valid_from.as_deref(), Some("090100"));
    }

    #[test]
    fn keeps_cancellation_sections() {
        let text = "KZAK SIGMET ALFA 3 VALID 090100/090700 KZAK-\nCANCEL SIGMET ALFA 2.\n";
        let bulletin = parse_sigmet_bulletin(text).expect("sigmet bulletin should parse");

        assert_eq!(bulletin.sections.len(), 1);
        assert!(bulletin.sections[0].is_cancellation);
    }
}
