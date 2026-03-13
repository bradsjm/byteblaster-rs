//! Parsing for daily climate report (CLI) products.

use chrono::NaiveDate;
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CliBulletin {
    pub reports: Vec<CliReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CliReport {
    pub station: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_maximum: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_minimum: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_maximum_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_minimum_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precip_today_inches: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precip_month_inches: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precip_year_inches: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snow_today_inches: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snow_month_inches: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snow_season_inches: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snow_depth_inches: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_sky_cover: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_wind_speed_mph: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resultant_wind_speed_mph: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resultant_wind_direction_degrees: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highest_wind_speed_mph: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highest_wind_direction_degrees: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highest_gust_speed_mph: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highest_gust_direction_degrees: Option<u16>,
    pub raw: String,
}

pub(crate) fn parse_cli_bulletin(text: &str) -> Option<CliBulletin> {
    let normalized = text.replace('\r', "");
    let reports = split_sections(&normalized)
        .into_iter()
        .filter_map(|section| parse_cli_section(&section))
        .collect::<Vec<_>>();
    (!reports.is_empty()).then_some(CliBulletin { reports })
}

fn split_sections(text: &str) -> Vec<String> {
    let mut sections = Vec::new();
    let mut current = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed == "&&" {
            if !current.is_empty() {
                sections.push(current.join("\n"));
                current.clear();
            }
            continue;
        }
        if headline_re().is_match(trimmed) && !current.is_empty() {
            sections.push(current.join("\n"));
            current.clear();
        }
        current.push(trimmed.to_string());
    }
    if !current.is_empty() {
        sections.push(current.join("\n"));
    }
    sections
}

fn parse_cli_section(section: &str) -> Option<CliReport> {
    let headline = headline_re().captures(section)?;
    let station = headline.name("station")?.as_str().trim().replace("  ", " ");
    let valid_date = parse_headline_date(headline.name("date")?.as_str());

    Some(CliReport {
        station,
        valid_date,
        temperature_maximum: parse_integer_metric(section, "MAXIMUM TEMPERATURE"),
        temperature_minimum: parse_integer_metric(section, "MINIMUM TEMPERATURE"),
        temperature_maximum_time: parse_time_metric(section, "MAXIMUM TEMPERATURE"),
        temperature_minimum_time: parse_time_metric(section, "MINIMUM TEMPERATURE"),
        precip_today_inches: parse_float_metric(section, "PRECIPITATION", &["TODAY", "YESTERDAY"]),
        precip_month_inches: parse_float_metric(section, "PRECIPITATION", &["MONTH TO DATE"]),
        precip_year_inches: parse_float_metric(section, "PRECIPITATION", &["SINCE JANUARY 1ST"]),
        snow_today_inches: parse_float_metric(section, "SNOWFALL", &["TODAY", "YESTERDAY"]),
        snow_month_inches: parse_float_metric(section, "SNOWFALL", &["MONTH TO DATE"]),
        snow_season_inches: parse_float_metric(section, "SNOWFALL", &["SINCE JULY 1ST"]),
        snow_depth_inches: parse_float_metric(section, "SNOW DEPTH", &[]),
        average_sky_cover: parse_float_metric(section, "AVERAGE SKY COVER", &[]),
        average_wind_speed_mph: parse_float_metric(section, "AVERAGE WIND SPEED", &[]),
        resultant_wind_speed_mph: parse_float_metric(section, "RESULTANT WIND SPEED", &[]),
        resultant_wind_direction_degrees: parse_integer_metric(section, "RESULTANT WIND DIRECTION")
            .map(|value| value as u16),
        highest_wind_speed_mph: parse_float_metric(section, "HIGHEST WIND SPEED", &[]),
        highest_wind_direction_degrees: parse_integer_metric(section, "HIGHEST WIND DIRECTION")
            .map(|value| value as u16),
        highest_gust_speed_mph: parse_float_metric(section, "HIGHEST GUST SPEED", &[]),
        highest_gust_direction_degrees: parse_integer_metric(section, "HIGHEST GUST DIRECTION")
            .map(|value| value as u16),
        raw: section.trim().to_string(),
    })
}

