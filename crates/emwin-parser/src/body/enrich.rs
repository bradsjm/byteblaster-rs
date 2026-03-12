//! Body enrichment and parsing orchestration.
//!
//! Generic body parsing is driven by a data-derived extraction plan instead of
//! branching directly on per-product flag booleans. The catalog now stores
//! ordered extractor lists, and this module turns those lists into reusable
//! extraction plans with plan-driven QC.

use crate::{
    GeoBounds, GeoPoint, HvtecCode, LatLonPolygon, ProductParseIssue, TimeMotLocEntry, UgcSection,
    VtecCode, WindHailEntry, parse_hvtec_codes_with_issues, parse_latlon_polygons_with_issues,
    parse_time_mot_loc_entries_with_issues, parse_ugc_sections_with_issues,
    parse_vtec_codes_with_issues, parse_wind_hail_entries_with_issues, polygon_bounds,
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

impl ProductBody {
    pub fn iter_location_points(&self) -> impl Iterator<Item = GeoPoint> + '_ {
        let time_mot_loc = self.time_mot_loc.iter().flat_map(|entries| {
            entries
                .iter()
                .flat_map(|entry| entry.points.iter().map(|&(lat, lon)| GeoPoint { lat, lon }))
        });
        let ugc = self.ugc.iter().flat_map(|sections| {
            sections.iter().flat_map(|section| {
                section
                    .counties
                    .values()
                    .chain(section.zones.values())
                    .flat_map(|areas| areas.iter())
                    .filter_map(|area| {
                        area.lat
                            .zip(area.lon)
                            .map(|(lat, lon)| GeoPoint { lat, lon })
                    })
            })
        });
        let hvtec = self.hvtec.iter().flat_map(|codes| {
            codes.iter().filter_map(|code| {
                code.location.map(|location| GeoPoint {
                    lat: location.latitude,
                    lon: location.longitude,
                })
            })
        });

        time_mot_loc.chain(ugc).chain(hvtec)
    }

    pub fn iter_polygons(&self) -> impl Iterator<Item = ParsedPolygon<'_>> + '_ {
        self.latlon.iter().flat_map(|polygons| {
            polygons.iter().map(|polygon| ParsedPolygon {
                points: &polygon.points,
                bounds: polygon_bounds(&polygon.points),
            })
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParsedPolygon<'a> {
    pub points: &'a [(f64, f64)],
    pub bounds: Option<GeoBounds>,
}

/// Stable identifier for a generic body extractor.
///
/// The execution order is fixed because downstream issue ordering and parse
/// semantics depend on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyExtractorId {
    Vtec,
    Ugc,
    Hvtec,
    LatLon,
    TimeMotLoc,
    WindHail,
}

/// Stable identifier for post-parse body QC checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodyQcRuleId {
    VtecMissingRequiredPolygon,
    UgcDuplicateCode,
}

/// Ordered extractor and QC configuration derived from catalog metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BodyExtractionPlan {
    pub(crate) extractors: &'static [BodyExtractorId],
    pub(crate) qc_rules: &'static [BodyQcRuleId],
}

/// Shared context passed to plan-driven extractors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BodyExtractionContext<'a> {
    pub(crate) text: &'a str,
    pub(crate) reference_time: Option<DateTime<Utc>>,
}

/// Final result produced by the plan-driven extraction engine.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct BodyExtractionOutcome {
    pub(crate) body: Option<ProductBody>,
    pub(crate) issues: Vec<ProductParseIssue>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct BodyExtractionState {
    body: ProductBody,
    issues: Vec<ProductParseIssue>,
    has_content: bool,
}

const QC_NONE: &[BodyQcRuleId] = &[];
const QC_VTEC_POLYGON: &[BodyQcRuleId] = &[BodyQcRuleId::VtecMissingRequiredPolygon];
const QC_UGC_DUPLICATES: &[BodyQcRuleId] = &[BodyQcRuleId::UgcDuplicateCode];
const QC_VTEC_POLYGON_AND_UGC: &[BodyQcRuleId] = &[
    BodyQcRuleId::VtecMissingRequiredPolygon,
    BodyQcRuleId::UgcDuplicateCode,
];

/// Builds an extraction plan from an ordered extractor list.
///
/// The extractor order is preserved exactly because downstream issue ordering
/// depends on it.
pub(crate) fn body_extraction_plan(extractors: &'static [BodyExtractorId]) -> BodyExtractionPlan {
    let has_vtec = extractors.contains(&BodyExtractorId::Vtec);
    let has_ugc = extractors.contains(&BodyExtractorId::Ugc);
    let has_latlon = extractors.contains(&BodyExtractorId::LatLon);
    let qc_rules = match (has_vtec && has_latlon, has_ugc) {
        (false, false) => QC_NONE,
        (true, false) => QC_VTEC_POLYGON,
        (false, true) => QC_UGC_DUPLICATES,
        (true, true) => QC_VTEC_POLYGON_AND_UGC,
    };

    BodyExtractionPlan {
        extractors,
        qc_rules,
    }
}

