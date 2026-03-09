//! Minimal SIGMET bulletin parsing.
//!
//! SIGMET (Significant Meteorological Information) bulletins contain critical
//! weather hazard information for aviation. This module parses both convective
//! SIGMETs (thunderstorms) and oceanic SIGMETs (international waters).
//!
//! ## SIGMET Types
//!
//! - **Convective SIGMETs**: Issued for thunderstorms, hail, and severe turbulence
//! - **Oceanic SIGMETs**: Issued for international airspace (e.g., KZAK for Oakland FIR)

use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

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

/// Parses a SIGMET bulletin from text content.
///
/// Splits the bulletin into sections and parses each as either convective
/// or oceanic format. Cancellation sections are filtered out.
///
/// # Arguments
///
/// * `text` - Raw SIGMET bulletin text
///
/// # Returns
///
/// `Some(SigmetBulletin)` if at least one valid section was parsed,
/// `None` if no valid sections were found
pub(crate) fn parse_sigmet_bulletin(text: &str) -> Option<SigmetBulletin> {
    let sections = split_sections(text);
    if sections.is_empty() {
        return None;
    }

    let mut parsed = Vec::new();
    for raw in sections {
        if cancellation_re().is_match(&raw) {
            continue;
        }

        if let Some(section) =
            parse_convective_section(&raw).or_else(|| parse_oceanic_section(&raw))
        {
            parsed.push(section);
        }
    }

    Some(SigmetBulletin { sections: parsed })
}

/// Splits SIGMET text into individual sections.
///
/// Sections are separated by blank lines or new SIGMET headers.
/// Stops at OUTLOOK sections.
fn split_sections(text: &str) -> Vec<String> {
    let lines = text
        .lines()
        .map(strip_control_chars)
        .map(|line| line.trim_end().to_string())
        .collect::<Vec<_>>();
    let mut sections = Vec::new();
    let mut current = Vec::new();

    for line in lines {
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
        current.push(trimmed.to_string());
    }

    push_section(&mut sections, &mut current);
    sections
}

/// Checks if a line starts a new SIGMET section.
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

/// Pushes accumulated lines as a section if non-empty.
fn push_section(sections: &mut Vec<String>, current: &mut Vec<String>) {
    if current.is_empty() {
        return;
    }
    sections.push(current.join("\n"));
    current.clear();
}

/// Parses a convective SIGMET section.
///
/// Format: `CONVECTIVE SIGMET <id>\nVALID UNTIL <time>\n<states>\nFROM <fixes>`
fn parse_convective_section(raw: &str) -> Option<SigmetSection> {
    let lines = raw.lines().collect::<Vec<_>>();
    let first = *lines.first()?;
    let captures = convective_header_re().captures(first)?;
    let valid_to = lines.iter().find_map(|line| {
        valid_until_re()
            .captures(line)
            .and_then(|caps| caps.name("time").map(|v| v.as_str().to_string()))
    });
    let states_raw = lines
        .iter()
        .skip(1)
        .take_while(|line| !line.starts_with("FROM "))
        .filter(|line| !line.starts_with("VALID UNTIL "))
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let fixes_raw = lines
        .iter()
        .find_map(|line| line.strip_prefix("FROM ").map(str::trim))
        .map(str::to_string);
    let hazard = lines
        .iter()
        .find(|line| line.contains(" TS"))
        .and_then(|line| hazard_from_descriptor(line));

    Some(SigmetSection {
        series: Some("convective".to_string()),
        identifier: captures
            .name("identifier")
            .map(|value| value.as_str().to_string()),
        hazard,
        valid_from: None,
        valid_to,
        origin: None,
        states_raw: (!states_raw.is_empty()).then_some(states_raw),
        fixes_raw,
        raw: raw.to_string(),
    })
}

