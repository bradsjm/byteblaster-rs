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
    }
}

#[test]
fn flood_warning_corpus_uses_vtec_event_body() {
    for case in fixture_cases("generic", "flood_warning") {
        let enrichment = enrich(&case);
        assert_vtec_body(&enrichment, &case);
    }
}
