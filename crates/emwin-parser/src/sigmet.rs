//! Minimal SIGMET bulletin parsing.

use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SigmetBulletin {
    pub sections: Vec<SigmetSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SigmetSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hazard: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub states_raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixes_raw: Option<String>,
    pub raw: String,
}

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

fn starts_new_section(line: &str) -> bool {
    line.starts_with("CONVECTIVE SIGMET ")
        || line.starts_with("KZAK SIGMET ")
        || line.starts_with("SIGMET ")
}

fn push_section(sections: &mut Vec<String>, current: &mut Vec<String>) {
    if current.is_empty() {
        return;
    }
    sections.push(current.join("\n"));
    current.clear();
}

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

fn strip_control_chars(line: &str) -> String {
    line.chars()
        .filter(|ch| !ch.is_ascii_control() || ch.is_ascii_whitespace())
        .collect()
}

fn hazard_from_descriptor(line: &str) -> Option<String> {
    let upper = line.to_ascii_uppercase();
    ["SEV EMBD TS", "EMBD TS", "SEV TS", "TS"]
        .iter()
        .find_map(|needle| upper.contains(needle).then(|| (*needle).to_string()))
}

fn convective_header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^CONVECTIVE SIGMET (?P<identifier>[0-9A-Z]+)\s*$")
            .expect("sigmet convective header regex compiles")
    })
}

fn valid_until_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^VALID UNTIL (?P<time>\d{4}Z)\s*$").expect("sigmet valid-until regex compiles")
    })
}

fn oceanic_header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?P<origin>[A-Z]{4}) SIGMET (?P<series>[A-Z]+) (?P<identifier>\d+) VALID (?P<valid_from>\d{6})/(?P<valid_to>\d{6}) ",
        )
        .expect("sigmet oceanic header regex compiles")
    })
}

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
}
