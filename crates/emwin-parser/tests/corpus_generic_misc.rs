mod common;

use common::{assert_generic_body, enrich, fixture_cases};

#[test]
fn airport_weather_warning_corpus_uses_generic_body() {
    for case in fixture_cases("generic", "airport_weather_warning") {
        let enrichment = enrich(&case);
        assert_generic_body(&enrichment, &case);
    }
}

#[test]
fn regional_weather_roundup_corpus_uses_generic_body() {
    for case in fixture_cases("generic", "regional_weather_roundup") {
        let enrichment = enrich(&case);
        assert_generic_body(&enrichment, &case);
    }
}

#[test]
fn tabular_state_forecast_corpus_uses_generic_body() {
    for case in fixture_cases("generic", "tabular_state_forecast") {
        let enrichment = enrich(&case);
        assert_generic_body(&enrichment, &case);
    }
}

#[test]
fn gateway_observations_corpus_uses_generic_body() {
    for case in fixture_cases("generic", "gateway_observations") {
        let enrichment = enrich(&case);
        assert_generic_body(&enrichment, &case);
    }
}