/// Parses an oceanic SIGMET section.
///
/// Format: `<origin> SIGMET <series> <id> VALID <from>/<to> <text>`
fn parse_oceanic_section(raw: &str) -> Option<SigmetSection> {
    let compact = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let captures = oceanic_header_re().captures(&compact)?;
    let hazard = compact.split('.').next().and_then(hazard_from_descriptor);
    let fixes_raw = compact
        .split(" FIR ")
        .nth(1)
        .map(str::trim)
        .map(|rest| rest.trim_end_matches('.').to_string());

    Some(SigmetSection {
        series: captures
            .name("series")
            .map(|value| value.as_str().to_string()),
        identifier: captures
            .name("identifier")
            .map(|value| value.as_str().to_string()),
        hazard,
        valid_from: captures
            .name("valid_from")
            .map(|value| value.as_str().to_string()),
        valid_to: captures
            .name("valid_to")
            .map(|value| value.as_str().to_string()),
        origin: captures
            .name("origin")
            .map(|value| value.as_str().to_string()),
        states_raw: None,
        fixes_raw,
        raw: raw.to_string(),
    })
}

/// Removes non-whitespace control characters from a line.
fn strip_control_chars(line: &str) -> String {
    line.chars()
        .filter(|ch| !ch.is_ascii_control() || ch.is_ascii_whitespace())
        .collect()
}

/// Extracts hazard type from a line containing thunderstorm descriptors.
fn hazard_from_descriptor(line: &str) -> Option<String> {
    let upper = line.to_ascii_uppercase();
    ["SEV EMBD TS", "EMBD TS", "SEV TS", "TS"]
        .iter()
        .find_map(|needle| upper.contains(needle).then(|| (*needle).to_string()))
}

/// Regex for convective SIGMET header line.
fn convective_header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^CONVECTIVE SIGMET (?P<identifier>[0-9A-Z]+)\s*$")
            .expect("sigmet convective header regex compiles")
    })
}

/// Regex for VALID UNTIL time extraction.
fn valid_until_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^VALID UNTIL (?P<time>\d{4}Z)\s*$").expect("sigmet valid-until regex compiles")
    })
}

/// Regex for oceanic SIGMET header parsing.
fn oceanic_header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?P<origin>[A-Z]{4}) SIGMET (?:(?P<series>[A-Z]+)\s+)?(?P<identifier>[0-9A-Z]+) VALID (?P<valid_from>\d{6})/(?P<valid_to>\d{6}) ",
        )
        .expect("sigmet oceanic header regex compiles")
    })
}

/// Regex to detect SIGMET cancellations.
fn cancellation_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bCANCEL SIGMET\b").expect("sigmet cancel regex compiles"))
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
    fn cancellation_yields_empty_sections() {
        let text = "KZAK SIGMET PAPA 3 VALID 162358/162355 PHFO-OAKLAND OCEANIC FIR\nCANCEL SIGMET PAPA 2 VALID 161955/162355. TS HAVE DIMINISHED.\n";
        let bulletin = parse_sigmet_bulletin(text).expect("bulletin should still parse");
        assert!(bulletin.sections.is_empty());
    }

    #[test]
    fn parses_international_sigmet_bulletin() {
        let text = "WAAF SIGMET 05 VALID 090100/090700 WAAA-\nWAAF UJUNG PANDANG FIR VA ERUPTION MT IBU PSN N0129 E12738 VA CLD OBS AT 0040Z WI N0129 E12737 - N0131 E12738 - N0129 E12751 - N0117 E12744 - N0129 E12737 SFC/FL070 MOV SE 10KT NC=\n";
        let bulletin = parse_sigmet_bulletin(text).expect("expected international SIGMET parsing");

        assert_eq!(bulletin.sections.len(), 1);
        assert_eq!(bulletin.sections[0].origin.as_deref(), Some("WAAF"));
        assert_eq!(bulletin.sections[0].identifier.as_deref(), Some("05"));
    }

    #[test]
    fn parses_philippines_sigmet_bulletin() {
        let text = "RPHI SIGMET 1 VALID 090034/090634 RPLL-\nRPHI MANILA FIR VA ERUPTION MT MAYON PSN N1315 E12341 VA CLD OBS AT 0000Z=\n";
        let bulletin = parse_sigmet_bulletin(text).expect("expected Philippines SIGMET parsing");

        assert_eq!(bulletin.sections.len(), 1);
        assert_eq!(bulletin.sections[0].origin.as_deref(), Some("RPHI"));
        assert_eq!(bulletin.sections[0].identifier.as_deref(), Some("1"));
    }
}
