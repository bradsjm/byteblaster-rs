//! NWS LAT...LON polygon parsing module.
//!
//! LAT...LON blocks define geographic warning areas as polygons using
//! coordinate pairs. Coordinates use a 4-8 digit format representing
//! decimal degrees or degrees/minutes/seconds variants.
//!
//! Format: `LAT...LON coordinate1 coordinate2 coordinate3 ...`
//!
//! Examples:
//! - `LAT...LON 4400 8900 4500 8800 4600 8900` (4-digit hundredths format)
//! - `LAT...LON 440012 890056` (6-digit format)
//! - `LAT...LON 44001234 89005678` (8-digit format)

use regex::Regex;
use serde::ser::{SerializeSeq, Serializer};
use std::sync::OnceLock;

use crate::ProductParseIssue;

/// A parsed LAT...LON polygon.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LatLonPolygon {
    /// Coordinate pairs as (latitude, longitude) in decimal degrees
    #[serde(serialize_with = "serialize_points")]
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
/// - 4 digits: hundredths of degrees (`4270` -> `42.70`)
/// - 5 digits: hundredths of degrees with leading zero support (`08449` -> `84.49`)
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
    let normalized = text.replace('\r', "").replace('\n', " ");

    for candidate in latlon_candidate_regex().find_iter(&normalized) {
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
        Regex::new(r"(?i)LAT\.\.\.LON\s+((?:-?\d{1,8}\s*)+)").expect("latlon regex compiles")
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
    let raw_coords: Vec<&str> = coords_str.split_whitespace().collect();
    let coords = normalize_coordinate_tokens(&raw_coords).ok_or_else(|| {
        ProductParseIssue::new(
            "latlon_parse",
            "invalid_latlon_coordinate_format",
            format!("could not normalize LAT...LON coordinates from block: `{raw}`"),
            Some(raw.to_string()),
        )
    })?;

    let points = parse_points(&coords, raw)?;

    // Ensure polygon is closed (first point == last point)
    if points.len() > 2 && points[0] != *points.last().unwrap() {
        let mut points = points;
        points.push(points[0]);
        let wkt = format_wkt(&points);
        return Ok(LatLonPolygon { points, wkt });
    }

    let wkt = format_wkt(&points);

    Ok(LatLonPolygon { points, wkt })
}

