mod common;

use common::{assert_vtec_body, enrich, fixture_cases, matches_any};

#[test]
fn flood_watch_corpus_uses_vtec_event_body() {
    for case in fixture_cases("generic", "flood_watch") {
        let enrichment = enrich(&case);
        assert_vtec_body(&enrichment, &case);
    }
}

#[test]
fn flash_flood_warning_corpus_uses_vtec_event_body() {
    for case in fixture_cases("generic", "flash_flood_warning") {
        let enrichment = enrich(&case);
        assert_vtec_body(&enrichment, &case);
    }
}

#[test]
fn flood_statement_corpus_uses_vtec_event_body() {
    for case in fixture_cases("generic", "flood_statement") {
        let enrichment = enrich(&case);
        assert_vtec_body(&enrichment, &case);
        if !matches_any(&case.name, &["notime", "indexerror"]) {
            let body = enrichment
                .body
                .as_ref()
                .and_then(|body| body.as_vtec_event());
            assert!(
                body.is_some_and(|body| !body.segments.is_empty()),
                "{} -> expected parsed flood statement segments",
                case.name
            );
        }
        if case.name == "FLSRAH.txt" {
            assert!(
                enrichment
                    .issues
                    .iter()
                    .any(|issue| issue.code == "vtec_segment_duplicate_ugc_code"),
                "{} -> expected duplicate UGC issue",
                case.name
            );
            let body = enrichment
                .body
                .as_ref()
                .and_then(|body| body.as_vtec_event())
                .unwrap_or_else(|| panic!("{} -> expected vtec event body", case.name));
            let sections = &body.segments[0].ugc_sections;
            assert_eq!(
                sections.len(),
                2,
                "{} -> expected duplicate UGC blocks",
                case.name
            );
            for section in sections {
                assert_eq!(
                    section.counties["NC"]
                        .iter()
                        .map(|area| area.id)
                        .collect::<Vec<_>>(),
                    vec![101],
                    "{} -> expected duplicated NC county section",
                    case.name
                );
            }
        }
    }
}

#[test]
fn flood_warning_corpus_uses_vtec_event_body() {
    for case in fixture_cases("generic", "flood_warning") {
        let enrichment = enrich(&case);
        assert_vtec_body(&enrichment, &case);
        if case.name == "FLWSHVLA.TXT" {
            let body = enrichment
                .body
                .as_ref()
                .and_then(|body| body.as_vtec_event())
                .unwrap_or_else(|| panic!("{} -> expected vtec event body", case.name));
            assert!(
                body.segments
                    .iter()
                    .any(|segment| !segment.hvtec.is_empty()),
                "{} -> expected HVTEC to remain attached to a VTEC segment",
                case.name
            );
        }
    }
}
