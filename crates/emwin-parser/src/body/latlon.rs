//! NWS LAT...LON polygon parsing module.
//!
//! LAT...LON blocks define geographic warning areas as polygons using
//! coordinate pairs. Coordinates use a 4-8 digit format representing
//! degrees and decimal minutes or decimal degrees.
//!
//! Format: `LAT...LON coordinate1 coordinate2 coordinate3 ...`
//!
//! Examples:
//! - `LAT...LON 4400 8900 4500 8800 4600 8900` (4-digit format)
//! - `LAT...LON 440012 890056` (6-digit format)
//! - `LAT...LON 44001234 89005678` (8-digit format)

use regex::Regex;
use std::sync::OnceLock;

use crate::ProductParseIssue;

/// A parsed LAT...LON polygon.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LatLonPolygon {
    /// Coordinate pairs as (latitude, longitude) in decimal degrees
    pub points: Vec<(f64, f64)>,
    /// PostGIS WKT (Well-Known Text) representation
    pub wkt: String,
}

/// Parses all LAT...LON polygons found in the given text.
///
/// This function searches for LAT...LON blocks throughout the entire text and
/// returns all valid polygons found. Invalid or malformed coordinate blocks
/// are skipped.
///
/// # Coordinate Formats
///
/// - 4 digits: `DDMM` format → `DD.MM` degrees
/// - 6 digits: `DDMMSS` format → `DD.MMSS` degrees
/// - 8 digits: `DDMMSSxx` format → `DD.MMSSxx` degrees (hundredths of seconds)
///
/// PGUM (Guam) office uses east longitude (positive values).
///
/// # Arguments
///
/// * `text` - The text to search for LAT...LON polygons
///
/// # Returns
///
/// A vector of parsed `LatLonPolygon` structs. Returns an empty vector if no
/// valid polygons are found.
///
/// # Examples
///
/// ```
/// use emwin_parser::parse_latlon_polygons;
///
/// let text = "LAT...LON 4400 8900 4500 8800 4600 8900";
/// let polygons = parse_latlon_polygons(text);
///
/// assert_eq!(polygons.len(), 1);
/// assert_eq!(polygons[0].points.len(), 4); // 3 points + 1 closed
/// assert!((polygons[0].points[0].0 - 44.0).abs() < 0.01);
/// ```
pub fn parse_latlon_polygons(text: &str) -> Vec<LatLonPolygon> {
    parse_latlon_polygons_with_issues(text).0
}

pub fn parse_latlon_polygons_with_issues(
    text: &str,
) -> (Vec<LatLonPolygon>, Vec<ProductParseIssue>) {
    let mut polygons = Vec::new();
    let mut issues = Vec::new();

    for candidate in latlon_candidate_regex().find_iter(text) {
        let raw = candidate.as_str().trim();
        let Some(captures) = latlon_regex().captures(raw) else {
            issues.push(ProductParseIssue::new(
                "latlon_parse",
                "invalid_latlon_format",
                format!("could not parse LAT...LON block: `{raw}`"),
                Some(raw.to_string()),
            ));
            continue;
        };

        match parse_latlon_capture(&captures, raw) {
            Ok(polygon) => polygons.push(polygon),
            Err(issue) => issues.push(issue),
        }
    }

    (polygons, issues)
}

fn latlon_candidate_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)LAT\.\.\.LON(?:\s+-?\d{1,8})+").expect("latlon candidate regex compiles")
    })
}

fn latlon_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Match LAT...LON followed by coordinate pairs
        // Pattern allows trailing whitespace or end of string after last coordinate
        Regex::new(r"(?i)LAT\.\.\.LON\s+((?:-?\d{4,8}\s*)+)").expect("latlon regex compiles")
    })
}

