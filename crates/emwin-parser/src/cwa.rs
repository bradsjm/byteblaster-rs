//! Parsing for Center Weather Advisories.

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

use crate::GeoPoint;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CwaGeometryKind {
    Polygon,
    LineBuffer,
    Circle,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CwaGeometry {
    pub kind: CwaGeometryKind,
    pub points: Vec<GeoPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CwaBulletin {
    pub center: String,
    pub number: u16,
    pub issue_time: String,
    pub expire_time: String,
    pub is_corrected: bool,
    pub is_cancelled: bool,
    pub narrative: Option<String>,
    pub geometry: Option<CwaGeometry>,
}

pub(crate) fn parse_cwa_bulletin(text: &str, reference_time: DateTime<Utc>) -> Option<CwaBulletin> {
    let normalized = text.replace('\r', "");
    let lines: Vec<&str> = normalized.lines().collect();
    if lines.len() < 3 {
        return None;
    }
    let line3 = lines.first()?.trim();
    let line4 = lines.get(1)?.trim();
    let line3_tokens = line3.split_whitespace().collect::<Vec<_>>();
    let line4_tokens = line4.split_whitespace().collect::<Vec<_>>();
    if line3_tokens.len() < 3 || line4_tokens.len() < 6 {
        return None;
    }
    let center = line4_tokens[0].to_string();
    let number = line4_tokens[2].parse::<u16>().ok()?;
    let issue_time = resolve_ddhhmm(line3_tokens[2], reference_time)
        .unwrap_or_else(|| line3_tokens[2].to_string());
    let expire_time = resolve_ddhhmm(line4_tokens[5], reference_time)
        .unwrap_or_else(|| line4_tokens[5].to_string());
    let is_corrected = line3_tokens.get(3).is_some_and(|token| *token == "COR");
    let body = lines[2..].join(" ");
    let upper = body.to_ascii_uppercase();
    if upper.contains("CANCEL") || upper.contains("ERROR") {
        return Some(CwaBulletin {
            center,
            number,
            issue_time,
            expire_time,
            is_corrected,
            is_cancelled: true,
            narrative: Some(body.trim().trim_end_matches('=').trim().to_string()),
            geometry: None,
        });
    }

    let kind = if upper.contains("NM WIDE") {
        CwaGeometryKind::LineBuffer
    } else if upper.contains("DIAM") {
        CwaGeometryKind::Circle
    } else {
        CwaGeometryKind::Polygon
    };
    let points = parse_points(&body).unwrap_or_default();
    let narrative = extract_narrative(&body);

    Some(CwaBulletin {
        center,
        number,
        issue_time,
        expire_time,
        is_corrected,
        is_cancelled: false,
        narrative,
        geometry: Some(CwaGeometry { kind, points }),
    })
}

fn lalo_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"([NS])\s?(\d{4,5})\s*([EW])\s?(\d{4,5})").expect("valid CWA LALO regex")
    })
}

fn parse_points(body: &str) -> Option<Vec<GeoPoint>> {
    let mut points = Vec::new();
    for caps in lalo_re().captures_iter(body) {
        let lat_raw = caps.get(2)?.as_str().parse::<f64>().ok()? / 100.0;
        let lon_raw = caps.get(4)?.as_str().parse::<f64>().ok()? / 100.0;
        let lat = if caps.get(1)?.as_str() == "S" {
            -lat_raw
        } else {
            lat_raw
        };
        let lon = if caps.get(3)?.as_str() == "W" {
            -lon_raw
        } else {
            lon_raw
        };
        points.push(GeoPoint { lat, lon });
    }
    if !points.is_empty() {
        return Some(points);
    }

    let from_text = body
        .replace("FFROM ", "")
        .replace("FROM ", "")
        .replace('=', "")
        .replace('\n', " ");
    let mut from_points = Vec::new();
    for token in from_text.split('-') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let token = token.split(" AREA ").next().unwrap_or(token);
        let parts = token.split_whitespace().collect::<Vec<_>>();
        let (offset_dir, loc) = match parts.as_slice() {
            [loc] => ("", *loc),
            [offset_dir, loc] => (*offset_dir, *loc),
            _ => continue,
        };
        let point = station_point(loc)?;
        let (offset, dir) = split_offset_dir(offset_dir);
        from_points.push(displace(point, dir, offset));
    }
    (!from_points.is_empty()).then_some(from_points)
}

