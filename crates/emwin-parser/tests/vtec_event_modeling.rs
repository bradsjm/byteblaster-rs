use emwin_parser::enrich_product;

#[test]
fn severe_warning_serializes_as_vtec_event_body() {
    let enrichment = enrich_product(
        "SVSCTPPA.TXT",
        include_bytes!("fixtures/products/generic/severe_weather_statement/SVSCTPPA.TXT"),
    );

    let body = enrichment
        .body
        .as_ref()
        .and_then(|body| body.as_vtec_event())
        .expect("expected vtec event body");

    assert!(!body.segments.is_empty());
    assert!(!body.segments[0].vtec.is_empty());
    assert!(!body.segments[0].ugc_sections.is_empty());
    assert!(!body.segments[0].polygons.is_empty());
    assert!(!body.segments[0].time_mot_loc.is_empty());
}

#[test]
fn flood_warning_keeps_hvtec_within_vtec_segment() {
    let enrichment = enrich_product(
        "FLWSHVLA.TXT",
        include_bytes!("fixtures/products/generic/flood_warning/FLWSHVLA.TXT"),
    );

    let body = enrichment
        .body
        .as_ref()
        .and_then(|body| body.as_vtec_event())
        .expect("expected vtec event body");

    assert!(
        body.segments
            .iter()
            .any(|segment| !segment.hvtec.is_empty())
    );
}

#[test]
fn marine_message_uses_vtec_event_body_without_polygon_issue() {
    let enrichment = enrich_product(
        "MWWBUFNY.TXT",
        include_bytes!("fixtures/products/generic/marine_weather_message/MWWBUFNY.TXT"),
    );

    assert!(
        !enrichment
            .issues
            .iter()
            .any(|issue| issue.code == "vtec_segment_missing_required_polygon")
    );
    assert!(
        enrichment
            .body
            .as_ref()
            .and_then(|body| body.as_vtec_event())
            .is_some()
    );
}

#[test]
fn airport_warning_uses_generic_body_when_no_vtec_is_present() {
    let enrichment = enrich_product(
        "AWWUNVPA.TXT",
        include_bytes!("fixtures/products/generic/airport_weather_warning/AWWUNVPA.TXT"),
    );

    let body = enrichment
        .body
        .as_ref()
        .and_then(|body| body.as_generic())
        .expect("expected generic body");

    assert!(
        body.latlon
            .as_ref()
            .is_some_and(|polygons| !polygons.is_empty())
    );
}