/// Enriches text body content by deriving the extraction plan from the text-product catalog.
///
/// Unknown PIL values intentionally produce no body content and no issues.
pub fn enrich_body(
    text: &str,
    pil: &str,
    reference_time: Option<DateTime<Utc>>,
) -> (Option<ProductBody>, Vec<ProductParseIssue>) {
    let Some(plan) = crate::data::text_product_catalog_entry(pil)
        .and_then(crate::data::body_extraction_plan_for_entry)
    else {
        return (None, Vec::new());
    };
    let outcome = enrich_body_from_plan(text, &plan, reference_time);
    (outcome.body, outcome.issues)
}

/// Runs the plan-driven extraction engine over a body text payload.
pub(crate) fn enrich_body_from_plan(
    text: &str,
    plan: &BodyExtractionPlan,
    reference_time: Option<DateTime<Utc>>,
) -> BodyExtractionOutcome {
    let context = BodyExtractionContext {
        text,
        reference_time,
    };
    let mut state = BodyExtractionState::default();

    for extractor in plan.extractors {
        match extractor {
            BodyExtractorId::Vtec => apply_vtec_extractor(&mut state, &context),
            BodyExtractorId::Ugc => apply_ugc_extractor(&mut state, &context),
            BodyExtractorId::Hvtec => apply_hvtec_extractor(&mut state, &context),
            BodyExtractorId::LatLon => apply_latlon_extractor(&mut state, &context),
            BodyExtractorId::TimeMotLoc => apply_time_mot_loc_extractor(&mut state, &context),
            BodyExtractorId::WindHail => apply_wind_hail_extractor(&mut state, &context),
        }
    }

    state.issues.extend(validate_body_qc(&state.body, plan));

    BodyExtractionOutcome {
        body: state.has_content.then_some(state.body),
        issues: state.issues,
    }
}

fn apply_vtec_extractor(state: &mut BodyExtractionState, context: &BodyExtractionContext<'_>) {
    let (parsed, issues) = parse_vtec_codes_with_issues(context.text);
    if !parsed.is_empty() {
        state.body.vtec = Some(parsed);
        state.has_content = true;
    }
    state.issues.extend(issues);
}

fn apply_ugc_extractor(state: &mut BodyExtractionState, context: &BodyExtractionContext<'_>) {
    match context.reference_time {
        Some(reference_time) => {
            let (parsed, issues) = parse_ugc_sections_with_issues(context.text, reference_time);
            if !parsed.is_empty() {
                state.body.ugc = Some(parsed);
                state.has_content = true;
            }
            state.issues.extend(issues);
        }
        None => state.issues.push(ProductParseIssue::new(
            "ugc_parse",
            "missing_reference_time",
            "could not parse UGC sections because the header timestamp could not be resolved",
            None,
        )),
    }
}

fn apply_hvtec_extractor(state: &mut BodyExtractionState, context: &BodyExtractionContext<'_>) {
    let (parsed, issues) = parse_hvtec_codes_with_issues(context.text);
    if !parsed.is_empty() {
        state.body.hvtec = Some(parsed);
        state.has_content = true;
    }
    state.issues.extend(issues);
}

fn apply_latlon_extractor(state: &mut BodyExtractionState, context: &BodyExtractionContext<'_>) {
    let (parsed, issues) = parse_latlon_polygons_with_issues(context.text);
    if !parsed.is_empty() {
        state.body.latlon = Some(parsed);
        state.has_content = true;
    }
    state.issues.extend(issues);
}

fn apply_time_mot_loc_extractor(
    state: &mut BodyExtractionState,
    context: &BodyExtractionContext<'_>,
) {
    match context.reference_time {
        Some(reference_time) => {
            let (parsed, issues) =
                parse_time_mot_loc_entries_with_issues(context.text, reference_time);
            if !parsed.is_empty() {
                state.body.time_mot_loc = Some(parsed);
                state.has_content = true;
            }
            state.issues.extend(issues);
        }
        None => state.issues.push(ProductParseIssue::new(
            "time_mot_loc_parse",
            "missing_reference_time",
            "could not parse TIME...MOT...LOC entries because the header timestamp could not be resolved",
            None,
        )),
    }
}

