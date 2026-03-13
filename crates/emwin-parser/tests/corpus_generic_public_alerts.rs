mod common;

use common::{assert_family, assert_vtec_body, enrich, fixture_cases};

#[test]
fn watch_county_notification_corpus_uses_vtec_event_body() {
    for case in fixture_cases("generic", "watch_county_notification") {
        let enrichment = enrich(&case);
        assert_vtec_body(&enrichment, &case);
    }
}

#[test]
fn snow_squall_warning_corpus_uses_vtec_event_body() {
    for case in fixture_cases("generic", "snow_squall_warning") {
        let enrichment = enrich(&case);
        assert_vtec_body(&enrichment, &case);
    }
}

#[test]
fn special_weather_statement_corpus_preserves_generic_text_body() {
    for case in fixture_cases("generic", "special_weather_statement") {
        let enrichment = enrich(&case);
        assert_family(&enrichment, "nws_text_product", &case);
        assert!(
            enrichment.body.is_some(),
            "{} -> expected parsed body for special weather statement",
            case.name
        );
    }
}

#[test]
fn vtec_regression_corpus_keeps_text_products_classified() {
    for case in fixture_cases("generic", "vtec_regression") {
        let enrichment = enrich(&case);
        assert_family(&enrichment, "nws_text_product", &case);
        assert!(
            enrichment.body.is_some() || !enrichment.issues.is_empty(),
            "{} -> expected parsed body or explicit issues",
            case.name
        );
    }
}
