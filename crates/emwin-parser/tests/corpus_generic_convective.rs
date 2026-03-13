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
        if case.name == "SVSCTPPA.TXT" {
            let body = enrichment
                .body
                .as_ref()
                .and_then(|body| body.as_vtec_event())
                .unwrap_or_else(|| panic!("{} -> expected vtec event body", case.name));
            assert!(
                !body.segments.is_empty(),
                "{} -> expected segments",
                case.name
            );
            assert!(
                !body.segments[0].vtec.is_empty(),
                "{} -> expected VTEC entries",
                case.name
            );
            assert!(
                !body.segments[0].ugc_sections.is_empty(),
                "{} -> expected UGC sections",
                case.name
            );
            assert!(
                !body.segments[0].polygons.is_empty(),
                "{} -> expected polygons",
                case.name
            );
            assert!(
                !body.segments[0].time_mot_loc.is_empty(),
                "{} -> expected TIME...MOT...LOC entries",
                case.name
            );
        }
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
