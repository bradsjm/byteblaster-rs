//! Body enrichment and parsing orchestration.
//!
//! This module coordinates parsing of body elements based on product metadata flags.

use crate::data::ProductMetadataFlags;
use crate::{
    HvtecCode, LatLonPolygon, ProductParseIssue, TimeMotLocEntry, UgcSection, VtecCode,
    WindHailEntry, parse_hvtec_codes_with_issues, parse_latlon_polygons_with_issues,
    parse_time_mot_loc_entries_with_issues, parse_ugc_sections_with_issues,
    parse_vtec_codes_with_issues, parse_wind_hail_entries_with_issues,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeSet;

/// Container for all parsed body elements from a text product.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct ProductBody {
    /// Parsed VTEC (Valid Time Event Code) entries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vtec: Option<Vec<VtecCode>>,
    /// Parsed UGC (Universal Geographic Code) sections
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ugc: Option<Vec<UgcSection>>,
    /// Parsed HVTEC (Hydrologic VTEC) entries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hvtec: Option<Vec<HvtecCode>>,
    /// Parsed LAT...LON polygons
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latlon: Option<Vec<LatLonPolygon>>,
    /// Parsed TIME...MOT...LOC entries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_mot_loc: Option<Vec<TimeMotLocEntry>>,
    /// Parsed wind/hail tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_hail: Option<Vec<WindHailEntry>>,
}

/// Enrich text body by parsing elements based on metadata flags.
///
/// This function conditionally parses body elements based on the product's
/// metadata flags. Each flag that is true will trigger parsing for that
/// element type.
///
/// # Arguments
///
/// * `text` - The full text content of the product
/// * `flags` - Metadata flags indicating which elements to parse
/// * `reference_time` - Reference time for UGC expiration calculation
///
/// # Returns
///
/// A tuple containing:
/// - Optional `ProductBody` with parsed content (None if no parser was attempted)
/// - Vector of `ProductParseIssue` for any parsing issues
pub fn enrich_body(
    text: &str,
    flags: &ProductMetadataFlags,
    reference_time: Option<DateTime<Utc>>,
) -> (Option<ProductBody>, Vec<ProductParseIssue>) {
    let mut body = ProductBody::default();
    let mut issues = Vec::new();
    let mut has_content = false;

    if flags.vtec {
        let (parsed, parse_issues) = parse_vtec_codes_with_issues(text);
        if !parsed.is_empty() {
            body.vtec = Some(parsed);
            has_content = true;
        }
        issues.extend(parse_issues);
    }

    if flags.ugc {
        match reference_time {
            Some(reference_time) => {
                let (parsed, parse_issues) = parse_ugc_sections_with_issues(text, reference_time);
                if !parsed.is_empty() {
                    body.ugc = Some(parsed);
                    has_content = true;
                }
                issues.extend(parse_issues);
            }
            None => {
                issues.push(ProductParseIssue::new(
                    "ugc_parse",
                    "missing_reference_time",
                    "could not parse UGC sections because the header timestamp could not be resolved",
                    None,
                ));
            }
        }
    }

    if flags.hvtec {
        let (parsed, parse_issues) = parse_hvtec_codes_with_issues(text);
        if !parsed.is_empty() {
            body.hvtec = Some(parsed);
            has_content = true;
        }
        issues.extend(parse_issues);
    }

    if flags.latlong {
        let (parsed, parse_issues) = parse_latlon_polygons_with_issues(text);
        if !parsed.is_empty() {
            body.latlon = Some(parsed);
            has_content = true;
        }
        issues.extend(parse_issues);
    }

    if flags.time_mot_loc {
        match reference_time {
            Some(reference_time) => {
                let (parsed, parse_issues) =
                    parse_time_mot_loc_entries_with_issues(text, reference_time);
                if !parsed.is_empty() {
                    body.time_mot_loc = Some(parsed);
                    has_content = true;
                }
                issues.extend(parse_issues);
            }
            None => {
                issues.push(ProductParseIssue::new(
                    "time_mot_loc_parse",
                    "missing_reference_time",
                    "could not parse TIME...MOT...LOC entries because the header timestamp could not be resolved",
                    None,
                ));
            }
        }
    }

    if flags.wind_hail {
        let (parsed, parse_issues) = parse_wind_hail_entries_with_issues(text);
        if !parsed.is_empty() {
            body.wind_hail = Some(parsed);
            has_content = true;
        }
        issues.extend(parse_issues);
    }

    issues.extend(validate_body_qc(&body, flags));

    // Note: `cz` stands for county zones and is intentionally not parsed here.

    (has_content.then_some(body), issues)
}

