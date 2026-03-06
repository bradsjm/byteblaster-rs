//! TIME...MOT...LOC parsing.

use regex::Regex;
use std::sync::OnceLock;

use crate::ProductParseIssue;

/// Parsed TIME...MOT...LOC entry.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TimeMotLocEntry {
    /// UTC time token in HHMMZ form.
    pub time_utc: String,
    /// Motion direction in degrees.
    pub direction_degrees: u16,
    /// Motion speed in knots.
    pub speed_kt: u16,
    /// Coordinate pairs as (latitude, longitude) in decimal degrees.
    pub points: Vec<(f64, f64)>,
    /// WKT representation as POINT or LINESTRING.
    pub wkt: String,
}

/// Parse TIME...MOT...LOC entries from text.
pub fn parse_time_mot_loc_entries(text: &str) -> Vec<TimeMotLocEntry> {
    parse_time_mot_loc_entries_with_issues(text).0
}

/// Parse TIME...MOT...LOC entries and collect structured issues.
pub fn parse_time_mot_loc_entries_with_issues(
    text: &str,
) -> (Vec<TimeMotLocEntry>, Vec<ProductParseIssue>) {
    let mut entries = Vec::new();
    let mut issues = Vec::new();
    let normalized = text.replace('\r', "").replace('\n', " ");

    for candidate in time_mot_loc_candidate_regex().find_iter(&normalized) {
        let line = candidate.as_str().trim();
        let Some(captures) = time_mot_loc_regex().captures(line) else {
            issues.push(invalid_format_issue(line));
            continue;
        };

        match parse_time_mot_loc_capture(&captures, line) {
            Ok(entry) => entries.push(entry),
            Err(issue) => issues.push(issue),
        }
    }

    (entries, issues)
}

fn time_mot_loc_candidate_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)TIME\.\.\.MOT\.\.\.LOC\s+[0-9]{4}Z\s+[0-9]{1,3}DEG\s+[0-9]{1,3}KT\s+(?:[0-9]{1,8}\s*)+",
        )
        .expect("time mot loc candidate regex compiles")
    })
}

fn time_mot_loc_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^TIME\.\.\.MOT\.\.\.LOC\s+([0-9]{4}Z)\s+([0-9]{1,3})DEG\s+([0-9]{1,3})KT\s+(.+?)\s*$",
        )
        .expect("time mot loc regex compiles")
    })
}

