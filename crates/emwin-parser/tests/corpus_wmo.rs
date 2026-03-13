mod common;

use common::{assert_wmo, enrich, fixture_cases, matches_any};
use emwin_parser::enrich_product;

#[test]
fn metar_corpus_routes_to_wmo_bulletins() {
    for case in fixture_cases("wmo", "metar_collective") {
        let enrichment = enrich(&case);
        if enrichment.family != Some("metar_collective") {
            assert!(
                matches!(
                    enrichment.family,
                    Some("nws_text_product") | Some("unsupported_wmo_bulletin")
                ),
                "{} -> expected generic or unsupported-wmo fallback, got {:?}",
                case.name,
                enrichment.family
            );
            assert!(
                enrichment.parsed.is_none(),
                "{} -> expected unstructured METAR fallback",
                case.name
            );
            continue;
        }
        let artifact = assert_wmo(&enrichment, "metar_collective", &case, &[]);
        let bulletin = artifact
            .as_metar()
            .unwrap_or_else(|| panic!("{} -> expected METAR artifact", case.name));
        assert!(
            !bulletin.reports.is_empty(),
            "{} -> expected METAR reports",
            case.name
        );
    }
}

#[test]
fn taf_corpus_routes_to_wmo_bulletins() {
    for case in fixture_cases("wmo", "taf_bulletin") {
        let enrichment = enrich(&case);
        if enrichment.family != Some("taf_bulletin") {
            assert!(
                matches!(
                    enrichment.family,
                    Some("nws_text_product") | Some("unsupported_wmo_bulletin")
                ),
                "{} -> expected generic or unsupported-wmo fallback, got {:?}",
                case.name,
                enrichment.family
            );
            assert!(
                enrichment.parsed.is_none(),
                "{} -> expected unstructured TAF fallback",
                case.name
            );
            continue;
        }
        let artifact = assert_wmo(&enrichment, "taf_bulletin", &case, &[]);
        let bulletin = artifact
            .as_taf()
            .unwrap_or_else(|| panic!("{} -> expected TAF artifact", case.name));
        assert!(
            !bulletin.station.trim().is_empty(),
            "{} -> expected station",
            case.name
        );
    }
}

#[test]
fn sigmet_corpus_routes_to_wmo_bulletins() {
    for case in fixture_cases("wmo", "sigmet_bulletin") {
        let enrichment = enrich(&case);
        if enrichment.family != Some("sigmet_bulletin") {
            assert!(
                matches!(
                    enrichment.family,
                    Some("nws_text_product") | Some("unsupported_wmo_bulletin")
                ),
                "{} -> expected generic or unsupported-wmo fallback, got {:?}",
                case.name,
                enrichment.family
            );
            assert!(
                enrichment.parsed.is_none(),
                "{} -> expected unstructured SIGMET fallback",
                case.name
            );
            continue;
        }
        let artifact = assert_wmo(&enrichment, "sigmet_bulletin", &case, &[]);
        let bulletin = artifact
            .as_sigmet()
            .unwrap_or_else(|| panic!("{} -> expected SIGMET artifact", case.name));
        assert!(
            !bulletin.sections.is_empty(),
            "{} -> expected SIGMET sections",
            case.name
        );
        if !matches_any(&case.name, &["cancel"]) {
            assert!(
                bulletin
                    .sections
                    .iter()
                    .any(|section| !section.raw.trim().is_empty()),
                "{} -> expected populated SIGMET sections",
                case.name
            );
        }
    }
}

#[test]
fn fd_corpus_routes_to_wmo_bulletins() {
    for case in fixture_cases("wmo", "fd_bulletin") {
        let enrichment = enrich(&case);
        if enrichment.family != Some("fd_bulletin") {
            assert!(
                matches!(
                    enrichment.family,
                    Some("nws_text_product") | Some("unsupported_wmo_bulletin")
                ),
                "{} -> expected generic or unsupported-wmo fallback, got {:?}",
                case.name,
                enrichment.family
            );
            assert!(
                enrichment.parsed.is_none(),
                "{} -> expected unstructured FD fallback",
                case.name
            );
            continue;
        }
        let artifact = assert_wmo(&enrichment, "fd_bulletin", &case, &[]);
        let bulletin = artifact
            .as_fd()
            .unwrap_or_else(|| panic!("{} -> expected FD artifact", case.name));
        assert!(
            !bulletin.forecasts.is_empty(),
            "{} -> expected FD forecasts",
            case.name
        );
    }
}

#[test]
fn dcp_corpus_routes_to_wmo_bulletins() {
    for case in fixture_cases("wmo", "dcp_telemetry_bulletin") {
        let filename = if case.name == "SXMS50.TXT" {
            "MISDCPSV.TXT"
        } else {
            &case.name
        };
        let enrichment = enrich_product(filename, &case.bytes);
        let artifact = assert_wmo(&enrichment, "dcp_telemetry_bulletin", &case, &[]);
        let bulletin = artifact
            .as_dcp()
            .unwrap_or_else(|| panic!("{} -> expected DCP artifact", case.name));
        assert!(
            bulletin.platform_id.is_some() || !bulletin.lines.is_empty(),
            "{} -> expected DCP payload",
            case.name
        );
    }
}