fn validate_body_qc(body: &ProductBody, flags: &ProductMetadataFlags) -> Vec<ProductParseIssue> {
    let mut issues = Vec::new();

    if flags.vtec
        && flags.latlong
        && body
            .vtec
            .as_ref()
            .is_some_and(|entries| !entries.is_empty())
        && body.latlon.as_ref().is_none_or(Vec::is_empty)
    {
        // Warning products that advertise both VTEC and LAT...LON content can
        // lose their polygon in dissemination. Surface that as QC instead of
        // failing the rest of the body parse.
        issues.push(ProductParseIssue::new(
            "body_qc",
            "vtec_missing_required_polygon",
            "parsed VTEC content but did not recover a LAT...LON polygon from the source text",
            None,
        ));
    }

    if let Some(ugc_sections) = &body.ugc {
        let mut duplicates = BTreeSet::new();

        for section in ugc_sections {
            let mut seen = BTreeSet::new();
            collect_duplicate_ugc_codes(&section.counties, 'C', &mut seen, &mut duplicates);
            collect_duplicate_ugc_codes(&section.zones, 'Z', &mut seen, &mut duplicates);
            collect_duplicate_ugc_codes(&section.fire_zones, 'F', &mut seen, &mut duplicates);
            collect_duplicate_ugc_codes(&section.marine_zones, 'M', &mut seen, &mut duplicates);
        }

        if !duplicates.is_empty() {
            // Some malformed products repeat UGCs across segments or within a
            // single section. Keep the parsed geography, but mark the duplication.
            issues.push(ProductParseIssue::new(
                "body_qc",
                "ugc_duplicate_code",
                format!(
                    "encountered duplicated UGC codes in parsed product body: {}",
                    duplicates.into_iter().collect::<Vec<_>>().join(", ")
                ),
                None,
            ));
        }
    }

    issues
}

fn collect_duplicate_ugc_codes(
    groups: &std::collections::BTreeMap<String, Vec<crate::UgcArea>>,
    class_code: char,
    seen: &mut BTreeSet<String>,
    duplicates: &mut BTreeSet<String>,
) {
    for (state, areas) in groups {
        for area in areas {
            let canonical = format!("{state}{class_code}{:03}", area.id);
            if !seen.insert(canonical.clone()) {
                duplicates.insert(canonical);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrich_body_with_all_flags() {
        let text = r#"
000
WUUS53 KOAX 051200
FFWOAX

NEC001>003-051300-
/O.NEW.KOAX.SV.W.0001.250305T1200Z-250305T1800Z/
/MSRM1.3.ER.250305T1200Z.250305T1800Z.250306T0000Z.NO/

LAT...LON 4143 9613 4145 9610 4140 9608 4138 9612
TIME...MOT...LOC 1200Z 300DEG 25KT 4143 9613 4140 9608
HAILTHREAT...RADARINDICATED
MAXHAILSIZE...1.00 IN
WINDTHREAT...OBSERVED
MAXWINDGUST...60 MPH
"#;

        let flags = ProductMetadataFlags {
            ugc: true,
            vtec: true,
            latlong: true,
            hvtec: true,
            cz: false,
            time_mot_loc: true,
            wind_hail: true,
        };

        let reference_time = Some(Utc::now());
        let (body, warnings) = enrich_body(text, &flags, reference_time);

        assert!(body.is_some());
        let body = body.unwrap();

        assert!(body.vtec.is_some());
        assert_eq!(body.vtec.as_ref().unwrap().len(), 1);

        assert!(body.ugc.is_some());
        assert_eq!(body.ugc.as_ref().unwrap().len(), 1);

        assert!(body.hvtec.is_some());
        assert_eq!(body.hvtec.as_ref().unwrap().len(), 1);
        assert!(body.hvtec.as_ref().unwrap()[0].location.is_none());

        assert!(body.latlon.is_some());
        assert_eq!(body.latlon.as_ref().unwrap().len(), 1);

        assert!(body.time_mot_loc.is_some());
        assert_eq!(body.time_mot_loc.as_ref().unwrap().len(), 1);

        assert!(body.wind_hail.is_some());
        assert_eq!(body.wind_hail.as_ref().unwrap().len(), 4);

        assert!(warnings.is_empty());
    }

    #[test]
    fn enrich_body_with_no_flags() {
        let text = "Some product text with VTEC /O.NEW.KDMX.TO.W.0001.250301T1200Z-250301T1300Z/";

        let flags = ProductMetadataFlags {
            ugc: false,
            vtec: false,
            latlong: false,
            hvtec: false,
            cz: false,
            time_mot_loc: false,
            wind_hail: false,
        };

        let reference_time = Some(Utc::now());
        let (body, warnings) = enrich_body(text, &flags, reference_time);

        assert!(body.is_none());
        assert!(warnings.is_empty());
    }

    #[test]
    fn enrich_body_empty_result_when_no_matches() {
        let text = "Plain text with no codes";

        let flags = ProductMetadataFlags {
            ugc: true,
            vtec: true,
            latlong: true,
            hvtec: true,
            cz: false,
            time_mot_loc: true,
            wind_hail: true,
        };

        let reference_time = Some(Utc::now());
        let (body, warnings) = enrich_body(text, &flags, reference_time);

        assert!(body.is_none());
        assert!(warnings.is_empty());
    }

    #[test]
    fn enrich_body_reports_missing_polygon_for_vtec_products() {
        let text = "/O.NEW.KOAX.SV.W.0001.250305T1200Z-250305T1800Z/\nTIME...MOT...LOC 1200Z 300DEG 25KT 4143 9613";
        let flags = ProductMetadataFlags {
            ugc: false,
            vtec: true,
            latlong: true,
            hvtec: false,
            cz: false,
            time_mot_loc: true,
            wind_hail: false,
        };

        let (_, issues) = enrich_body(text, &flags, Some(Utc::now()));
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "vtec_missing_required_polygon")
        );
    }

    #[test]
    fn enrich_body_reports_duplicate_ugc_codes() {
        let text = "IAC001-IAC001-041200-\n";
        let flags = ProductMetadataFlags {
            ugc: true,
            vtec: false,
            latlong: false,
            hvtec: false,
            cz: false,
            time_mot_loc: false,
            wind_hail: false,
        };

        let (_, issues) = enrich_body(text, &flags, Some(Utc::now()));
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "ugc_duplicate_code")
        );
    }
}
