//! Parsing for SPC preliminary notice of watch products.

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

use crate::specialized::wwp::SpcWatchType;
use crate::{GeoPoint, WmoHeader, parse_latlon_polygons};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SawAction {
    Issue,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SawBulletin {
    pub saw_number: u16,
    pub watch_number: u16,
    pub watch_type: SpcWatchType,
    pub action: SawAction,
    pub is_test: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaces_watch_number: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polygon: Option<Vec<GeoPoint>>,
}

pub(crate) fn parse_saw_bulletin(
    text: &str,
    afos: Option<&str>,
    reference_time: DateTime<Utc>,
) -> Option<SawBulletin> {
    let normalized = text.replace('\r', "");
    let header = watch_re().captures(&normalized)?;
    let watch_number = header.name("num")?.as_str().parse().ok()?;
    let action = if normalized.to_ascii_uppercase().contains("CANCELLED") {
        SawAction::Cancel
    } else {
        SawAction::Issue
    };
    let polygon = parse_latlon_polygons(&normalized)
        .into_iter()
        .next()
        .map(|polygon| {
            polygon
                .points
                .into_iter()
                .map(|(lat, lon)| GeoPoint { lat, lon })
                .collect()
        });

    let valid_from = header
        .name("from")
        .and_then(|value| resolve_ddhhmmz(value.as_str(), reference_time));
    let valid_to = header
        .name("to")
        .and_then(|value| resolve_ddhhmmz(value.as_str(), reference_time));

    Some(SawBulletin {
        saw_number: parse_saw_number(afos)?,
        watch_number,
        watch_type: if header.name("typ")?.as_str() == "TORNADO" {
            SpcWatchType::Tornado
        } else {
            SpcWatchType::SevereThunderstorm
        },
        action,
        is_test: watch_number > 9000 || normalized.to_ascii_uppercase().contains("...TEST"),
        replaces_watch_number: replaces_re()
            .captures(&normalized)
            .and_then(|captures| captures.name("num")?.as_str().parse().ok()),
        valid_from,
        valid_to,
        polygon,
    })
}

fn parse_saw_number(afos: Option<&str>) -> Option<u16> {
    let afos = afos?;
    afos.strip_prefix("SAW")?.parse().ok()
}

fn watch_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?im)^WW\s+(?P<num>\d{1,4})\s+(?P<typ>TORNADO|SEVERE TSTM|SEVERE THUNDERSTORM)(?:\s+.*?\s+(?P<from>\d{6}Z)\s*-\s*(?P<to>\d{6}Z))?(?:\s|$)",
        )
        .expect("valid SAW watch regex")
    })
}

fn replaces_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?im)REPLACES WW\s+(?P<num>\d{1,4})").expect("valid SAW replaces regex")
    })
}

fn resolve_ddhhmmz(ddhhmmz: &str, reference_time: DateTime<Utc>) -> Option<String> {
    let ddhhmm = ddhhmmz.strip_suffix('Z')?;
    let wmo = WmoHeader {
        ttaaii: "WWUS30".to_string(),
        cccc: "KWNS".to_string(),
        ddhhmm: ddhhmm.to_string(),
        bbb: None,
    };
    Some(wmo.timestamp(reference_time)?.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{SawAction, SawBulletin, parse_saw_bulletin};
    use crate::GeoPoint;
    use crate::specialized::wwp::SpcWatchType;

    fn reference_time() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 7, 25, 17, 40, 0)
            .single()
            .expect("valid reference time")
    }

    #[test]
    fn parses_issuance_with_polygon() {
        let text = "\
WW 542 SEVERE TSTM CT DE MA NJ NY PA RI CW 251745Z - 260100Z
LAT...LON 41087082 39507704 41247704 42827082
";
        let bulletin = parse_saw_bulletin(text, Some("SAW2"), reference_time());
        assert_eq!(
            bulletin,
            Some(SawBulletin {
                saw_number: 2,
                watch_number: 542,
                watch_type: SpcWatchType::SevereThunderstorm,
                action: SawAction::Issue,
                is_test: false,
                replaces_watch_number: None,
                valid_from: Some("2025-07-25T17:45:00+00:00".to_string()),
                valid_to: Some("2025-07-26T01:00:00+00:00".to_string()),
                polygon: Some(vec![
                    GeoPoint {
                        lat: 41.08,
                        lon: -70.82,
                    },
                    GeoPoint {
                        lat: 39.5,
                        lon: -77.04,
                    },
                    GeoPoint {
                        lat: 41.24,
                        lon: -77.04,
                    },
                    GeoPoint {
                        lat: 42.82,
                        lon: -70.82,
                    },
                    GeoPoint {
                        lat: 41.08,
                        lon: -70.82,
                    },
                ]),
            })
        );
    }

    #[test]
    fn parses_cancellation_without_polygon() {
        let text = "WW 540 SEVERE THUNDERSTORM CANCELLED";
        let bulletin = parse_saw_bulletin(text, Some("SAW0"), reference_time());
        assert_eq!(
            bulletin,
            Some(SawBulletin {
                saw_number: 0,
                watch_number: 540,
                watch_type: SpcWatchType::SevereThunderstorm,
                action: SawAction::Cancel,
                is_test: false,
                replaces_watch_number: None,
                valid_from: None,
                valid_to: None,
                polygon: None,
            })
        );
    }

    #[test]
    fn parses_replacement_watch_number() {
        let text = "\
WW 533 TORNADO IA KS 252100Z - 260200Z
REPLACES WW 531
";
        let bulletin = parse_saw_bulletin(text, Some("SAW3"), reference_time()).expect("saw");
        assert_eq!(bulletin.replaces_watch_number, Some(531));
        assert_eq!(bulletin.watch_type, SpcWatchType::Tornado);
    }

    #[test]
    fn detects_test_product() {
        let text = "\
WW 9001 TORNADO OK 252100Z - 260200Z
...TEST
";
        let bulletin = parse_saw_bulletin(text, Some("SAW9"), reference_time()).expect("saw");
        assert!(bulletin.is_test);
    }
}
