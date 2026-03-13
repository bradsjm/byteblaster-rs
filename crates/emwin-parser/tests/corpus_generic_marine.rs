mod common;

use common::{assert_family, assert_vtec_body, enrich, fixture_cases};

#[test]
fn marine_weather_message_corpus_uses_vtec_event_body() {
    for case in fixture_cases("generic", "marine_weather_message") {
        let enrichment = enrich(&case);
        if enrichment.body.is_none() {
            assert_family(&enrichment, "nws_text_product", &case);
            continue;
        }
        assert_vtec_body(&enrichment, &case);
    }
}

#[test]
fn special_marine_warning_corpus_uses_vtec_event_body() {
    for case in fixture_cases("generic", "special_marine_warning") {
        let enrichment = enrich(&case);
        assert_vtec_body(&enrichment, &case);
    }
}
