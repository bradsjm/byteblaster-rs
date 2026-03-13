mod common;

use common::{assert_family, assert_vtec_body, enrich, fixture_cases};

#[test]
fn severe_thunderstorm_warning_corpus_uses_vtec_event_body() {
    for case in fixture_cases("generic", "severe_thunderstorm_warning") {
        let enrichment = enrich(&case);
        if enrichment.body.is_none() {
            assert_family(&enrichment, "nws_text_product", &case);
            continue;
        }
        assert_vtec_body(&enrichment, &case);
    }
}

#[test]
fn severe_weather_statement_corpus_uses_vtec_event_body() {
    for case in fixture_cases("generic", "severe_weather_statement") {
        let enrichment = enrich(&case);
        assert_vtec_body(&enrichment, &case);
    }
}

#[test]
fn tornado_warning_corpus_uses_vtec_event_body() {
    for case in fixture_cases("generic", "tornado_warning") {
        let enrichment = enrich(&case);
        if enrichment.body.is_none() {
            assert_family(&enrichment, "nws_text_product", &case);
            continue;
        }
        assert_vtec_body(&enrichment, &case);
    }
}

#[test]
fn tornado_emergency_statement_corpus_uses_vtec_event_body() {
    for case in fixture_cases("generic", "tornado_emergency_statement") {
        let enrichment = enrich(&case);
        assert_vtec_body(&enrichment, &case);
    }
}