fn parse_points(coords: &[String], raw: &str) -> Result<Vec<(f64, f64)>, ProductParseIssue> {
    let mut points = Vec::with_capacity(coords.len() / 2);
    let mut pending_latitude: Option<f64> = None;

    for coord in coords {
        if coord.len() == 8 && !coord.starts_with('-') {
            if pending_latitude.is_some() {
                return Err(ProductParseIssue::new(
                    "latlon_parse",
                    "invalid_latlon_coordinate_count",
                    format!("LAT...LON block has an invalid coordinate count: `{raw}`"),
                    Some(raw.to_string()),
                ));
            }

            let lat = parse_coordinate(&coord[..4], true).ok_or_else(|| {
                ProductParseIssue::new(
                    "latlon_parse",
                    "invalid_latlon_latitude",
                    format!("could not parse LAT...LON latitude from block: `{raw}`"),
                    Some(raw.to_string()),
                )
            })?;
            let lon = parse_coordinate(&coord[4..], false).ok_or_else(|| {
                ProductParseIssue::new(
                    "latlon_parse",
                    "invalid_latlon_longitude",
                    format!("could not parse LAT...LON longitude from block: `{raw}`"),
                    Some(raw.to_string()),
                )
            })?;
            points.push((lat, lon));
            continue;
        }

        let value = parse_coordinate(coord, pending_latitude.is_none()).ok_or_else(|| {
            ProductParseIssue::new(
                "latlon_parse",
                if pending_latitude.is_none() {
                    "invalid_latlon_latitude"
                } else {
                    "invalid_latlon_longitude"
                },
                format!(
                    "could not parse LAT...LON {} from block: `{raw}`",
                    if pending_latitude.is_none() {
                        "latitude"
                    } else {
                        "longitude"
                    }
                ),
                Some(raw.to_string()),
            )
        })?;

        if let Some(lat) = pending_latitude.take() {
            points.push((lat, value));
        } else {
            pending_latitude = Some(value);
        }
    }

    if pending_latitude.is_some() || points.len() < 3 {
        return Err(ProductParseIssue::new(
            "latlon_parse",
            "invalid_latlon_coordinate_count",
            format!("LAT...LON block has an invalid coordinate count: `{raw}`"),
            Some(raw.to_string()),
        ));
    }

    Ok(points)
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

fn parse_coordinate(coord: &str, is_latitude: bool) -> Option<f64> {
    let coord = coord.trim();

    // Handle negative values (already decimal)
    if coord.starts_with('-') {
        return coord.parse().ok();
    }

    let value = match coord.len() {
        4 => {
            let hundredths: f64 = coord.parse().ok()?;
            hundredths / 100.0
        }
        5 => {
            let hundredths: f64 = coord.parse().ok()?;
            hundredths / 100.0
        }
        6 => {
            // DDMMSS format: DD + MM/60 + SS/3600
            let degrees: f64 = coord[0..2].parse().ok()?;
            let minutes: f64 = coord[2..4].parse().ok()?;
            let seconds: f64 = coord[4..6].parse().ok()?;
            degrees + minutes / 60.0 + seconds / 3600.0
        }
        7 => {
            // DDMMMMM format (rare): DD + MMMMM/100000 as decimal degrees
            let degrees: f64 = coord[0..2].parse().ok()?;
            let decimal: f64 = coord[2..7].parse().ok()?;
            degrees + decimal / 100000.0
        }
        8 => {
            // DDMMSSxx format (hundredths of seconds): DD + MM/60 + SS.xx/3600
            let degrees: f64 = coord[0..2].parse().ok()?;
            let minutes: f64 = coord[2..4].parse().ok()?;
            let seconds: f64 = coord[4..6].parse().ok()?;
            let hundredths: f64 = coord[6..8].parse().ok()?;
            degrees + minutes / 60.0 + (seconds + hundredths / 100.0) / 3600.0
        }
        _ => {
            // Try direct decimal parse
            coord.parse().ok()?
        }
    };

    Some(if is_latitude { value } else { -value })
}

fn serialize_points<S>(points: &[(f64, f64)], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut seq = serializer.serialize_seq(Some(points.len()))?;
    for (lat, lon) in points {
        seq.serialize_element(&[round_coordinate(*lat), round_coordinate(*lon)])?;
    }
    seq.end()
}

fn round_coordinate(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
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

        // 4400 -> 44.00 degrees
        assert!((polygons[0].points[0].0 - 44.0).abs() < 0.01);
        assert!((polygons[0].points[0].1 + 89.0).abs() < 0.01);

        // 4500 -> 45.00 degrees
        assert!((polygons[0].points[1].0 - 45.0).abs() < 0.01);
        assert!((polygons[0].points[1].1 + 88.0).abs() < 0.01);
    }

    #[test]
    fn parse_polygon_4digit_hundredths() {
        let text = "LAT...LON 4430 8930 4530 8830 4430 8930";
        let polygons = parse_latlon_polygons(text);

        assert_eq!(polygons.len(), 1);
        assert!((polygons[0].points[0].0 - 44.30).abs() < 0.01);
        assert!((polygons[0].points[0].1 + 89.30).abs() < 0.01);
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
        // 8-digit format: packed lat/lon pair in hundredths of degrees.
        let text = "LAT...LON 44308930 45308830 44308930";
        let polygons = parse_latlon_polygons(text);

        assert_eq!(polygons.len(), 1);
        assert!((polygons[0].points[0].0 - 44.30).abs() < 0.00001);
        assert!((polygons[0].points[0].1 + 89.30).abs() < 0.00001);
    }

    #[test]
    fn serializes_points_without_float_artifacts() {
        let text = "LAT...LON 4000 9802 4000 9822 4011 9817";
        let polygons = parse_latlon_polygons(text);

        let json = serde_json::to_value(&polygons[0]).expect("polygon serializes");
        assert_eq!(
            json["points"],
            serde_json::json!([
                [40.0, -98.02],
                [40.0, -98.22],
                [40.11, -98.17],
                [40.0, -98.02]
            ])
        );
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
        assert!(polygons[0].wkt.contains("-89.000000 44.000000"));
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

    #[test]
    fn parse_latlon_wrapped_across_lines() {
        let text = "LAT...LON 4143 9613 4145 9610\r\n4140 9608 4138 9612";
        let polygons = parse_latlon_polygons(text);

        assert_eq!(polygons.len(), 1);
        assert_eq!(polygons[0].points.len(), 5);
    }

    #[test]
    fn parse_latlon_with_split_coordinate_token() {
        let text = "LAT...LON 4143 96\r\n13 4145 9610 4140 9608";
        let polygons = parse_latlon_polygons(text);

        assert_eq!(polygons.len(), 1);
        assert_eq!(polygons[0].points.len(), 4);
        assert!((polygons[0].points[0].1 + 96.13).abs() < 0.01);
    }

    #[test]
    fn parse_latlon_svsgrrmi_polygon() {
        let text = "LAT...LON 4270 8449 4278 8454 4288 8437 4278 8436 4278 8416 4276 8416";
        let polygons = parse_latlon_polygons(text);

        assert_eq!(polygons.len(), 1);
        assert_eq!(polygons[0].points.len(), 7);
        assert_eq!(
            serde_json::to_value(&polygons[0]).expect("polygon serializes")["points"],
            serde_json::json!([
                [42.7, -84.49],
                [42.78, -84.54],
                [42.88, -84.37],
                [42.78, -84.36],
                [42.78, -84.16],
                [42.76, -84.16],
                [42.7, -84.49]
            ])
        );
    }

    #[test]
    fn parse_latlon_with_packed_wrapped_tail() {
        let text = "LAT...LON 4101 9512 4106 9519 4116 9512 4116 9493 41109493";
        let polygons = parse_latlon_polygons(text);

        assert_eq!(polygons.len(), 1);
        assert_eq!(
            serde_json::to_value(&polygons[0]).expect("polygon serializes")["points"],
            serde_json::json!([
                [41.01, -95.12],
                [41.06, -95.19],
                [41.16, -95.12],
                [41.16, -94.93],
                [41.10, -94.93],
                [41.01, -95.12]
            ])
        );
    }
}