fn split_offset_dir(value: &str) -> (f64, &str) {
    let digits_len = value.chars().take_while(|ch| ch.is_ascii_digit()).count();
    let digits = &value[..digits_len];
    let direction = &value[digits_len..];
    (
        digits.parse::<f64>().unwrap_or(0.0),
        if direction.is_empty() { "N" } else { direction },
    )
}

fn extract_narrative(body: &str) -> Option<String> {
    let upper = body.to_ascii_uppercase();
    let narrative = if let Some(idx) = upper.find(" AREA ") {
        &body[idx + 1..]
    } else {
        body
    };
    let trimmed = narrative.trim().trim_end_matches('=').trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn resolve_ddhhmm(ddhhmm: &str, reference_time: DateTime<Utc>) -> Option<String> {
    let wmo = crate::WmoHeader {
        ttaaii: "FAUS00".to_string(),
        cccc: "KXXX".to_string(),
        ddhhmm: ddhhmm.to_string(),
        bbb: None,
    };
    Some(wmo.timestamp(reference_time)?.to_rfc3339())
}

fn station_point(code: &str) -> Option<GeoPoint> {
    match code {
        "BIL" => Some(GeoPoint {
            lat: 45.8077,
            lon: -108.5429,
        }),
        "SHR" => Some(GeoPoint {
            lat: 44.7692,
            lon: -106.9803,
        }),
        "DDY" => Some(GeoPoint {
            lat: 42.7972,
            lon: -105.3864,
        }),
        "OCS" => Some(GeoPoint {
            lat: 41.5928,
            lon: -109.0157,
        }),
        "SLC" => Some(GeoPoint {
            lat: 40.7884,
            lon: -111.9778,
        }),
        _ => None,
    }
}

fn displace(point: GeoPoint, dir: &str, nm: f64) -> GeoPoint {
    let bearing: f64 = match dir {
        "NNE" => 22.5,
        "NE" => 45.0,
        "ENE" => 67.5,
        "E" => 90.0,
        "ESE" => 112.5,
        "SE" => 135.0,
        "SSE" => 157.5,
        "S" => 180.0,
        "SSW" => 202.5,
        "SW" => 225.0,
        "WSW" => 247.5,
        "W" => 270.0,
        "WNW" => 292.5,
        "NW" => 315.0,
        "NNW" => 337.5,
        _ => 0.0,
    };
    let degrees = nm / 60.0;
    let rad = bearing.to_radians();
    GeoPoint {
        lat: point.lat + degrees * rad.cos(),
        lon: point.lon + degrees * rad.sin() / point.lat.to_radians().cos().max(0.1),
    }
}

#[cfg(test)]
mod tests {
    use super::{CwaGeometryKind, parse_cwa_bulletin};
    use chrono::Utc;

    #[test]
    fn parses_active_cwa_fixture() {
        let text = "ZLC2 CWA 100230 \nZLC CWA 202 VALID UNTIL 100630\nFROM 75W BIL-15NNE SHR-55SW DDY-45S OCS-35SSE SLC-75W BIL\nAREA MOD/ISO SEV MTN WAVE FL350-ABV FL450.\n";
        let bulletin = parse_cwa_bulletin(text, Utc::now()).expect("cwa bulletin");
        assert!(!bulletin.is_cancelled);
        assert_eq!(bulletin.number, 202);
        assert!(matches!(
            bulletin.geometry.as_ref().map(|g| &g.kind),
            Some(CwaGeometryKind::Polygon)
        ));
    }

    #[test]
    fn parses_cancel_cwa_fixture() {
        let text = "ZFW4 CWA 100038 \nZFW CWA 402 VALID UNTIL 100100\nCANCEL ZFW CWA 401. CONDS MOSTLY RA.\n=";
        let bulletin = parse_cwa_bulletin(text, Utc::now()).expect("cancel cwa");
        assert!(bulletin.is_cancelled);
        assert!(bulletin.geometry.is_none());
    }
}
