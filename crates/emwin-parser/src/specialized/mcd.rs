//! Parsing for SPC mesoscale discussions and WPC mesoscale precipitation discussions.

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

use crate::{LatLonPolygon, WmoHeader, parse_latlon_polygons};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McdCenter {
    Spc,
    Wpc,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct McdMostProbableTags {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tornado: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gust: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct McdBulletin {
    pub center: McdCenter,
    pub discussion_number: u32,
    pub is_correction: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub areas_affected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concerning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watch_probability_percent: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attn_wfo: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attn_rfc: Vec<String>,
    pub most_probable: McdMostProbableTags,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polygon: Option<LatLonPolygon>,
    pub raw: String,
}

pub(crate) fn parse_mcd_bulletin(
    text: &str,
    afos: Option<&str>,
    reference_time: DateTime<Utc>,
) -> Option<McdBulletin> {
    let normalized = text.replace('\r', "");
    let compact = normalized.trim().to_string();
    let center = if afos == Some("FFGMPD")
        || compact
            .to_ascii_uppercase()
            .contains("WEATHER PREDICTION CENTER")
    {
        McdCenter::Wpc
    } else {
        McdCenter::Spc
    };
    let discussion_number = discussion_re()
        .captures(&compact)?
        .name("num")?
        .as_str()
        .parse()
        .ok()?;
    let (valid_from, valid_to) = valid_re()
        .captures(&compact)
        .map(|captures| {
            let from = captures
                .name("from")
                .and_then(|value| resolve_ddhhmmz(value.as_str(), reference_time));
            let to = captures
                .name("to")
                .and_then(|value| resolve_ddhhmmz(value.as_str(), reference_time));
            (from, to)
        })
        .unwrap_or((None, None));

    Some(McdBulletin {
        center,
        discussion_number,
        is_correction: correction_re().is_match(&compact),
        valid_from,
        valid_to,
        areas_affected: section_value(&compact, "AREAS AFFECTED"),
        concerning: section_value(&compact, "CONCERNING"),
        watch_probability_percent: watch_prob_re()
            .captures(&compact)
            .and_then(|captures| captures.name("percent")?.as_str().parse().ok()),
        attn_wfo: attn_list(&compact, "ATTN...WFO"),
        attn_rfc: attn_list(&compact, "ATTN...RFC"),
        most_probable: McdMostProbableTags {
            tornado: most_probable_value(&compact, "TORNADO INTENSITY"),
            hail: most_probable_value(&compact, "HAIL SIZE"),
            gust: most_probable_value(&compact, "WIND GUST"),
        },
        polygon: parse_latlon_polygons(&compact).into_iter().next(),
        raw: compact,
    })
}

fn section_value(text: &str, name: &str) -> Option<String> {
    let needle = format!("{name}...");
    let start = text.find(&needle)?;
    let remainder = &text[start + needle.len()..];
    let value = remainder
        .split("\n\n")
        .next()
        .unwrap_or_default()
        .replace('\n', " ")
        .trim()
        .trim_matches('.')
        .trim()
        .to_string();
    (!value.is_empty()).then_some(value)
}

fn attn_list(text: &str, prefix: &str) -> Vec<String> {
    let Some(start) = text.find(prefix) else {
        return Vec::new();
    };
    let mut lines = text[start..].lines();
    let first = lines.next().unwrap_or_default();
    let mut joined = String::from(first);
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if trimmed.contains("...") && !trimmed.starts_with("...") {
            break;
        }
        joined.push(' ');
        joined.push_str(trimmed);
    }
    let normalized = joined
        .strip_prefix(prefix)
        .unwrap_or_default()
        .trim()
        .trim_matches('.')
        .replace("...", ",");
    normalized
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn most_probable_value(text: &str, name: &str) -> Option<String> {
    let pattern = format!("MOST PROBABLE PEAK {name}...");
    let start = text.to_ascii_uppercase().find(&pattern)?;
    let remainder = &text[start + pattern.len()..];
    let value = remainder
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    (!value.is_empty()).then_some(value)
}

fn resolve_ddhhmmz(ddhhmmz: &str, reference_time: DateTime<Utc>) -> Option<String> {
    let wmo = WmoHeader {
        ttaaii: "ACUS11".to_string(),
        cccc: "KWNS".to_string(),
        ddhhmm: ddhhmmz.to_string(),
        bbb: None,
    };
    Some(wmo.timestamp(reference_time)?.to_rfc3339())
}

fn discussion_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?im)^MESOSCALE (?:PRECIPITATION )?DISCUSSION\s+(?P<num>\d+)$")
            .expect("valid mcd discussion regex")
    })
}

fn valid_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?im)^VALID\s+(?P<from>\d{6})Z?\s*-\s*(?P<to>\d{6})Z?$")
            .expect("valid mcd valid regex")
    })
}

fn watch_prob_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)PROBABILITY OF WATCH ISSUANCE\s*\.\.\.\s*(?P<percent>\d+)\s*PERCENT")
            .expect("valid mcd watch probability regex")
    })
}

fn correction_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?im)^SPC\s+MCD\s+\d+\s+COR\b|^MESOSCALE (?:PRECIPITATION )?DISCUSSION.*\bCOR\b",
        )
        .expect("valid mcd correction regex")
    })
}

#[cfg(test)]
mod tests {
    use super::{McdCenter, parse_mcd_bulletin};
    use chrono::{TimeZone, Utc};

    fn reference_time() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2013, 7, 26, 2, 8, 0)
            .single()
            .expect("valid reference time")
    }

    #[test]
    fn parses_swomcd_bulletin() {
        let text = "\
SPC MCD 260208
MIZ000-WIZ000-260415-

MESOSCALE DISCUSSION 1525
NWS STORM PREDICTION CENTER NORMAN OK
0908 PM CDT THU JUL 25 2013

AREAS AFFECTED...PORTIONS OF NRN WI AND THE UPPER PENINSULA OF MI

CONCERNING...SEVERE THUNDERSTORM WATCH 446...

VALID 260208Z - 260415Z

ATTN...WFO...MQT...GRB...DLH...

LAT...LON 44738786 45378992 45829078 46369061 46638962 46338801
 45868698 44738786";
        let bulletin =
            parse_mcd_bulletin(text, Some("SWOMCD"), reference_time()).expect("mcd bulletin");
        assert_eq!(bulletin.center, McdCenter::Spc);
        assert_eq!(bulletin.discussion_number, 1525);
        assert_eq!(bulletin.attn_wfo, vec!["MQT", "GRB", "DLH"]);
        assert_eq!(
            bulletin.areas_affected.as_deref(),
            Some("PORTIONS OF NRN WI AND THE UPPER PENINSULA OF MI")
        );
        assert!(bulletin.polygon.is_some());
    }

    #[test]
    fn parses_most_probable_tags() {
        let text = "\
MESOSCALE DISCUSSION 2237
VALID 070200Z - 070500Z
MOST PROBABLE PEAK TORNADO INTENSITY...85-115 MPH
MOST PROBABLE PEAK HAIL SIZE...1.00-1.75 INCHES
MOST PROBABLE PEAK WIND GUST...55-70 MPH";
        let bulletin =
            parse_mcd_bulletin(text, Some("SWOMCD"), reference_time()).expect("mcd bulletin");
        assert_eq!(
            bulletin.most_probable.tornado.as_deref(),
            Some("85-115 MPH")
        );
        assert_eq!(
            bulletin.most_probable.hail.as_deref(),
            Some("1.00-1.75 INCHES")
        );
        assert_eq!(bulletin.most_probable.gust.as_deref(), Some("55-70 MPH"));
    }
}
