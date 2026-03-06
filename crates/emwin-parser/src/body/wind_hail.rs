//! WIND/HAIL tag parsing.

use regex::Regex;
use std::sync::OnceLock;

use crate::ProductParseIssue;

/// Parsed wind/hail tag kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindHailKind {
    LegacyWind,
    LegacyHail,
    WindThreat,
    MaxWindGust,
    HailThreat,
    MaxHailSize,
}

/// Parsed wind/hail entry.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WindHailEntry {
    pub kind: WindHailKind,
    pub raw_value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison: Option<char>,
}

/// Parse wind and hail tags from text.
pub fn parse_wind_hail_entries(text: &str) -> Vec<WindHailEntry> {
    parse_wind_hail_entries_with_issues(text).0
}

/// Parse wind and hail tags and collect structured issues.
pub fn parse_wind_hail_entries_with_issues(
    text: &str,
) -> (Vec<WindHailEntry>, Vec<ProductParseIssue>) {
    let mut entries = Vec::new();
    let mut issues = Vec::new();

    let flattened = text.replace('\n', " ");

    for captures in legacy_wind_hail_regex().captures_iter(&flattened) {
        let Some(raw) = captures.get(0).map(|value| value.as_str()) else {
            continue;
        };

        match parse_legacy_wind_hail_capture(&captures, raw) {
            Ok(mut parsed) => entries.append(&mut parsed),
            Err(issue) => issues.push(issue),
        }
    }

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if modern_wind_hail_candidate_regex().is_match(line) {
            match parse_modern_wind_hail_line(line) {
                Ok(Some(entry)) => entries.push(entry),
                Ok(None) => {}
                Err(issue) => issues.push(issue),
            }
        }
    }

    (entries, issues)
}

fn legacy_wind_hail_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)WIND\.\.\.\s*([<>])?\s*([0-9]+)\s*(MPH|KTS)\s+HAIL\.\.\.\s*([<>])?\s*([0-9.]+)\s*IN",
        )
        .expect("legacy wind hail regex compiles")
    })
}

fn modern_wind_hail_candidate_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(HAIL(?:THREAT)?|MAX\s*HAIL\s*SIZE|MAXHAILSIZE|WIND(?:THREAT)?|MAX\s*WIND\s*GUST|MAXWINDGUST)\.\.\.",
        )
        .expect("modern wind hail candidate regex compiles")
    })
}

fn modern_wind_hail_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^([A-Z ]+?)\.\.\.\s*(.+?)\s*$").expect("modern wind hail regex compiles")
    })
}

fn parse_legacy_wind_hail_capture(
    captures: &regex::Captures<'_>,
    raw: &str,
) -> Result<Vec<WindHailEntry>, ProductParseIssue> {
    let wind_value = captures
        .get(2)
        .and_then(|value| value.as_str().parse::<f64>().ok())
        .ok_or_else(|| invalid_wind_hail_issue(raw))?;
    let wind_units = captures
        .get(3)
        .map(|value| value.as_str().to_ascii_uppercase())
        .ok_or_else(|| invalid_wind_hail_issue(raw))?;
    let hail_value = captures
        .get(5)
        .and_then(|value| value.as_str().parse::<f64>().ok())
        .ok_or_else(|| invalid_wind_hail_issue(raw))?;

    Ok(vec![
        WindHailEntry {
            kind: WindHailKind::LegacyWind,
            raw_value: captures
                .get(2)
                .map(|value| value.as_str().to_string())
                .unwrap_or_default(),
            numeric_value: Some(wind_value),
            units: Some(wind_units),
            comparison: captures
                .get(1)
                .and_then(|value| value.as_str().chars().next()),
        },
        WindHailEntry {
            kind: WindHailKind::LegacyHail,
            raw_value: captures
                .get(5)
                .map(|value| value.as_str().to_string())
                .unwrap_or_default(),
            numeric_value: Some(hail_value),
            units: Some("IN".to_string()),
            comparison: captures
                .get(4)
                .and_then(|value| value.as_str().chars().next()),
        },
    ])
}

