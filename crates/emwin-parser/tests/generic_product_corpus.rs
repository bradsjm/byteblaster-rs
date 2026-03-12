//! Real-product corpus coverage for generic body enrichment paths.
//!
//! Fixture provenance:
//! - AFOS-backed products were archived from Iowa Mesonet `api/1/nwstext/{product_id}`.

use emwin_parser::enrich_product;

#[test]
fn mww_marine_only_ugc_does_not_require_polygon() {
    let text = include_bytes!("fixtures/products/generic/marine_weather_message/MWWBUFNY.TXT");
    let enrichment = enrich_product("MWWBUFNY.TXT", text);

    assert!(
        !enrichment
            .issues
            .iter()
            .any(|issue| issue.code == "vtec_segment_missing_required_polygon"),
        "{:#?}",
        enrichment.issues
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
fn flood_product_url_does_not_trigger_hvtec_issue() {
    let text = include_bytes!("fixtures/products/generic/flood_warning/FLWSHVLA.TXT");
    let enrichment = enrich_product("FLWSHVLA.TXT", text);

    assert!(
        !enrichment
            .issues
            .iter()
            .any(|issue| issue.code == "invalid_hvtec_format"),
        "{:#?}",
        enrichment.issues
    );
}

#[test]
fn latlon_inline_terminator_does_not_trigger_issue() {
    let text = include_bytes!("fixtures/products/generic/airport_weather_warning/AWWUNVPA.TXT");
    let enrichment = enrich_product("AWWUNVPA.TXT", text);

    assert!(
        !enrichment
            .issues
            .iter()
            .any(|issue| issue.code == "invalid_latlon_coordinate_format"),
        "{:#?}",
        enrichment.issues
    );
}

#[test]
fn standards_compliant_ugc_shorthand_now_parses() {
    for (filename, bytes) in [
        (
            "SFTPIHID.TXT",
            include_bytes!("fixtures/products/generic/tabular_state_forecast/SFTPIHID.TXT")
                .as_slice(),
        ),
        (
            "SFTLWXDC.TXT",
            include_bytes!("fixtures/products/generic/tabular_state_forecast/SFTLWXDC.TXT")
                .as_slice(),
        ),
        (
            "RWRMTMT.TXT",
            include_bytes!("fixtures/products/generic/regional_weather_roundup/RWRMTMT.TXT")
                .as_slice(),
        ),
    ] {
        let enrichment = enrich_product(filename, bytes);

        assert!(
            !enrichment
                .issues
                .iter()
                .any(|issue| issue.code == "invalid_ugc_codes"),
            "{filename}: {:#?}",
            enrichment.issues
        );
    }
}

#[test]
fn malformed_ugc_source_still_reports_issue() {
    let text = include_bytes!("fixtures/products/generic/gateway_observations/OPUCHSSC.TXT");
    let enrichment = enrich_product("OPUCHSSC.TXT", text);

    assert!(
        enrichment
            .issues
            .iter()
            .any(|issue| issue.code == "invalid_ugc_codes"),
        "{:#?}",
        enrichment.issues
    );
}

#[test]
fn wind_hail_format_variants_no_longer_trigger_issues() {
    for (filename, bytes) in [
        (
            "SMWHFOHI.TXT",
            include_bytes!("fixtures/products/generic/special_marine_warning/SMWHFOHI.TXT")
                .as_slice(),
        ),
        (
            "SVSCTPPA.TXT",
            include_bytes!("fixtures/products/generic/severe_weather_statement/SVSCTPPA.TXT")
                .as_slice(),
        ),
    ] {
        let enrichment = enrich_product(filename, bytes);

        assert!(
            !enrichment.issues.iter().any(|issue| {
                matches!(
                    issue.code,
                    "invalid_wind_hail_wind_value" | "invalid_wind_hail_hail_value"
                )
            }),
            "{filename}: {:#?}",
            enrichment.issues
        );
    }
}
