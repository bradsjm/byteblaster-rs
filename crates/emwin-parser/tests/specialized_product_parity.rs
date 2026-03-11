//! Fixture-backed parity coverage for specialized AFOS bulletin families.

use emwin_parser::{CwaGeometryKind, ProductEnrichmentSource, WwpWatchType, enrich_product};

#[test]
fn exact_lsr_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603100015-KBMX-NWUS54-LSRBMX.txt",
        include_bytes!("fixtures/specialized/202603100015-KBMX-NWUS54-LSRBMX.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextLsrBulletin);
    assert_eq!(enrichment.family, Some("lsr_bulletin"));
    assert!(enrichment.body.is_none());
    assert!(enrichment.cwa.is_none());
    assert!(enrichment.wwp.is_none());

    let bulletin = enrichment.lsr.expect("expected LSR bulletin");
    assert_eq!(bulletin.reports.len(), 1);
    assert_eq!(bulletin.reports[0].city, "Brooksville");
    assert_eq!(bulletin.reports[0].state.as_deref(), Some("AL"));
}

#[test]
fn exact_cwa_active_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603100229-KZLC-FAUS22-CWAZLC.txt",
        include_bytes!("fixtures/specialized/202603100229-KZLC-FAUS22-CWAZLC.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextCwaBulletin);
    assert_eq!(enrichment.family, Some("cwa_bulletin"));
    assert!(enrichment.body.is_none());
    assert!(enrichment.lsr.is_none());

    let bulletin = enrichment.cwa.expect("expected CWA bulletin");
    assert!(!bulletin.is_cancelled);
    assert_eq!(bulletin.number, 202);
    assert!(matches!(
        bulletin.geometry.as_ref().map(|geometry| &geometry.kind),
        Some(CwaGeometryKind::Polygon)
    ));
    assert!(
        bulletin
            .geometry
            .as_ref()
            .is_some_and(|geometry| !geometry.points.is_empty())
    );
}

#[test]
fn exact_cwa_cancel_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603100038-KZFW-FAUS24-CWAZFW.txt",
        include_bytes!("fixtures/specialized/202603100038-KZFW-FAUS24-CWAZFW.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextCwaBulletin);
    let bulletin = enrichment.cwa.expect("expected cancel CWA bulletin");
    assert!(bulletin.is_cancelled);
    assert!(bulletin.geometry.is_none());
}

#[test]
fn exact_wwp_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603102008-KWNS-WWUS40-WWP1.txt",
        include_bytes!("fixtures/specialized/202603102008-KWNS-WWUS40-WWP1.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextWwpBulletin);
    assert_eq!(enrichment.family, Some("wwp_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment.wwp.expect("expected WWP bulletin");
    assert_eq!(bulletin.watch_number, 31);
    assert_eq!(bulletin.watch_type, WwpWatchType::Tornado);
    assert_eq!(bulletin.max_tops_feet, 50_000);
    assert!(!bulletin.is_pds);
}

#[test]
fn exact_cf6_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603100030-PGUM-CXGM50-CF6GSN.txt",
        include_bytes!("fixtures/specialized/202603100030-PGUM-CXGM50-CF6GSN.txt"),
    );

    assert_eq!(
        enrichment.source,
        ProductEnrichmentSource::TextCf6Bulletin,
        "{enrichment:#?}"
    );
    assert_eq!(enrichment.family, Some("cf6_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment.cf6.expect("expected CF6 bulletin");
    assert_eq!(bulletin.station, "SAIPAN/ISLEY_(CGS) MP");
    assert_eq!(bulletin.month, 3);
    assert_eq!(bulletin.year, 2026);
    assert_eq!(bulletin.rows.len(), 9);
}

#[test]
fn exact_dsm_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603110415-KABQ-CXUS45-DSMCQC.txt",
        include_bytes!("fixtures/specialized/202603110415-KABQ-CXUS45-DSMCQC.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextDsmBulletin);
    assert_eq!(enrichment.family, Some("dsm_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment.dsm.expect("expected DSM bulletin");
    assert_eq!(bulletin.summaries.len(), 1);
    assert_eq!(bulletin.summaries[0].station, "KCQC");
    assert_eq!(bulletin.summaries[0].hourly_precip_inches.len(), 24);
}

#[test]
fn exact_hml_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603100002-KMTR-SRUS56-HMLMTR.txt",
        include_bytes!("fixtures/specialized/202603100002-KMTR-SRUS56-HMLMTR.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHmlBulletin);
    assert_eq!(enrichment.family, Some("hml_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment.hml.expect("expected HML bulletin");
    assert!(bulletin.documents.len() > 1);
    assert_eq!(bulletin.documents[0].station_id, "AAMC1");
    assert!(bulletin.documents[0].observed.is_some());
}

#[test]
fn exact_met_mos_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603100000-KWNO-FOUS46-METBCK.txt",
        include_bytes!("fixtures/specialized/202603100000-KWNO-FOUS46-METBCK.txt"),
    );

    assert_eq!(
        enrichment.source,
        ProductEnrichmentSource::TextMosBulletin,
        "{enrichment:#?}"
    );
    assert_eq!(enrichment.family, Some("mos_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment.mos.expect("expected MOS bulletin");
    assert_eq!(bulletin.sections.len(), 1);
    assert_eq!(bulletin.sections[0].station, "KBCK");
    assert_eq!(bulletin.sections[0].model, "NAM");
    assert!(bulletin.sections[0].forecasts[0].values.contains_key("WSP"));
}

#[test]
fn exact_ftp_mos_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603100000-KWNO-FOAK12-FTPACR.txt",
        include_bytes!("fixtures/specialized/202603100000-KWNO-FOAK12-FTPACR.txt"),
    );

    assert_eq!(
        enrichment.source,
        ProductEnrichmentSource::TextMosBulletin,
        "{enrichment:#?}"
    );
    assert_eq!(enrichment.family, Some("mos_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment.mos.expect("expected FTP MOS bulletin");
    assert!(!bulletin.sections.is_empty());
    assert_eq!(bulletin.sections[0].station, "AHP");
}
