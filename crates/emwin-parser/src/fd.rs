//! Minimal FD winds/temps aloft bulletin parsing.

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

use crate::time::resolve_day_time_nearest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FdBulletin {
    pub based_on_time: String,
    pub valid_time: String,
    pub levels: Vec<u32>,
    pub forecasts: Vec<FdForecast>,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FdForecast {
    pub station: String,
    pub groups: Vec<FdLevelForecast>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FdLevelForecast {
    pub altitude_hundreds_ft: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_direction_degrees: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_speed_kt: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<i16>,
}

pub(crate) fn parse_fd_bulletin(
    text: &str,
    routing_code: Option<&str>,
    reference_time: DateTime<Utc>,
) -> Option<FdBulletin> {
    let lines = normalized_lines(text);
    let raw = lines.join("\n");
    let based_on_time = based_on_re()
        .captures(&raw)?
        .name("time")?
        .as_str()
        .to_string();
    let valid_time = valid_re()
        .captures(&raw)?
        .name("time")?
        .as_str()
        .to_string();
    let _ = parse_ddhhmmz(reference_time, &based_on_time)?;
    let _ = parse_ddhhmmz(reference_time, &valid_time)?;

    let level_line = lines.iter().find(|line| line.starts_with("FT "))?;
    let levels = parse_levels(level_line)?;
    if levels.is_empty() {
        return None;
    }

    let mut forecasts = Vec::new();
    let mut seen_level_row = false;

    for line in &lines {
        if line.starts_with("FT ") {
            seen_level_row = true;
            continue;
        }
        if !seen_level_row || line.len() < 4 {
            continue;
        }
        let Some(forecast) = parse_forecast_line(line, &levels, routing_code) else {
            continue;
        };
        if !forecast.groups.is_empty() {
            forecasts.push(forecast);
        }
    }

    if forecasts.is_empty() {
        return None;
    }

    Some(FdBulletin {
        based_on_time,
        valid_time,
        levels,
        forecasts,
        raw,
    })
}

fn parse_ddhhmmz(reference_time: DateTime<Utc>, value: &str) -> Option<DateTime<Utc>> {
    if value.len() != 7 || !value.ends_with('Z') {
        return None;
    }
    let day: u32 = value[0..2].parse().ok()?;
    let hour: u32 = value[2..4].parse().ok()?;
    let minute: u32 = value[4..6].parse().ok()?;
    let resolved = resolve_day_time_nearest(reference_time, day, hour, minute)?;
    (resolved
        .signed_duration_since(reference_time)
        .num_days()
        .abs()
        <= 5)
        .then_some(resolved)
}

fn parse_levels(line: &str) -> Option<Vec<u32>> {
    let levels = line
        .split_whitespace()
        .skip(1)
        .map(|token| token.parse().ok())
        .collect::<Option<Vec<u32>>>()?;
    Some(levels)
}

fn parse_forecast_line(
    line: &str,
    levels: &[u32],
    routing_code: Option<&str>,
) -> Option<FdForecast> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 2 {
        return None;
    }

    let station = normalize_station(tokens[0], routing_code);
    let encoded = tokens.iter().skip(1).copied().collect::<Vec<_>>();
    let take_count = encoded.len().min(levels.len());
    if take_count == 0 {
        return None;
    }

    let start = levels.len() - take_count;
    let mut groups = Vec::with_capacity(take_count);
    for (level, token) in levels[start..].iter().zip(encoded.iter()) {
        let decoded = decode_group(token)?;
        groups.push(FdLevelForecast {
            altitude_hundreds_ft: *level,
            wind_direction_degrees: decoded.wind_direction_degrees,
            wind_speed_kt: decoded.wind_speed_kt,
            temperature_c: decoded.temperature_c,
        });
    }

    Some(FdForecast { station, groups })
}

fn normalized_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(strip_control_chars)
        .map(|line| line.trim_end().to_string())
        .filter(|line| !line.trim().is_empty())
        .collect()
}

fn strip_control_chars(line: &str) -> String {
    line.chars()
        .filter(|ch| !ch.is_ascii_control() || ch.is_ascii_whitespace())
        .collect()
}

