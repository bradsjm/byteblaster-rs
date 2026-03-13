mod common;

use common::{assert_specialized, assert_supported_family, enrich, fixture_cases};

#[test]
fn pirep_corpus_routes_to_structured_bulletins() {
    for case in fixture_cases("specialized", "pirep") {
        let enrichment = enrich(&case);
        let artifact = assert_specialized(
            &enrichment,
            "pirep_bulletin",
            &case,
            &["invalid_pirep_bulletin"],
        );
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
    for case in fixture_cases("specialized", "lsr") {
        let enrichment = enrich(&case);
        assert_supported_family(
            &enrichment,
            "lsr_bulletin",
            &case,
            &["invalid_lsr_bulletin", "invalid_lsr_report"],
        );
        let Some(artifact) = enrichment.parsed.as_ref() else {
            continue;
        };
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
    for case in fixture_cases("specialized", "mos") {
        let enrichment = enrich(&case);
        assert_supported_family(
            &enrichment,
            "mos_bulletin",
            &case,
            &["invalid_mos_bulletin"],
        );
        let Some(artifact) = enrichment.parsed.as_ref() else {
            continue;
        };
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
