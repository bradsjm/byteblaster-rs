mod common;

use common::{assert_family, assert_specialized, enrich, fixture_cases, matches_any};

#[test]
fn pirep_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "pirep") {
        let enrichment = enrich(&case);
        if enrichment.family != Some("pirep_bulletin") {
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
                "{} -> expected unstructured fallback for unsupported PIREP variant: {enrichment:#?}",
                case.name
            );
            continue;
        }

        let artifact = assert_specialized(&enrichment, "pirep_bulletin", &case, &[]);
        let bulletin = artifact
            .as_pirep()
            .unwrap_or_else(|| panic!("{} -> expected PIREP artifact", case.name));

        assert!(
            !bulletin.reports.is_empty(),
            "{} -> expected at least one PIREP report",
            case.name
        );
        assert!(
            bulletin
                .reports
                .iter()
                .all(|report| !report.raw.trim().is_empty()),
            "{} -> expected preserved raw reports",
            case.name
        );
    }
}

#[test]
fn lsr_corpus_routes_to_structured_bulletins() {
    let negative = ["LSR_empty"];

    for case in fixture_cases("specialized", "lsr") {
        let enrichment = enrich(&case);
        if matches_any(&case.name, &negative) {
            assert_family(&enrichment, "nws_text_product", &case);
            continue;
        }
        if enrichment.family != Some("lsr_bulletin") {
            assert_family(&enrichment, "nws_text_product", &case);
            assert!(
                enrichment.parsed.is_none(),
                "{} -> expected generic fallback for unstructured LSR variant",
                case.name
            );
            continue;
        }

        let artifact =
            assert_specialized(&enrichment, "lsr_bulletin", &case, &["invalid_lsr_report"]);
        let bulletin = artifact
            .as_lsr()
            .unwrap_or_else(|| panic!("{} -> expected LSR artifact", case.name));
        assert!(
            !bulletin.reports.is_empty(),
            "{} -> expected at least one LSR report",
            case.name
        );
    }
}

#[test]
fn cli_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "cli") {
        let enrichment = enrich(&case);
        let artifact = assert_specialized(&enrichment, "cli_bulletin", &case, &[]);
        let bulletin = artifact
            .as_cli()
            .unwrap_or_else(|| panic!("{} -> expected CLI artifact", case.name));
        assert!(
            !bulletin.reports.is_empty(),
            "{} -> expected CLI reports",
            case.name
        );
        assert!(
            bulletin
                .reports
                .iter()
                .all(|report| !report.station.trim().is_empty()),
            "{} -> expected CLI station names",
            case.name
        );
    }
}

#[test]
fn mos_corpus_routes_to_structured_bulletins() {
    let sparse = ["MET_empty", "NBSUSA_empty"];
    let unsupported = ["ECS", "LAV", "LEV", "NBE", "NBS", "NBX"];

    for case in fixture_cases("specialized", "mos") {
        let enrichment = enrich(&case);
        if matches_any(&case.name, &sparse) {
            assert_family(&enrichment, "nws_text_product", &case);
            assert!(
                enrichment.parsed.is_none(),
                "{} -> expected sparse MOS fallback",
                case.name
            );
            continue;
        }
        if matches_any(&case.name, &unsupported) || enrichment.family != Some("mos_bulletin") {
            assert_family(&enrichment, "nws_text_product", &case);
            assert!(
                enrichment.parsed.is_none(),
                "{} -> expected generic fallback for unsupported or currently unstructured MOS family",
                case.name
            );
            continue;
        }

        let artifact = assert_specialized(&enrichment, "mos_bulletin", &case, &[]);
        let bulletin = artifact
            .as_mos()
            .unwrap_or_else(|| panic!("{} -> expected MOS artifact", case.name));

        assert!(
            !bulletin.sections.is_empty(),
            "{} -> expected MOS sections",
            case.name
        );
        assert!(
            bulletin
                .sections
                .iter()
                .all(|section| !section.station.trim().is_empty()
                    && !section.model.trim().is_empty()),
            "{} -> expected populated MOS section metadata",
            case.name
        );
    }
}
