//! Regression coverage for products with repeated UGC sections.

use emwin_parser::enrich_product;

#[test]
fn product_with_duplicate_ugc_blocks_collects_both_sections() {
    let enrichment = enrich_product(
        "FLSRAH.txt",
        include_bytes!("fixtures/products/generic/flood_statement/FLSRAH.txt"),
    );

    assert!(enrichment.issues.is_empty());

    let sections = enrichment
        .body
        .as_ref()
        .and_then(|body| body.ugc.as_ref())
        .expect("expected parsed UGC sections");

    assert_eq!(sections.len(), 2);

    for section in sections {
        assert_eq!(
            section.counties["NC"]
                .iter()
                .map(|area| area.id)
                .collect::<Vec<_>>(),
            vec![101]
        );
    }
}
