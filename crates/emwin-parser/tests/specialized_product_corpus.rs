//! Real-product corpus coverage for specialized bulletin families.
//!
//! Fixture provenance:
//! - `SAW` and `SEL` fixtures were archived from Iowa Mesonet
//!   `api/1/nwstext/{product_id}`.

use emwin_parser::{
    CwaGeometryKind, ProductArtifact, ProductEnrichmentSource, SpcWatchType, enrich_product,
};

#[test]
fn exact_lsr_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603100015-KBMX-NWUS54-LSRBMX.txt",
        include_bytes!("fixtures/products/specialized/lsr/202603100015-KBMX-NWUS54-LSRBMX.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
    assert_eq!(enrichment.family, Some("lsr_bulletin"));
    assert!(enrichment.body.is_none());
    assert!(
        enrichment
            .parsed
            .as_ref()
            .and_then(ProductArtifact::as_cwa)
            .is_none()
    );
    assert!(
        enrichment
            .parsed
            .as_ref()
            .and_then(ProductArtifact::as_wwp)
            .is_none()
    );

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_lsr)
        .expect("expected LSR bulletin");
    assert_eq!(bulletin.reports.len(), 1);
    assert_eq!(bulletin.reports[0].city, "Brooksville");
    assert_eq!(bulletin.reports[0].state.as_deref(), Some("AL"));
}

#[test]
fn exact_lsr_edge_case_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "LSRCYSWY.TXT",
        include_bytes!("fixtures/products/specialized/lsr/LSRCYSWY.TXT"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
    assert_eq!(enrichment.family, Some("lsr_bulletin"));
    assert!(
        !enrichment
            .issues
            .iter()
            .any(|issue| issue.code == "invalid_lsr_report"),
        "{:#?}",
        enrichment.issues
    );

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_lsr)
        .expect("expected LSR bulletin");
    assert!(
        bulletin
            .reports
            .iter()
            .any(|report| report.city == "7 NW Elk Mountain")
    );
    assert!(bulletin.reports.iter().any(|report| {
        report.city == "7 NW Elk Mountain"
            && report.magnitude_value == Some(63.0)
            && report.magnitude_units.as_deref() == Some("MPH")
            && report.magnitude_qualifier.as_deref() == Some("M")
    }));
}

#[test]
fn exact_cwa_active_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603100229-KZLC-FAUS22-CWAZLC.txt",
        include_bytes!("fixtures/products/specialized/cwa/202603100229-KZLC-FAUS22-CWAZLC.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::WmoBulletin);
    assert_eq!(enrichment.family, Some("cwa_bulletin"));
    assert!(enrichment.body.is_none());
    assert!(
        enrichment
            .parsed
            .as_ref()
            .and_then(ProductArtifact::as_lsr)
            .is_none()
    );

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_cwa)
        .expect("expected CWA bulletin");
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
        include_bytes!("fixtures/products/specialized/cwa/202603100038-KZFW-FAUS24-CWAZFW.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::WmoBulletin);
    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_cwa)
        .expect("expected cancel CWA bulletin");
    assert!(bulletin.is_cancelled);
    assert!(bulletin.geometry.is_none());
}