fn parse_latlon_capture(
    cap: &regex::Captures<'_>,
    raw: &str,
) -> Result<LatLonPolygon, ProductParseIssue> {
    let coords_str = cap.get(1).map(|value| value.as_str()).ok_or_else(|| {
        ProductParseIssue::new(
            "latlon_parse",
            "invalid_latlon_format",
            format!("could not parse LAT...LON block: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;
    let coords: Vec<&str> = coords_str.split_whitespace().collect();

    if coords.len() < 6 || !coords.len().is_multiple_of(2) {
        return Err(ProductParseIssue::new(
            "latlon_parse",
            "invalid_latlon_coordinate_count",
            format!("LAT...LON block has an invalid coordinate count: `{raw}`"),
            Some(raw.to_string()),
        ));
    }

    let mut points = Vec::with_capacity(coords.len() / 2);

    for chunk in coords.chunks(2) {
        let lat = parse_coordinate(chunk[0], true).ok_or_else(|| {
            ProductParseIssue::new(
                "latlon_parse",
                "invalid_latlon_latitude",
                format!("could not parse LAT...LON latitude from block: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;
        let lon = parse_coordinate(chunk[1], false).ok_or_else(|| {
            ProductParseIssue::new(
                "latlon_parse",
                "invalid_latlon_longitude",
                format!("could not parse LAT...LON longitude from block: `{raw}`"),
                Some(raw.to_string()),
            )
        })?;
        points.push((lat, lon));
    }

    // Ensure polygon is closed (first point == last point)
    if points.len() > 2 && points[0] != *points.last().unwrap() {
        points.push(points[0]);
    }

    let wkt = format_wkt(&points);

    Ok(LatLonPolygon { points, wkt })
}

fn parse_coordinate(coord: &str, _is_latitude: bool) -> Option<f64> {
    let coord = coord.trim();

    // Handle negative values (already decimal)
    if coord.starts_with('-') {
        return coord.parse().ok();
    }

    match coord.len() {
        4 => {
            // DDMM format: DD + MM/60
            let degrees: f64 = coord[0..2].parse().ok()?;
            let minutes: f64 = coord[2..4].parse().ok()?;
            Some(degrees + minutes / 60.0)
        }
        5 => {
            // DDMMM format (rare): DD + MMM/1000 as decimal degrees
            let degrees: f64 = coord[0..2].parse().ok()?;
            let decimal: f64 = coord[2..5].parse().ok()?;
            Some(degrees + decimal / 1000.0)
        }
        6 => {
            // DDMMSS format: DD + MM/60 + SS/3600
            let degrees: f64 = coord[0..2].parse().ok()?;
            let minutes: f64 = coord[2..4].parse().ok()?;
            let seconds: f64 = coord[4..6].parse().ok()?;
            Some(degrees + minutes / 60.0 + seconds / 3600.0)
        }
        7 => {
            // DDMMMMM format (rare): DD + MMMMM/100000 as decimal degrees
            let degrees: f64 = coord[0..2].parse().ok()?;
            let decimal: f64 = coord[2..7].parse().ok()?;
            Some(degrees + decimal / 100000.0)
        }
        8 => {
            // DDMMSSxx format (hundredths of seconds): DD + MM/60 + SS.xx/3600
            let degrees: f64 = coord[0..2].parse().ok()?;
            let minutes: f64 = coord[2..4].parse().ok()?;
            let seconds: f64 = coord[4..6].parse().ok()?;
            let hundredths: f64 = coord[6..8].parse().ok()?;
            Some(degrees + minutes / 60.0 + (seconds + hundredths / 100.0) / 3600.0)
        }
        _ => {
            // Try direct decimal parse
            coord.parse().ok()
        }
    }
}

fn format_wkt(points: &[(f64, f64)]) -> String {
    let coords: Vec<String> = points
        .iter()
        .map(|(lat, lon)| format!("{:.6} {:.6}", lon, lat))
        .collect();

    format!("POLYGON(({}))", coords.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_polygon_4digit() {
        let text = "LAT...LON 4400 8900 4500 8800 4600 8900 4400 8900";
        let polygons = parse_latlon_polygons(text);

        assert_eq!(polygons.len(), 1);
        assert_eq!(polygons[0].points.len(), 4);

        // 4400 → 44.0°
        assert!((polygons[0].points[0].0 - 44.0).abs() < 0.01);
        assert!((polygons[0].points[0].1 - 89.0).abs() < 0.01);

        // 4500 → 45.0°
        assert!((polygons[0].points[1].0 - 45.0).abs() < 0.01);
        assert!((polygons[0].points[1].1 - 88.0).abs() < 0.01);
    }

    #[test]
    fn parse_polygon_with_minutes() {
        // 4-digit format: DDMM (degrees, minutes)
        let text = "LAT...LON 4430 8930 4530 8830 4430 8930";
        let polygons = parse_latlon_polygons(text);

        assert_eq!(polygons.len(), 1);
        // 4430 → 44 + 30/60 = 44.5°
        assert!((polygons[0].points[0].0 - 44.5).abs() < 0.01);
        assert!((polygons[0].points[0].1 - 89.5).abs() < 0.01);
    }

    #[test]
    fn parse_polygon_6digit() {
        // 6-digit format: DDMMSS (degrees, minutes, seconds)
        let text = "LAT...LON 443012 893056 443015 893100 443012 893056";
        let polygons = parse_latlon_polygons(text);

        assert_eq!(polygons.len(), 1);
        // 443012 → 44 + 30/60 + 12/3600 ≈ 44.5033°
        let expected_lat = 44.0 + 30.0 / 60.0 + 12.0 / 3600.0;
        assert!((polygons[0].points[0].0 - expected_lat).abs() < 0.0001);
    }

    #[test]
    fn parse_polygon_8digit() {
        // 8-digit format: DDMMSSxx (degrees, minutes, seconds, hundredths)
        let text = "LAT...LON 44301234 89305678 44301240 89305680 44301234 89305678";
        let polygons = parse_latlon_polygons(text);

        assert_eq!(polygons.len(), 1);
        // 44301234 → 44 + 30/60 + (12.34)/3600
        let expected_lat = 44.0 + 30.0 / 60.0 + 12.34 / 3600.0;
        assert!((polygons[0].points[0].0 - expected_lat).abs() < 0.00001);
    }

    #[test]
    fn polygon_auto_closes() {
        // 3 points provided, but should be closed (4 points after)
        let text = "LAT...LON 4400 8900 4500 8800 4600 8900";
        let polygons = parse_latlon_polygons(text);

        assert_eq!(polygons[0].points.len(), 4);
        assert_eq!(polygons[0].points[0], polygons[0].points[3]);
    }

    #[test]
    fn wkt_format() {
        let text = "LAT...LON 4400 8900 4500 8800 4400 8900";
        let polygons = parse_latlon_polygons(text);

        assert!(polygons[0].wkt.starts_with("POLYGON(("));
        assert!(polygons[0].wkt.contains("89.000000 44.000000"));
        assert!(polygons[0].wkt.ends_with("))"));
    }

    #[test]
    fn parse_latlon_case_insensitive() {
        let text = "lat...lon 4400 8900 4500 8800 4400 8900";
        let polygons = parse_latlon_polygons(text);
        assert_eq!(polygons.len(), 1);
    }

    #[test]
    fn parse_latlon_empty() {
        let polygons = parse_latlon_polygons("");
        assert!(polygons.is_empty());
    }

    #[test]
    fn parse_latlon_invalid_skipped() {
        let text = "LAT...LON 4400"; // Too few coordinates
        let polygons = parse_latlon_polygons(text);
        assert!(polygons.is_empty());
    }

    #[test]
    fn parse_latlon_invalid_reports_issue() {
        let text = "LAT...LON 4400";
        let (polygons, issues) = parse_latlon_polygons_with_issues(text);

        assert!(polygons.is_empty());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "invalid_latlon_coordinate_count");
    }

    #[test]
    fn parse_latlon_odd_coordinates() {
        let text = "LAT...LON 4400 8900 4500"; // Odd number
        let polygons = parse_latlon_polygons(text);
        assert!(polygons.is_empty());
    }

    #[test]
    fn parse_multiple_polygons() {
        let text = concat!(
            "LAT...LON 4400 8900 4500 8800 4400 8900",
            " some text ",
            "LAT...LON 4600 8700 4700 8600 4600 8700"
        );
        let polygons = parse_latlon_polygons(text);

        assert_eq!(polygons.len(), 2);
    }
}
