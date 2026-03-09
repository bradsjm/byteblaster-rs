//! Minimal FD winds/temps aloft bulletin parsing.
//!
//! FD parsing is inherently line-structured, so this implementation keeps the
//! parser line-oriented instead of re-normalizing the whole bulletin into one
//! large string. That removes the old regex scan for header timestamps and
//! makes the required sections explicit.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::time::resolve_day_time_nearest;

/// FD bulletin containing winds and temperatures aloft forecasts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FdBulletin {
    /// Data based on timestamp (DDHHMMZ format)
    pub based_on_time: String,
    /// Validity timestamp (DDHHMMZ format)
    pub valid_time: String,
    /// Altitude levels in feet
    pub levels: Vec<u32>,
    /// Forecast entries by station
    pub forecasts: Vec<FdForecast>,
}

/// Forecast data for a single station.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FdForecast {
    /// Station identifier (ICAO code, e.g., "KBOS")
    pub station: String,
    /// Forecast values at each altitude level
    pub groups: Vec<FdLevelForecast>,
}

/// Forecast values at a specific altitude.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FdLevelForecast {
    /// Altitude in feet
    pub altitude_hundreds_ft: u32,
    /// Wind direction in degrees (0-360)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_direction_degrees: Option<u16>,
    /// Wind speed in knots
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_speed_kt: Option<u16>,
    /// Temperature in Celsius
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<i16>,
}

struct FdHeaderParts<'a> {
    based_on_time: &'a str,
    valid_time: &'a str,
    level_index: usize,
}

/// Parses an FD bulletin from text content.
pub(crate) fn parse_fd_bulletin(
    text: &str,
    routing_code: Option<&str>,
    reference_time: DateTime<Utc>,
) -> Option<FdBulletin> {
    let lines = iter_nonempty_fd_lines(text).collect::<Vec<_>>();
    let header = parse_fd_header(&lines, reference_time)?;
    let levels = parse_levels(lines[header.level_index])?;
    if levels.is_empty() {
        return None;
    }

    let mut forecasts = Vec::new();
    for line in &lines[header.level_index + 1..] {
        let Some(forecast) = parse_fd_station_line(line, &levels, routing_code) else {
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
        based_on_time: header.based_on_time.to_string(),
        valid_time: header.valid_time.to_string(),
        levels,
        forecasts,
    })
}

/// Iterates over non-empty FD lines after stripping control characters.
fn iter_nonempty_fd_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim).filter(|line| !line.is_empty())
}

/// Extracts the required FD header fields and the `FT` level row location.
fn parse_fd_header<'a>(
    lines: &'a [&'a str],
    reference_time: DateTime<Utc>,
) -> Option<FdHeaderParts<'a>> {
    let mut based_on_time = None;
    let mut valid_time = None;
    let mut level_index = None;

    for (index, line) in lines.iter().enumerate() {
        if let Some(time) = line.strip_prefix("DATA BASED ON ") {
            let time = time.split_whitespace().next()?;
            let _ = parse_ddhhmmz(reference_time, time)?;
            based_on_time = Some(time);
            continue;
        }
        if let Some(time) = line.strip_prefix("VALID ") {
            let time = time.split_whitespace().next()?;
            let _ = parse_ddhhmmz(reference_time, time)?;
            valid_time = Some(time);
            continue;
        }
        if line.starts_with("FT ") {
            level_index = Some(index);
            break;
        }
    }

    Some(FdHeaderParts {
        based_on_time: based_on_time?,
        valid_time: valid_time?,
        level_index: level_index?,
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
    let mut tokens = line.split_whitespace();
    (tokens.next()? == "FT").then_some(())?;
    tokens
        .map(|token| token.parse::<u32>().ok())
        .collect::<Option<Vec<u32>>>()
}

/// Parses a single station forecast line.
fn parse_fd_station_line(
    line: &str,
    levels: &[u32],
    routing_code: Option<&str>,
) -> Option<FdForecast> {
    let mut tokens = line.split_whitespace();
    let station = normalize_station(tokens.next()?, routing_code);
    let encoded = tokens.collect::<Vec<_>>();
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
        let json = serde_json::to_value(&bulletin).expect("fd bulletin serializes");
        assert!(json.get("raw").is_none());
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

    #[test]
    fn ignores_extra_blank_lines() {
        let text =
            "\nDATA BASED ON 090000Z\n\nVALID 090600Z FOR USE 0200-0900Z.\n\nFT 3000\n\nBFF 1424\n";
        assert!(
            parse_fd_bulletin(
                text,
                Some("FD1US1"),
                chrono::Utc
                    .with_ymd_and_hms(2023, 3, 9, 1, 59, 0)
                    .single()
                    .expect("valid time"),
            )
            .is_some()
        );
    }

    #[test]
    fn rejects_missing_header_fields() {
        assert!(
            parse_fd_bulletin(
                "VALID 090600Z\nFT 3000\nBFF 1424\n",
                Some("FD1US1"),
                chrono::Utc
                    .with_ymd_and_hms(2023, 3, 9, 1, 59, 0)
                    .single()
                    .expect("valid time"),
            )
            .is_none()
        );
    }
}