#[test]
fn exact_wwp_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603102008-KWNS-WWUS40-WWP1.txt",
        include_bytes!("fixtures/products/specialized/wwp/202603102008-KWNS-WWUS40-WWP1.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
    assert_eq!(enrichment.family, Some("wwp_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_wwp)
        .expect("expected WWP bulletin");
    assert_eq!(bulletin.watch_number, 31);
    assert_eq!(bulletin.watch_type, SpcWatchType::Tornado);
    assert_eq!(bulletin.max_tops_feet, 50_000);
    assert!(!bulletin.is_pds);
}

#[test]
fn exact_saw_issue_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202507251740-KWNS-WWUS30-SAW2.txt",
        include_bytes!("fixtures/products/specialized/saw/202507251740-KWNS-WWUS30-SAW2.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
    assert_eq!(enrichment.family, Some("saw_bulletin"));
    assert!(enrichment.body.is_some());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_saw)
        .expect("expected SAW bulletin");
    assert_eq!(bulletin.saw_number, 2);
    assert_eq!(bulletin.watch_number, 542);
    assert_eq!(bulletin.watch_type, SpcWatchType::SevereThunderstorm);
    assert!(matches!(bulletin.action, emwin_parser::SawAction::Issue));
    assert!(
        bulletin
            .polygon
            .as_ref()
            .is_some_and(|points| !points.is_empty())
    );

    let body = enrichment.body.expect("expected generic body");
    assert!(
        body.as_generic()
            .and_then(|body| body.latlon.as_ref())
            .is_some()
    );
}

#[test]
fn exact_saw_cancel_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202507250013-KWNS-WWUS30-SAW0.txt",
        include_bytes!("fixtures/products/specialized/saw/202507250013-KWNS-WWUS30-SAW0.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
    assert_eq!(enrichment.family, Some("saw_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_saw)
        .expect("expected SAW bulletin");
    assert_eq!(bulletin.saw_number, 0);
    assert_eq!(bulletin.watch_number, 540);
    assert!(matches!(bulletin.action, emwin_parser::SawAction::Cancel));
    assert!(bulletin.polygon.is_none());
}

#[test]
fn exact_sel_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202507251745-KWNS-WWUS20-SEL2.txt",
        include_bytes!("fixtures/products/specialized/sel/202507251745-KWNS-WWUS20-SEL2.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
    assert_eq!(enrichment.family, Some("sel_bulletin"));
    assert!(enrichment.body.is_some());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_sel)
        .expect("expected SEL bulletin");
    assert_eq!(bulletin.watch_number, 542);
    assert_eq!(bulletin.watch_type, SpcWatchType::SevereThunderstorm);
    assert!(!bulletin.is_test);

    let body = enrichment.body.expect("expected generic body");
    assert!(
        body.as_generic()
            .and_then(|body| body.ugc.as_ref())
            .is_some()
    );
}

#[test]
fn exact_sel_test_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202001271450-KWNS-WWUS20-SEL9.txt",
        include_bytes!("fixtures/products/specialized/sel/202001271450-KWNS-WWUS20-SEL9.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
    assert_eq!(enrichment.family, Some("sel_bulletin"));
    assert!(enrichment.body.is_some());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_sel)
        .expect("expected SEL bulletin");
    assert_eq!(bulletin.watch_number, 9999);
    assert_eq!(bulletin.watch_type, SpcWatchType::SevereThunderstorm);
    assert!(bulletin.is_test);

    let body = enrichment.body.expect("expected generic body");
    assert!(
        body.as_generic()
            .and_then(|body| body.ugc.as_ref())
            .is_some()
    );
}

#[test]
fn exact_cf6_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603100030-PGUM-CXGM50-CF6GSN.txt",
        include_bytes!("fixtures/products/specialized/cf6/202603100030-PGUM-CXGM50-CF6GSN.txt"),
    );

    assert_eq!(
        enrichment.source,
        ProductEnrichmentSource::TextHeader,
        "{enrichment:#?}"
    );
    assert_eq!(enrichment.family, Some("cf6_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_cf6)
        .expect("expected CF6 bulletin");
    assert_eq!(bulletin.station, "SAIPAN/ISLEY_(CGS) MP");
    assert_eq!(bulletin.month, 3);
    assert_eq!(bulletin.year, 2026);
    assert_eq!(bulletin.rows.len(), 9);
}

#[test]
fn exact_dsm_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603110415-KABQ-CXUS45-DSMCQC.txt",
        include_bytes!("fixtures/products/specialized/dsm/202603110415-KABQ-CXUS45-DSMCQC.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
    assert_eq!(enrichment.family, Some("dsm_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_dsm)
        .expect("expected DSM bulletin");
    assert_eq!(bulletin.summaries.len(), 1);
    assert_eq!(bulletin.summaries[0].station, "KCQC");
    assert_eq!(bulletin.summaries[0].hourly_precip_inches.len(), 24);
}

#[test]
fn exact_hml_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603100002-KMTR-SRUS56-HMLMTR.txt",
        include_bytes!("fixtures/products/specialized/hml/202603100002-KMTR-SRUS56-HMLMTR.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
    assert_eq!(enrichment.family, Some("hml_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_hml)
        .expect("expected HML bulletin");
    assert!(bulletin.documents.len() > 1);
    assert_eq!(bulletin.documents[0].station_id, "AAMC1");
    assert!(bulletin.documents[0].observed.is_some());
}

#[test]
fn exact_met_mos_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603100000-KWNO-FOUS46-METBCK.txt",
        include_bytes!("fixtures/products/specialized/mos/202603100000-KWNO-FOUS46-METBCK.txt"),
    );

    assert_eq!(
        enrichment.source,
        ProductEnrichmentSource::TextHeader,
        "{enrichment:#?}"
    );
    assert_eq!(enrichment.family, Some("mos_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_mos)
        .expect("expected MOS bulletin");
    assert_eq!(bulletin.sections.len(), 1);
    assert_eq!(bulletin.sections[0].station, "KBCK");
    assert_eq!(bulletin.sections[0].model, "NAM");
    assert!(bulletin.sections[0].forecasts[0].values.contains_key("WSP"));
}

#[test]
fn exact_ftp_mos_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603100000-KWNO-FOAK12-FTPACR.txt",
        include_bytes!("fixtures/products/specialized/mos/202603100000-KWNO-FOAK12-FTPACR.txt"),
    );

    assert_eq!(
        enrichment.source,
        ProductEnrichmentSource::TextHeader,
        "{enrichment:#?}"
    );
    assert_eq!(enrichment.family, Some("mos_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_mos)
        .expect("expected FTP MOS bulletin");
    assert!(!bulletin.sections.is_empty());
    assert_eq!(bulletin.sections[0].station, "AHP");
}

#[test]
fn exact_mcd_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603120208-KWNS-ACUS11-SWOMCD.txt",
        include_bytes!("fixtures/products/specialized/mcd/202603120208-KWNS-ACUS11-SWOMCD.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
    assert_eq!(enrichment.family, Some("mcd_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_mcd)
        .expect("expected MCD bulletin");
    assert_eq!(bulletin.discussion_number, 1525);
    assert!(bulletin.polygon.is_some());
}

#[test]
fn exact_ero_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202107132156-KWBC-FOUS30-RBG94E.txt",
        include_bytes!("fixtures/products/specialized/ero/202107132156-KWBC-FOUS30-RBG94E.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
    assert_eq!(enrichment.family, Some("ero_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_ero)
        .expect("expected ERO bulletin");
    assert_eq!(bulletin.day, 1);
    assert_eq!(bulletin.outlooks.len(), 1);
    assert_eq!(bulletin.outlooks[0].threshold, "MRGL");
    assert!(bulletin.outlooks[0].polygon.is_none());
}

#[test]
fn exact_spc_outlook_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603071300-KWNS-WUUS01-PTSDY1.txt",
        include_bytes!(
            "fixtures/products/specialized/spc_outlook/202603071300-KWNS-WUUS01-PTSDY1.txt"
        ),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
    assert_eq!(enrichment.family, Some("spc_outlook_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_spc_outlook)
        .expect("expected SPC outlook bulletin");
    assert_eq!(bulletin.days.len(), 1);
    assert_eq!(bulletin.days[0].day, 1);
    assert_eq!(bulletin.days[0].outlooks[0].threshold, "MRGL");
}

#[test]
fn exact_cli_product_parses_specialized_bulletin() {
    let enrichment = enrich_product(
        "202603110100-KDMX-CXUS53-CLIDSM.txt",
        include_bytes!("fixtures/products/specialized/cli/202603110100-KDMX-CXUS53-CLIDSM.txt"),
    );

    assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
    assert_eq!(enrichment.family, Some("cli_bulletin"));
    assert!(enrichment.body.is_none());

    let bulletin = enrichment
        .parsed
        .as_ref()
        .and_then(ProductArtifact::as_cli)
        .expect("expected CLI bulletin");
    assert_eq!(bulletin.reports.len(), 1);
    assert_eq!(bulletin.reports[0].station, "DES MOINES");
    assert_eq!(bulletin.reports[0].temperature_maximum, Some(72));
    assert_eq!(bulletin.reports[0].precip_today_inches, Some(0.1));
}