fn normalize_station(raw: &str, routing_code: Option<&str>) -> String {
    if raw.len() >= 4 {
        return raw.to_string();
    }

    let country = routing_code
        .and_then(|value| value.get(3..5))
        .unwrap_or_default()
        .to_ascii_uppercase();

    let prefix = match country.as_str() {
        "US" => "K",
        "CN" => "C",
        _ => "P",
    };
    format!("{prefix}{raw}")
}

struct DecodedGroup {
    wind_direction_degrees: Option<u16>,
    wind_speed_kt: Option<u16>,
    temperature_c: Option<i16>,
}

fn decode_group(text: &str) -> Option<DecodedGroup> {
    if !matches!(text.len(), 4 | 6 | 7)
        || !text
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '+' || ch == '-')
    {
        return None;
    }

    let mut wind_direction = text[0..2].parse::<u16>().ok()?.saturating_mul(10);
    let mut wind_speed = text[2..4].parse::<u16>().ok()?;
    let mut temperature = None;

    if text.starts_with("9900") {
        wind_direction = 0;
        wind_speed = 0;
    }

    if wind_direction >= 500 {
        wind_direction -= 500;
        wind_speed = wind_speed.saturating_add(100);
    }

    if text.len() > 4 {
        let magnitude = text[text.len() - 2..].parse::<i16>().ok()?;
        let signed_negative = text.as_bytes().get(4).copied() == Some(b'-') || text.len() == 6;
        temperature = Some(if signed_negative {
            -magnitude
        } else {
            magnitude
        });
    }

    Some(DecodedGroup {
        wind_direction_degrees: Some(wind_direction),
        wind_speed_kt: Some(wind_speed),
        temperature_c: temperature,
    })
}

fn based_on_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)^DATA BASED ON (?P<time>\d{6}Z)\s*$").expect("fd based-on regex compiles")
    })
}

fn valid_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)^VALID (?P<time>\d{6}Z)\b").expect("fd valid regex compiles")
    })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::{decode_group, parse_fd_bulletin};

    #[test]
    fn decodes_calm_and_high_speed_groups() {
        let calm = decode_group("9900").expect("calm should decode");
        assert_eq!(calm.wind_direction_degrees, Some(0));
        assert_eq!(calm.wind_speed_kt, Some(0));
        assert_eq!(calm.temperature_c, None);

        let fast = decode_group("790261").expect("high-speed group should decode");
        assert_eq!(fast.wind_direction_degrees, Some(290));
        assert_eq!(fast.wind_speed_kt, Some(102));
        assert_eq!(fast.temperature_c, Some(-61));
    }

    #[test]
    fn parses_fd_bulletin() {
        let text = "DATA BASED ON 090000Z\nVALID 090600Z FOR USE 0200-0900Z.\n\nFT 3000 6000 9000 12000\nBFF 1424 2414-08 2428-10 2342-27\n";
        let bulletin = parse_fd_bulletin(
            text,
            Some("FD1US1"),
            chrono::Utc
                .with_ymd_and_hms(2023, 3, 9, 1, 59, 0)
                .single()
                .expect("valid time"),
        )
        .expect("fd bulletin should parse");

        assert_eq!(bulletin.based_on_time, "090000Z");
        assert_eq!(bulletin.valid_time, "090600Z");
        assert_eq!(bulletin.forecasts[0].station, "KBFF");
        assert_eq!(bulletin.forecasts[0].groups[0].altitude_hundreds_ft, 3000);
        assert_eq!(
            bulletin.forecasts[0].groups[0].wind_direction_degrees,
            Some(140)
        );
    }

    #[test]
    fn rejects_invalid_timestamp_distance() {
        let text =
            "DATA BASED ON 090000Z\nVALID 090600Z FOR USE 0200-0900Z.\n\nFT 3000\nBFF 1424\n";
        assert!(
            parse_fd_bulletin(
                text,
                Some("FD1US1"),
                chrono::Utc
                    .with_ymd_and_hms(2023, 2, 19, 1, 59, 0)
                    .single()
                    .expect("valid time"),
            )
            .is_none()
        );
    }
}