fn apply_wind_hail_extractor(state: &mut BodyExtractionState, context: &BodyExtractionContext<'_>) {
    let (parsed, issues) = parse_wind_hail_entries_with_issues(context.text);
    if !parsed.is_empty() {
        state.body.wind_hail = Some(parsed);
        state.has_content = true;
    }
    state.issues.extend(issues);
}

fn validate_body_qc(body: &ProductBody, plan: &BodyExtractionPlan) -> Vec<ProductParseIssue> {
    let mut issues = Vec::new();

    for rule in plan.qc_rules {
        match rule {
            BodyQcRuleId::VtecMissingRequiredPolygon => {
                // Marine warnings can legitimately express their geography with
                // marine-zone UGC alone, so missing LAT...LON should not be QC'd there.
                if body
                    .vtec
                    .as_ref()
                    .is_some_and(|entries| !entries.is_empty())
                    && body.latlon.as_ref().is_none_or(Vec::is_empty)
                    && !body_has_marine_only_ugc(body)
                {
                    issues.push(ProductParseIssue::new(
                        "body_qc",
                        "vtec_missing_required_polygon",
                        "parsed VTEC content but did not recover a LAT...LON polygon from the source text",
                        None,
                    ));
                }
            }
            BodyQcRuleId::UgcDuplicateCode => {
                if let Some(ugc_sections) = &body.ugc {
                    let mut duplicates = BTreeSet::new();

                    for section in ugc_sections {
                        let mut seen = BTreeSet::new();
                        collect_duplicate_ugc_codes(
                            &section.counties,
                            'C',
                            &mut seen,
                            &mut duplicates,
                        );
                        collect_duplicate_ugc_codes(
                            &section.zones,
                            'Z',
                            &mut seen,
                            &mut duplicates,
                        );
                        collect_duplicate_ugc_codes(
                            &section.fire_zones,
                            'F',
                            &mut seen,
                            &mut duplicates,
                        );
                        collect_duplicate_ugc_codes(
                            &section.marine_zones,
                            'M',
                            &mut seen,
                            &mut duplicates,
                        );
                    }

                    if !duplicates.is_empty() {
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
            }
        }
    }

    issues
}

fn body_has_marine_only_ugc(body: &ProductBody) -> bool {
    let Some(ugc_sections) = &body.ugc else {
        return false;
    };

    !ugc_sections.is_empty()
        && ugc_sections.iter().all(|section| {
            section.counties.is_empty()
                && section.zones.is_empty()
                && section.fire_zones.is_empty()
                && !section.marine_zones.is_empty()
        })
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
    use crate::{GeoBounds, GeoPoint};

    const FULL_EXTRACTORS: &[BodyExtractorId] = &[
        BodyExtractorId::Vtec,
        BodyExtractorId::Ugc,
        BodyExtractorId::Hvtec,
        BodyExtractorId::LatLon,
        BodyExtractorId::TimeMotLoc,
        BodyExtractorId::WindHail,
    ];
    const UGC_AND_TIME_MOT_LOC_EXTRACTORS: &[BodyExtractorId] =
        &[BodyExtractorId::Ugc, BodyExtractorId::TimeMotLoc];
    const VTEC_LATLON_TIME_MOT_LOC_EXTRACTORS: &[BodyExtractorId] = &[
        BodyExtractorId::Vtec,
        BodyExtractorId::LatLon,
        BodyExtractorId::TimeMotLoc,
    ];
    const UGC_ONLY_EXTRACTORS: &[BodyExtractorId] = &[BodyExtractorId::Ugc];

    #[test]
    fn body_extraction_plan_maps_known_extractor_sets_to_expected_extractors() {
        let plan = body_extraction_plan(FULL_EXTRACTORS);

        assert_eq!(
            plan.extractors,
            &[
                BodyExtractorId::Vtec,
                BodyExtractorId::Ugc,
                BodyExtractorId::Hvtec,
                BodyExtractorId::LatLon,
                BodyExtractorId::TimeMotLoc,
                BodyExtractorId::WindHail,
            ]
        );
    }

    #[test]
    fn body_extraction_plan_maps_qc_rules_for_vtec_latlon_products() {
        let plan = body_extraction_plan(&[BodyExtractorId::Vtec, BodyExtractorId::LatLon]);

        assert_eq!(plan.qc_rules, &[BodyQcRuleId::VtecMissingRequiredPolygon]);
    }

    #[test]
    fn enrich_body_with_full_plan() {
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

        let reference_time = Some(Utc::now());
        let outcome =
            enrich_body_from_plan(text, &body_extraction_plan(FULL_EXTRACTORS), reference_time);
        let body = outcome.body;
        let warnings = outcome.issues;

        assert!(body.is_some());
        let body = body.unwrap();

        assert!(body.vtec.is_some());
        assert_eq!(body.vtec.as_ref().expect("vtec parsed").len(), 1);
        assert!(body.ugc.is_some());
        assert_eq!(body.ugc.as_ref().expect("ugc parsed").len(), 1);
        assert!(body.hvtec.is_some());
        assert_eq!(body.hvtec.as_ref().expect("hvtec parsed").len(), 1);
        assert!(
            body.hvtec.as_ref().expect("hvtec parsed")[0]
                .location
                .is_none()
        );
        assert!(body.latlon.is_some());
        assert_eq!(body.latlon.as_ref().expect("latlon parsed").len(), 1);
        assert!(body.time_mot_loc.is_some());
        assert_eq!(
            body.time_mot_loc
                .as_ref()
                .expect("time mot loc parsed")
                .len(),
            1
        );
        assert!(body.wind_hail.is_some());
        assert_eq!(body.wind_hail.as_ref().expect("wind hail parsed").len(), 4);
        assert!(warnings.is_empty());
    }

    #[test]
    fn enrich_body_from_plan_preserves_current_extractor_order() {
        let plan = body_extraction_plan(UGC_AND_TIME_MOT_LOC_EXTRACTORS);
        let outcome = enrich_body_from_plan("plain text", &plan, None);

        assert_eq!(outcome.issues.len(), 2);
        assert_eq!(outcome.issues[0].kind, "ugc_parse");
        assert_eq!(outcome.issues[1].kind, "time_mot_loc_parse");
    }

    #[test]
    fn enrich_body_wrapper_matches_plan_based_engine() {
        let text = "/O.NEW.KOAX.SV.W.0001.250305T1200Z-250305T1800Z/";
        let plan = crate::data::text_product_catalog_entry("SVR")
            .and_then(crate::data::body_extraction_plan_for_entry)
            .expect("SVR should have body extraction plan");

        let wrapper = enrich_body(text, "SVR", Some(Utc::now()));
        let outcome = enrich_body_from_plan(text, &plan, Some(Utc::now()));

        assert_eq!(wrapper.0, outcome.body);
        assert_eq!(wrapper.1, outcome.issues);
    }

    #[test]
    fn ugc_and_time_mot_loc_emit_missing_reference_time_via_plan() {
        let plan = body_extraction_plan(UGC_AND_TIME_MOT_LOC_EXTRACTORS);
        let outcome = enrich_body_from_plan("plain text", &plan, None);

        assert_eq!(outcome.issues.len(), 2);
        assert_eq!(outcome.issues[0].code, "missing_reference_time");
        assert_eq!(outcome.issues[1].code, "missing_reference_time");
    }

    #[test]
    fn product_body_iter_location_points_collects_all_supported_sources() {
        let body = ProductBody {
            ugc: Some(vec![UgcSection {
                counties: std::collections::BTreeMap::from([(
                    "NE".to_string(),
                    vec![crate::UgcArea {
                        id: 1,
                        name: Some("County"),
                        lat: Some(41.3),
                        lon: Some(-96.1),
                    }],
                )]),
                zones: std::collections::BTreeMap::from([(
                    "NE".to_string(),
                    vec![crate::UgcArea {
                        id: 2,
                        name: Some("Zone"),
                        lat: Some(41.4),
                        lon: Some(-96.2),
                    }],
                )]),
                fire_zones: std::collections::BTreeMap::new(),
                marine_zones: std::collections::BTreeMap::new(),
                expires: Utc::now(),
            }]),
            hvtec: Some(vec![crate::HvtecCode {
                nwslid: "MSRM1".to_string(),
                location: Some(crate::NwslidEntry {
                    nwslid: "MSRM1",
                    state_code: "NE",
                    stream_name: "Stream",
                    proximity: "at",
                    place_name: "Place",
                    latitude: 41.5,
                    longitude: -96.3,
                }),
                severity: crate::HvtecSeverity::Major,
                cause: crate::HvtecCause::ExcessiveRainfall,
                begin: None,
                crest: None,
                end: None,
                record: crate::HvtecRecord::NoRecord,
            }]),
            time_mot_loc: Some(vec![TimeMotLocEntry {
                time_utc: Utc::now(),
                direction_degrees: 300,
                speed_kt: 25,
                points: vec![(41.6, -96.4), (41.7, -96.5)],
                wkt: "LINESTRING(-96.4000 41.6000,-96.5000 41.7000)".to_string(),
            }]),
            ..ProductBody::default()
        };

        let points = body.iter_location_points().collect::<Vec<_>>();

        assert_eq!(
            points,
            vec![
                GeoPoint {
                    lat: 41.6,
                    lon: -96.4,
                },
                GeoPoint {
                    lat: 41.7,
                    lon: -96.5,
                },
                GeoPoint {
                    lat: 41.3,
                    lon: -96.1,
                },
                GeoPoint {
                    lat: 41.4,
                    lon: -96.2,
                },
                GeoPoint {
                    lat: 41.5,
                    lon: -96.3,
                },
            ]
        );
    }

    #[test]
    fn product_body_iter_polygons_yields_bounds_and_points() {
        let body = ProductBody {
            latlon: Some(vec![LatLonPolygon {
                points: vec![
                    (41.0, -97.0),
                    (42.0, -97.0),
                    (42.0, -95.0),
                    (41.0, -95.0),
                    (41.0, -97.0),
                ],
                wkt: "POLYGON((-97.0000 41.0000,-97.0000 42.0000,-95.0000 42.0000,-95.0000 41.0000,-97.0000 41.0000))"
                    .to_string(),
            }]),
            ..ProductBody::default()
        };

        let polygons = body.iter_polygons().collect::<Vec<_>>();

        assert_eq!(polygons.len(), 1);
        assert_eq!(polygons[0].points.len(), 5);
        assert_eq!(
            polygons[0].bounds,
            Some(GeoBounds {
                min_lat: 41.0,
                max_lat: 42.0,
                min_lon: -97.0,
                max_lon: -95.0,
            })
        );
    }

    #[test]
    fn enrich_body_with_unknown_pil_is_empty() {
        let text = "Some product text with VTEC /O.NEW.KDMX.TO.W.0001.250301T1200Z-250301T1300Z/";
        let reference_time = Some(Utc::now());
        let (body, warnings) = enrich_body(text, "ZZZ", reference_time);

        assert!(body.is_none());
        assert!(warnings.is_empty());
    }

    #[test]
    fn enrich_body_empty_result_when_no_matches() {
        let text = "Plain text with no codes";
        let reference_time = Some(Utc::now());
        let (body, warnings) = enrich_body(text, "FFW", reference_time);

        assert!(body.is_none());
        assert!(warnings.is_empty());
    }

    #[test]
    fn plan_driven_qc_emits_vtec_missing_required_polygon() {
        let text = "/O.NEW.KOAX.SV.W.0001.250305T1200Z-250305T1800Z/\nTIME...MOT...LOC 1200Z 300DEG 25KT 4143 9613";
        let plan = body_extraction_plan(VTEC_LATLON_TIME_MOT_LOC_EXTRACTORS);

        let outcome = enrich_body_from_plan(text, &plan, Some(Utc::now()));
        assert!(
            outcome
                .issues
                .iter()
                .any(|issue| issue.code == "vtec_missing_required_polygon")
        );
    }

    #[test]
    fn plan_driven_qc_skips_vtec_missing_required_polygon_for_marine_only_ugc() {
        let body = ProductBody {
            vtec: Some(crate::parse_vtec_codes(
                "/O.CON.KBUF.GL.W.0007.000000T0000Z-260312T1200Z/",
            )),
            ugc: Some(vec![crate::UgcSection {
                counties: std::collections::BTreeMap::new(),
                zones: std::collections::BTreeMap::new(),
                fire_zones: std::collections::BTreeMap::new(),
                marine_zones: std::collections::BTreeMap::from([(
                    "LO".to_string(),
                    vec![crate::UgcArea {
                        id: 42,
                        name: None,
                        lat: None,
                        lon: None,
                    }],
                )]),
                expires: Utc::now(),
            }]),
            ..ProductBody::default()
        };

        let issues = validate_body_qc(
            &body,
            &BodyExtractionPlan {
                extractors: &[
                    BodyExtractorId::Vtec,
                    BodyExtractorId::Ugc,
                    BodyExtractorId::LatLon,
                ],
                qc_rules: QC_VTEC_POLYGON_AND_UGC,
            },
        );

        assert!(
            !issues
                .iter()
                .any(|issue| issue.code == "vtec_missing_required_polygon")
        );
    }

    #[test]
    fn plan_driven_qc_emits_ugc_duplicate_code() {
        let plan = body_extraction_plan(UGC_ONLY_EXTRACTORS);
        let outcome = enrich_body_from_plan("IAC001-IAC001-041200-\n", &plan, Some(Utc::now()));

        assert!(
            outcome
                .issues
                .iter()
                .any(|issue| issue.code == "ugc_duplicate_code")
        );
    }
}