fn parse_time_mot_loc_capture(
    captures: &regex::Captures<'_>,
    raw: &str,
) -> Result<TimeMotLocEntry, ProductParseIssue> {
    let time_utc = captures
        .get(1)
        .map(|value| value.as_str().to_string())
        .ok_or_else(|| invalid_format_issue(raw))?;
    let direction_degrees = captures
        .get(2)
        .and_then(|value| value.as_str().parse().ok())
        .ok_or_else(|| {
            ProductParseIssue::new(
                "time_mot_loc_parse",
                "invalid_time_mot_loc_direction",
                format!("could not parse TIME...MOT...LOC direction from line: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;
    let speed_kt = captures
        .get(3)
        .and_then(|value| value.as_str().parse().ok())
        .ok_or_else(|| {
            ProductParseIssue::new(
                "time_mot_loc_parse",
                "invalid_time_mot_loc_speed",
                format!("could not parse TIME...MOT...LOC speed from line: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;
    let coords_str = captures
        .get(4)
        .map(|value| value.as_str())
        .ok_or_else(|| invalid_format_issue(raw))?;

    let raw_coord_tokens: Vec<&str> = coords_str.split_whitespace().collect();
    let coord_tokens = normalize_coordinate_tokens(&raw_coord_tokens).ok_or_else(|| {
        ProductParseIssue::new(
            "time_mot_loc_parse",
            "invalid_time_mot_loc_coordinate_format",
            format!("could not normalize TIME...MOT...LOC coordinates from line: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;
    if coord_tokens.len() < 2 || !coord_tokens.len().is_multiple_of(2) {
        return Err(ProductParseIssue::new(
            "time_mot_loc_parse",
            "invalid_time_mot_loc_coordinate_count",
            format!("TIME...MOT...LOC line has an invalid coordinate count: `{raw}`"),
            Some(raw.to_string()),
        ));
    }

    let mut points = Vec::with_capacity(coord_tokens.len() / 2);
    for pair in coord_tokens.chunks(2) {
        let lat = parse_coordinate(&pair[0], true).ok_or_else(|| {
            ProductParseIssue::new(
                "time_mot_loc_parse",
                "invalid_time_mot_loc_latitude",
                format!("could not parse TIME...MOT...LOC latitude from line: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;
        let lon = parse_coordinate(&pair[1], false).ok_or_else(|| {
            ProductParseIssue::new(
                "time_mot_loc_parse",
                "invalid_time_mot_loc_longitude",
                format!("could not parse TIME...MOT...LOC longitude from line: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;
        points.push((lat, lon));
    }

    let wkt = if points.len() == 1 {
        format!("POINT({:.4} {:.4})", points[0].1, points[0].0)
    } else {
        let pairs = points
            .iter()
            .map(|(lat, lon)| format!("{lon:.4} {lat:.4}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("LINESTRING({pairs})")
    };

    Ok(TimeMotLocEntry {
        time_utc,
        direction_degrees,
        speed_kt,
        points,
        wkt,
    })
}

fn normalize_coordinate_tokens(tokens: &[&str]) -> Option<Vec<String>> {
    let mut normalized = Vec::new();
    let mut index = 0;

    while index < tokens.len() {
        let token = consume_coordinate_token(tokens, &mut index)?;
        normalized.push(token);
    }

    Some(normalized)
}

fn consume_coordinate_token(tokens: &[&str], index: &mut usize) -> Option<String> {
    let mut token = String::new();

    while *index < tokens.len() {
        token.push_str(tokens[*index].trim());
        *index += 1;

        if (4..=8).contains(&token.len()) {
            return Some(token);
        }
        if token.len() > 8 {
            return None;
        }
    }

    None
}

fn invalid_format_issue(raw: &str) -> ProductParseIssue {
    ProductParseIssue::new(
        "time_mot_loc_parse",
        "invalid_time_mot_loc_format",
        format!("could not parse TIME...MOT...LOC line: `{raw}`"),
        Some(raw.to_string()),
    )
}

fn parse_coordinate(text: &str, is_lat: bool) -> Option<f64> {
    let digits = text.trim().trim_start_matches('-');
    let negative = !is_lat;

    let value = match digits.len() {
        4 => {
            let degrees: f64 = digits[0..2].parse().ok()?;
            let hundredths: f64 = digits[2..4].parse().ok()?;
            degrees + (hundredths / 100.0)
        }
        5 => {
            let degrees_digits = if is_lat { 2 } else { 3 };
            let degrees: f64 = digits[0..degrees_digits].parse().ok()?;
            let hundredths: f64 = digits[degrees_digits..].parse().ok()?;
            degrees + (hundredths / 100.0)
        }
        6 => {
            let degrees_digits = if is_lat { 2 } else { 3 };
            let minutes_digits = digits.len() - degrees_digits;

            let degrees: f64 = digits[0..degrees_digits].parse().ok()?;
            let minutes: f64 = digits[degrees_digits..degrees_digits + minutes_digits]
                .parse()
                .ok()?;
            degrees + (minutes / 60.0)
        }
        8 => {
            let degrees_digits = if is_lat { 2 } else { 3 };
            let whole_minutes_digits = 2;
            let hundredths_digits = digits.len() - degrees_digits - whole_minutes_digits;

            let degrees: f64 = digits[0..degrees_digits].parse().ok()?;
            let whole_minutes: f64 = digits[degrees_digits..degrees_digits + whole_minutes_digits]
                .parse()
                .ok()?;
            let hundredths: f64 = digits[degrees_digits + whole_minutes_digits
                ..degrees_digits + whole_minutes_digits + hundredths_digits]
                .parse()
                .ok()?;
            degrees + ((whole_minutes + (hundredths / 100.0)) / 60.0)
        }
        _ => return None,
    };

    Some(if negative { -value } else { value })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_time_mot_loc_point() {
        let text = "TIME...MOT...LOC 2310Z 238DEG 39KT 3221 08853";
        let entries = parse_time_mot_loc_entries(text);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].time_utc, "2310Z");
        assert_eq!(entries[0].direction_degrees, 238);
        assert_eq!(entries[0].speed_kt, 39);
        assert_eq!(entries[0].points.len(), 1);
        assert!(entries[0].wkt.starts_with("POINT("));
    }

    #[test]
    fn parse_time_mot_loc_linestring() {
        let text = "TIME...MOT...LOC 2359Z 332DEG 25KT 3704 9736 3699 9720";
        let entries = parse_time_mot_loc_entries(text);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].points.len(), 2);
        assert!(entries[0].wkt.starts_with("LINESTRING("));
    }

    #[test]
    fn parse_time_mot_loc_invalid_reports_issue() {
        let text = "TIME...MOT...LOC 2359Z 332DEG 25KT 3704";
        let (entries, issues) = parse_time_mot_loc_entries_with_issues(text);

        assert!(entries.is_empty());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "invalid_time_mot_loc_coordinate_count");
    }

    #[test]
    fn parse_time_mot_loc_wrapped_across_lines() {
        let text = "TIME...MOT...LOC 2310Z 238DEG 39KT\r\n3221 08853 3225 08849\n";
        let entries = parse_time_mot_loc_entries(text);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].points.len(), 2);
    }

    #[test]
    fn parse_time_mot_loc_with_split_coordinate_token() {
        let text = "TIME...MOT...LOC 2310Z 238DEG 39KT 3221 088\r\n53 3225 08849\n";
        let entries = parse_time_mot_loc_entries(text);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].points.len(), 2);
        assert!((entries[0].points[0].1 + 88.53).abs() < 0.01);
    }
}
