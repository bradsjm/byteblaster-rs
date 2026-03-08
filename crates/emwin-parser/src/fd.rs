//! Minimal FD winds/temps aloft bulletin parsing.
//!
//! FD (Forecast Winds and Temperatures Aloft) bulletins contain forecast data
//! for wind direction, wind speed, and temperature at standard pressure altitudes.
//! These forecasts are used for flight planning and aviation weather analysis.
//!
//! ## FD Bulletin Format
//!
//! - Header lines: "DATA BASED ON DDHHMMZ" and "VALID DDHHMMZ"
//! - Level line: "FT alt1 alt2 alt3 ..." (altitudes in hundreds of feet)
//! - Data lines: "STATION dirspd dirspd ..." where dirspd encodes wind and temp
//!
//! ## Wind/Temperature Encoding
//!
//! - 4-digit codes: `DDSS` -> direction (DD*10), speed (SS) in knots
//! - 6-digit codes: `DDSSXX` -> direction, speed, temperature in Celsius
//! - Special code 9900 indicates calm winds (0° direction, 0 kt speed)
//! - Direction >= 500 indicates speed > 100 knots (subtract 500 from direction, add 100 to speed)

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

use crate::time::resolve_day_time_nearest;

/// FD bulletin containing winds and temperatures aloft forecasts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FdBulletin {
    /// Data based on timestamp (DDHHMMZ format)
    pub based_on_time: String,
    /// Validity timestamp (DDHHMMZ format)
    pub valid_time: String,
    /// Altitude levels in hundreds of feet (e.g., [30, 60, 90] for 3000, 6000, 9000 ft)
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
    /// Altitude in hundreds of feet
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

/// Parses an FD bulletin from text content.
///
/// Extracts the data timestamps, altitude levels, and station forecasts.
/// Validates that timestamps are within 5 days of the reference time.
///
/// # Arguments
///
/// * `text` - Raw FD bulletin text
/// * `routing_code` - Optional routing code for station prefix inference (e.g., "FD1US1")
/// * `reference_time` - Reference UTC time for timestamp validation
///
/// # Returns
///
/// `Some(FdBulletin)` if parsing succeeds, `None` if validation fails
pub(crate) fn parse_fd_bulletin(
    text: &str,
    routing_code: Option<&str>,
    reference_time: DateTime<Utc>,
) -> Option<FdBulletin> {
    let lines = normalized_lines(text);
    let normalized = lines.join("\n");
    let based_on_time = based_on_re()
        .captures(&normalized)?
        .name("time")?
        .as_str()
        .to_string();
    let valid_time = valid_re()
        .captures(&normalized)?
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
    })
}

/// Parses DDHHMMZ timestamp and validates it is within 5 days of reference.
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

/// Parses the FT line to extract altitude levels.
///
/// Input: "FT 3000 6000 9000 12000" -> [30, 60, 90, 120]
fn parse_levels(line: &str) -> Option<Vec<u32>> {
    let levels = line
        .split_whitespace()
        .skip(1)
        .map(|token| token.parse().ok())
        .collect::<Option<Vec<u32>>>()?;
    Some(levels)
}

/// Parses a single station forecast line.
///
/// Extracts the station identifier and decodes wind/temperature groups
/// for each altitude level.
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

/// Normalizes text by stripping control characters and empty lines.
fn normalized_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(strip_control_chars)
        .map(|line| line.trim_end().to_string())
        .filter(|line| !line.trim().is_empty())
        .collect()
}

/// Removes non-whitespace control characters from a line.
fn strip_control_chars(line: &str) -> String {
    line.chars()
        .filter(|ch| !ch.is_ascii_control() || ch.is_ascii_whitespace())
        .collect()
}

/// Normalizes a station identifier to ICAO format.
///
/// 3-letter codes get a country prefix based on routing code:
/// - US -> K prefix (e.g., "BOS" -> "KBOS")
/// - CN -> C prefix
/// - Other -> P prefix
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

/// Decoded wind/temperature data from an FD group.
struct DecodedGroup {
    wind_direction_degrees: Option<u16>,
    wind_speed_kt: Option<u16>,
    temperature_c: Option<i16>,
}

/// Decodes a wind/temperature group from FD format.
///
/// Format variations:
/// - 4 digits (DDSS): direction (degrees/10), speed (knots)
/// - 6 digits (DDSSXX): direction, speed, temperature (Celsius)
/// - 7 digits (DDSS+XX or DDSS-XX): direction, speed, signed temperature
/// - 9900: calm (0°, 0 kt)
/// - Direction >= 500: subtract 500, add 100 to speed
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

/// Regex for "DATA BASED ON" timestamp extraction.
fn based_on_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)^DATA BASED ON (?P<time>\d{6}Z)\s*$").expect("fd based-on regex compiles")
    })
}

/// Regex for "VALID" timestamp extraction.
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
}
