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
