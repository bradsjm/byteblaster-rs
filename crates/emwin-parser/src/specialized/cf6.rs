//! Parsing for CF6 climate products.

use serde::Serialize;

use crate::ProductParseIssue;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cf6Bulletin {
    pub station: String,
    pub month: u8,
    pub year: i32,
    pub rows: Vec<Cf6DayRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cf6DayRow {
    pub day: u8,
    pub max_temp_f: Option<i16>,
    pub min_temp_f: Option<i16>,
    pub avg_temp_f: Option<i16>,
    pub departure_f: Option<i16>,
    pub heating_degree_days: Option<i16>,
    pub cooling_degree_days: Option<i16>,
    pub precip_inches: Option<f32>,
    pub snow_inches: Option<f32>,
    pub snow_depth_inches: Option<f32>,
    pub avg_wind_mph: Option<f32>,
    pub max_wind_mph: Option<i16>,
    pub avg_wind_dir_degrees: Option<u16>,
    pub minutes_sunshine: Option<u16>,
    pub possible_sunshine_minutes: Option<u16>,
    pub sky_cover: Option<String>,
    pub weather_codes: Option<String>,
    pub gust_mph: Option<i16>,
    pub gust_dir_degrees: Option<u16>,
}

pub(crate) fn parse_cf6_bulletin(text: &str) -> Option<(Cf6Bulletin, Vec<ProductParseIssue>)> {
    let normalized = text.replace('\r', "");
    let station = normalized
        .lines()
        .find(|line| line.contains("STATION:"))?
        .split(':')
        .nth(1)?
        .trim()
        .to_string();
    let month = parse_month(
        normalized
            .lines()
            .find(|line| line.trim_start().starts_with("MONTH:"))?
            .split(':')
            .nth(1)?
            .trim(),
    )?;
    let year = normalized
        .lines()
        .find(|line| line.trim_start().starts_with("YEAR:"))?
        .split(':')
        .nth(1)?
        .trim()
        .parse::<i32>()
        .ok()?;
    let mut rows = Vec::new();
    let mut issues = Vec::new();
    let mut in_table = false;
    for line in normalized.lines() {
        if line.trim_start().starts_with("DY MAX") {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        let trimmed = line.trim_end();
        if trimmed.starts_with("===") || trimmed.starts_with("SM ") || trimmed.starts_with("AV ") {
            continue;
        }
        if !trimmed
            .trim_start()
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_digit())
        {
            continue;
        }
        let fields = trimmed.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 18 {
            continue;
        }
        let day = fields[0].parse::<u8>().ok()?;
        let (sky_cover_idx, weather_idx, gust_idx, gust_dir_idx) = if fields.len() >= 19 {
            (15, Some(16), 17, 18)
        } else {
            (15, None, 16, 17)
        };
        let mut trace_hit = false;
        let mut inches = |value: &str| -> Option<f32> {
            match value {
                "M" => None,
                "T" => {
                    trace_hit = true;
                    Some(0.0)
                }
                _ => value.parse::<f32>().ok(),
            }
        };
        rows.push(Cf6DayRow {
            day,
            max_temp_f: parse_i16(fields[1]),
            min_temp_f: parse_i16(fields[2]),
            avg_temp_f: parse_i16(fields[3]),
            departure_f: parse_i16(fields[4]),
            heating_degree_days: parse_i16(fields[5]),
            cooling_degree_days: parse_i16(fields[6]),
            precip_inches: inches(fields[7]),
            snow_inches: inches(fields[8]),
            snow_depth_inches: inches(fields[9]),
            avg_wind_mph: fields[10].parse::<f32>().ok(),
            max_wind_mph: parse_i16(fields[11]),
            avg_wind_dir_degrees: fields[12].parse::<u16>().ok(),
            minutes_sunshine: parse_u16(fields[13]),
            possible_sunshine_minutes: parse_u16(fields[14]),
            sky_cover: string_or_none(fields[sky_cover_idx]),
            weather_codes: weather_idx.and_then(|idx| string_or_none(fields[idx])),
            gust_mph: parse_i16(fields[gust_idx]),
            gust_dir_degrees: parse_u16(fields[gust_dir_idx]),
        });
        if trace_hit {
            issues.push(ProductParseIssue::new(
                "cf6_parse",
                "trace_value_flattened",
                "trace precipitation or snow value flattened to 0.0",
                Some(trimmed.to_string()),
            ));
        }
    }
    (!rows.is_empty()).then_some((
        Cf6Bulletin {
            station,
            month,
            year,
            rows,
        },
        issues,
    ))
}

fn parse_month(value: &str) -> Option<u8> {
    match value.to_ascii_uppercase().as_str() {
        "JANUARY" | "JAN" => Some(1),
        "FEBRUARY" | "FEB" => Some(2),
        "MARCH" | "MAR" => Some(3),
        "APRIL" | "APR" => Some(4),
        "MAY" => Some(5),
        "JUNE" | "JUN" => Some(6),
        "JULY" | "JUL" => Some(7),
        "AUGUST" | "AUG" => Some(8),
        "SEPTEMBER" | "SEP" => Some(9),
        "OCTOBER" | "OCT" => Some(10),
        "NOVEMBER" | "NOV" => Some(11),
        "DECEMBER" | "DEC" => Some(12),
        _ => None,
    }
}

fn parse_i16(value: &str) -> Option<i16> {
    if value == "M" {
        None
    } else {
        value.parse().ok()
    }
}

fn parse_u16(value: &str) -> Option<u16> {
    if value == "M" {
        None
    } else {
        value.parse().ok()
    }
}

fn string_or_none(value: &str) -> Option<String> {
    (value != "M" && !value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_cf6_bulletin;

    #[test]
    fn parses_local_cf6_rows() {
        let text = "\
PRELIMINARY LOCAL CLIMATOLOGICAL DATA
STATION: TEST STATION
MONTH: MARCH
YEAR: 2026
DY MAX MIN AVG DEP HDD CDD PCP SNW SND AWD MWD DIR MIN PSBL SKY WX GST GDR
 1 70 50 60 0 5 0 0.10 0.0 0 8.5 20 180 600 720 CLR RA 30 190";
        let (bulletin, issues) = parse_cf6_bulletin(text).expect("cf6 bulletin");
        assert_eq!(bulletin.station, "TEST STATION");
        assert_eq!(bulletin.rows.len(), 1);
        assert!(issues.is_empty());
    }
}