fn parse_modern_wind_hail_line(line: &str) -> Result<Option<WindHailEntry>, ProductParseIssue> {
    let Some(captures) = modern_wind_hail_regex().captures(line) else {
        return Err(invalid_wind_hail_issue(line));
    };

    let label = captures
        .get(1)
        .map(|value| value.as_str().trim().to_ascii_uppercase().replace(' ', ""))
        .ok_or_else(|| invalid_wind_hail_issue(line))?;
    let value = captures
        .get(2)
        .map(|value| value.as_str().trim().to_string())
        .ok_or_else(|| invalid_wind_hail_issue(line))?;

    let entry = match label.as_str() {
        "HAILTHREAT" => WindHailEntry {
            kind: WindHailKind::HailThreat,
            raw_value: value,
            numeric_value: None,
            units: None,
            comparison: None,
        },
        "WINDTHREAT" => WindHailEntry {
            kind: WindHailKind::WindThreat,
            raw_value: value,
            numeric_value: None,
            units: None,
            comparison: None,
        },
        "HAIL" | "MAXHAILSIZE" => parse_numeric_line(
            WindHailKind::MaxHailSize,
            &value,
            &["IN"],
            "invalid_wind_hail_hail_value",
            line,
        )?,
        "WIND" | "MAXWINDGUST" => parse_numeric_line(
            WindHailKind::MaxWindGust,
            &value,
            &["MPH", "KTS"],
            "invalid_wind_hail_wind_value",
            line,
        )?,
        _ => return Ok(None),
    };

    Ok(Some(entry))
}

fn parse_numeric_line(
    kind: WindHailKind,
    value: &str,
    allowed_units: &[&str],
    error_code: &'static str,
    raw: &str,
) -> Result<WindHailEntry, ProductParseIssue> {
    let captures = numeric_value_regex().captures(value).ok_or_else(|| {
        ProductParseIssue::new(
            "wind_hail_parse",
            error_code,
            format!("could not parse wind/hail value from line: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;

    let comparison = captures.get(1).and_then(|m| m.as_str().chars().next());
    let numeric_value = captures
        .get(2)
        .and_then(|m| m.as_str().parse::<f64>().ok())
        .ok_or_else(|| {
            ProductParseIssue::new(
                "wind_hail_parse",
                error_code,
                format!("could not parse wind/hail value from line: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;
    let units = captures
        .get(3)
        .map(|m| m.as_str().to_ascii_uppercase())
        .filter(|units| allowed_units.iter().any(|allowed| units == allowed))
        .ok_or_else(|| {
            ProductParseIssue::new(
                "wind_hail_parse",
                error_code,
                format!("could not parse wind/hail units from line: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;

    Ok(WindHailEntry {
        kind,
        raw_value: value.to_string(),
        numeric_value: Some(numeric_value),
        units: Some(units),
        comparison,
    })
}

fn numeric_value_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^([<>])?\s*([0-9]+(?:\.[0-9]+)?)\s*([A-Z]+)\s*$")
            .expect("numeric value regex compiles")
    })
}

fn invalid_wind_hail_issue(raw: &str) -> ProductParseIssue {
    ProductParseIssue::new(
        "wind_hail_parse",
        "invalid_wind_hail_format",
        format!("could not parse wind/hail line: `{raw}`"),
        Some(raw.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_legacy_wind_hail_tags() {
        let text = "WIND...>60MPH HAIL...<1.00IN";
        let entries = parse_wind_hail_entries(text);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].kind, WindHailKind::LegacyWind);
        assert_eq!(entries[0].comparison, Some('>'));
        assert_eq!(entries[1].kind, WindHailKind::LegacyHail);
        assert_eq!(entries[1].comparison, Some('<'));
    }

    #[test]
    fn parse_modern_wind_hail_tags() {
        let text = "HAILTHREAT...RADARINDICATED\nMAXHAILSIZE...1.00 IN\nWINDTHREAT...OBSERVED\nMAXWINDGUST...60 MPH";
        let entries = parse_wind_hail_entries(text);

        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].kind, WindHailKind::HailThreat);
        assert_eq!(entries[1].kind, WindHailKind::MaxHailSize);
        assert_eq!(entries[2].kind, WindHailKind::WindThreat);
        assert_eq!(entries[3].kind, WindHailKind::MaxWindGust);
    }

    #[test]
    fn parse_invalid_modern_wind_hail_reports_issue() {
        let text = "MAXWINDGUST...FAST";
        let (entries, issues) = parse_wind_hail_entries_with_issues(text);

        assert!(entries.is_empty());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "invalid_wind_hail_wind_value");
    }
}
