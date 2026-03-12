//! Parsing for WPC excessive rainfall outlook text products.

use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EroBulletin {
    pub day: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    pub outlooks: Vec<EroOutlook>,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EroOutlook {
    pub category: String,
    pub threshold: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub location_tokens: Vec<String>,
    pub raw: String,
}

pub(crate) fn parse_ero_bulletin(text: &str, afos: Option<&str>) -> Option<EroBulletin> {
    let normalized = text.replace('\r', "");
    let compact = normalized.trim().to_string();
    let day = match afos {
        Some("RBG94E") => 1,
        Some("RBG98E") => 2,
        Some("RBG99E") => 3,
        _ => return None,
    };
    let (valid_from, valid_to) = valid_re()
        .captures(&compact)
        .map(|captures| {
            (
                captures
                    .name("from")
                    .map(|value| value.as_str().to_string()),
                captures.name("to").map(|value| value.as_str().to_string()),
            )
        })
        .unwrap_or((None, None));

    let mut outlooks = Vec::new();
    let lines: Vec<&str> = compact.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        if let Some(captures) = outlook_re().captures(line) {
            let risk = captures.name("risk")?.as_str().to_ascii_uppercase();
            let mut raw = String::from(line);
            index += 1;
            while index < lines.len() {
                let next = lines[index].trim();
                if next.is_empty() {
                    break;
                }
                raw.push(' ');
                raw.push_str(next);
                if next.ends_with('.') {
                    break;
                }
                index += 1;
            }
            let location_tokens = raw
                .split("TO THE RIGHT OF A LINE FROM")
                .nth(1)
                .unwrap_or_default()
                .trim()
                .trim_end_matches('.')
                .split_whitespace()
                .map(str::to_string)
                .collect();
            outlooks.push(EroOutlook {
                category: "categorical".to_string(),
                threshold: normalize_risk(&risk),
                location_tokens,
                raw,
            });
        }
        index += 1;
    }

    (!outlooks.is_empty()).then_some(EroBulletin {
        day,
        cycle: None,
        valid_from,
        valid_to,
        outlooks,
        raw: compact,
    })
}

fn normalize_risk(value: &str) -> String {
    match value {
        "MARGINAL" => "MRGL",
        "SLIGHT" => "SLGT",
        "MODERATE" => "MDT",
        "HIGH" => "HIGH",
        other => other,
    }
    .to_string()
}

fn valid_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?im)^Valid\s+(?P<from>.+?)\s+-\s+(?P<to>.+?)$")
            .expect("valid ero valid regex")
    })
}

fn outlook_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(?P<risk>MARGINAL|SLIGHT|MODERATE|HIGH)\s+RISK OF RAINFALL EXCEEDING FFG TO THE RIGHT OF A LINE FROM$",
        )
        .expect("valid ero outlook regex")
    })
}

#[cfg(test)]
mod tests {
    use super::parse_ero_bulletin;

    #[test]
    fn parses_day_and_location_tokens() {
        let text = "\
Day 1 Excessive Rainfall Threat Area
Valid 2156Z Tue Jul 13 2021 - 12Z Wed Jul 14 2021

MARGINAL RISK OF RAINFALL EXCEEDING FFG TO THE RIGHT OF A LINE FROM
20 N CYSC 20 N 1V4 20 SW PSF.

SLIGHT RISK OF RAINFALL EXCEEDING FFG TO THE RIGHT OF A LINE FROM
75 W OLS 40 S GBN 35 W GYR.";
        let bulletin = parse_ero_bulletin(text, Some("RBG94E")).expect("ero bulletin");
        assert_eq!(bulletin.day, 1);
        assert_eq!(bulletin.outlooks.len(), 2);
        assert_eq!(bulletin.outlooks[0].threshold, "MRGL");
        assert!(bulletin.outlooks[0].location_tokens.starts_with(&[
            "20".into(),
            "N".into(),
            "CYSC".into()
        ]));
    }
}