fn parse_headline_date(text: &str) -> Option<String> {
    let cleaned = text.trim().replace(',', "");
    let date = NaiveDate::parse_from_str(&cleaned, "%B %d %Y")
        .or_else(|_| NaiveDate::parse_from_str(&cleaned, "%b %d %Y"))
        .ok()?;
    Some(date.format("%Y-%m-%d").to_string())
}

fn parse_integer_metric(section: &str, label: &str) -> Option<i16> {
    metric_line(section, label, &[])
        .and_then(find_numeric_token)
        .and_then(|value| value.parse::<i16>().ok())
}

fn parse_time_metric(section: &str, label: &str) -> Option<String> {
    let line = metric_line(section, label, &[])?;
    time_re()
        .captures(line)
        .and_then(|captures| captures.name("time"))
        .map(|value| value.as_str().to_string())
}

fn parse_float_metric(section: &str, label: &str, qualifiers: &[&str]) -> Option<f32> {
    metric_line(section, label, qualifiers)
        .and_then(find_numeric_token)
        .and_then(parse_float_token)
}

fn metric_line<'a>(section: &'a str, label: &str, qualifiers: &[&str]) -> Option<&'a str> {
    section.lines().map(str::trim).find(|line| {
        let upper = line.to_ascii_uppercase();
        upper.starts_with(label)
            && (qualifiers.is_empty()
                || qualifiers.iter().any(|qualifier| upper.contains(qualifier)))
    })
}

fn find_numeric_token(line: &str) -> Option<&str> {
    line.split_whitespace().find(|token| {
        token
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+'))
    })
}

fn parse_float_token(token: &str) -> Option<f32> {
    match token {
        "M" | "MM" => None,
        "T" => Some(0.0),
        other => other.parse::<f32>().ok(),
    }
}

fn headline_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\.\.\.THE (?P<station>[A-Z0-9\.\-()/,\s]+?) CLIMATE SUMMARY (?:FOR|FROM)\s+(?P<date>[A-Z][A-Z]+\s+\d{1,2}\s+\d{4})",
        )
        .expect("valid cli headline regex")
    })
}

fn time_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?P<time>\d{1,2}:\d{2}\s*[AP]M)").expect("valid cli time regex"))
}

#[cfg(test)]
mod tests {
    use super::parse_cli_bulletin;

    #[test]
    fn parses_basic_cli_section() {
        let text = "\
...THE DES MOINES CLIMATE SUMMARY FOR MARCH 10 2026...

WEATHER ITEM   OBSERVED TIME   RECORD YEAR NORMAL DEPARTURE LAST
MAXIMUM TEMPERATURE         72   3:52 PM   78  1894  48       24       61
MINIMUM TEMPERATURE         41   6:05 AM   18  1948  29       12       28
PRECIPITATION (TODAY)     0.10
PRECIPITATION MONTH TO DATE 0.42
SNOWFALL (TODAY)          0.0
AVERAGE WIND SPEED        12.5
HIGHEST GUST SPEED        31";
        let bulletin = parse_cli_bulletin(text).expect("cli bulletin");
        assert_eq!(bulletin.reports.len(), 1);
        let report = &bulletin.reports[0];
        assert_eq!(report.station, "DES MOINES");
        assert_eq!(report.valid_date.as_deref(), Some("2026-03-10"));
        assert_eq!(report.temperature_maximum, Some(72));
        assert_eq!(report.temperature_minimum, Some(41));
        assert_eq!(report.precip_today_inches, Some(0.10));
        assert_eq!(report.average_wind_speed_mph, Some(12.5));
        assert_eq!(report.highest_gust_speed_mph, Some(31.0));
    }
}
