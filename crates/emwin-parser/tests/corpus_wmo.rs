mod common;

use common::{assert_supported_family, assert_wmo, enrich, fixture_cases};
use emwin_parser::enrich_product;

#[test]
fn metar_corpus_routes_to_wmo_bulletins() {
    for case in fixture_cases("wmo", "metar_collective") {
        let enrichment = enrich(&case);
        assert_supported_family(
            &enrichment,
            "metar_collective",
            &case,
            &["invalid_metar_bulletin", "invalid_metar_report"],
        );
        let Some(artifact) = enrichment.parsed.as_ref() else {
            continue;
        };
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
        assert_supported_family(
            &enrichment,
            "taf_bulletin",
            &case,
            &["invalid_taf_bulletin"],
        );
        let Some(artifact) = enrichment.parsed.as_ref() else {
            continue;
        };
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
        assert_supported_family(
            &enrichment,
            "sigmet_bulletin",
            &case,
            &["invalid_sigmet_bulletin"],
        );
        let Some(artifact) = enrichment.parsed.as_ref() else {
            continue;
        };
        let bulletin = artifact
            .as_sigmet()
            .unwrap_or_else(|| panic!("{} -> expected SIGMET artifact", case.name));
        assert!(
            !bulletin.sections.is_empty(),
            "{} -> expected SIGMET sections",
            case.name
        );
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

#[test]
fn fd_corpus_routes_to_wmo_bulletins() {
    for case in fixture_cases("wmo", "fd_bulletin") {
        let enrichment = enrich(&case);
        assert_supported_family(
            &enrichment,
            "fd_bulletin",
            &case,
            &["invalid_fd_bulletin", "missing_reference_time"],
        );
        let Some(artifact) = enrichment.parsed.as_ref() else {
            continue;
        };
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
