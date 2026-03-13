//! Real-product corpus coverage for WMO-only bulletin families.

use emwin_parser::enrich_product;

// Fixture provenance:
// - `SABZ31.TXT` is a tracked real bulletin text fixture for a WMO-only collective that
//   was not discoverable via the documented Iowa Mesonet AFOS list endpoint.

#[test]
fn compact_metar_prefix_does_not_trigger_issue() {
    let text = include_bytes!("fixtures/products/wmo/metar_collective/SABZ31.TXT");
    let enrichment = enrich_product("SABZ31.TXT", text);

    assert!(
        !enrichment
            .issues
            .iter()
            .any(|issue| issue.code == "invalid_metar_report"),
        "{:#?}",
        enrichment.issues
    );
}

#[test]
fn taf_fixture_routes_to_structured_wmo_bulletin() {
    let text = include_bytes!("fixtures/products/wmo/taf_bulletin/FTXX01.TXT");
    let enrichment = enrich_product("FTXX01.TXT", text);

    assert_eq!(enrichment.family, Some("taf_bulletin"));
    let taf = enrichment
        .parsed
        .as_ref()
        .and_then(|artifact| artifact.as_taf())
        .expect("expected TAF bulletin");
    assert_eq!(taf.station, "WBCF");
    assert!(taf.amendment);
}

#[test]
fn sigmet_fixture_routes_to_structured_wmo_bulletin() {
    let text = include_bytes!("fixtures/products/wmo/sigmet_bulletin/WVID21.TXT");
    let enrichment = enrich_product("WVID21.TXT", text);

    assert_eq!(enrichment.family, Some("sigmet_bulletin"));
    let sigmet = enrichment
        .parsed
        .as_ref()
        .and_then(|artifact| artifact.as_sigmet())
        .expect("expected SIGMET bulletin");
    assert_eq!(sigmet.sections.len(), 1);
    assert_eq!(sigmet.sections[0].identifier.as_deref(), Some("05"));
}

#[test]
fn dcp_fixture_routes_to_structured_wmo_bulletin() {
    let text = include_bytes!("fixtures/products/wmo/dcp_telemetry_bulletin/SXMS50.TXT");
    let enrichment = enrich_product("MISDCPSV.TXT", text);

    assert_eq!(enrichment.family, Some("dcp_telemetry_bulletin"));
    let dcp = enrichment
        .parsed
        .as_ref()
        .and_then(|artifact| artifact.as_dcp())
        .expect("expected DCP bulletin");
    assert_eq!(dcp.platform_id.as_deref(), Some("83786162 066025814"));
}

#[test]
fn fd_fixture_routes_to_structured_wmo_bulletin() {
    let text = include_bytes!("fixtures/products/wmo/fd_bulletin/FDUS80.TXT");
    let enrichment = enrich_product("FDUS80.TXT", text);

    assert_eq!(enrichment.family, Some("fd_bulletin"));
    let fd = enrichment
        .parsed
        .as_ref()
        .and_then(|artifact| artifact.as_fd())
        .expect("expected FD bulletin");
    assert_eq!(fd.forecasts.len(), 1);
}
