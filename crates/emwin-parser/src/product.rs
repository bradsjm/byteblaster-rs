//! Public product enrichment types and facade.
//!
//! The implementation now lives in the internal `pipeline` module. This file
//! retains the stable public result types and `enrich_product` entrypoint used
//! by downstream callers.

use crate::pipeline::{NormalizedInput, ParsedEnvelope, assemble_product_enrichment, classify};
use crate::{
    BbbKind, ProductBody, ProductParseIssue, TextProductHeader, WmoHeader, WmoOfficeEntry,
};
use crate::{DcpBulletin, FdBulletin, MetarBulletin, PirepBulletin, SigmetBulletin, TafBulletin};
use serde::Serialize;

/// Source of product enrichment data.
///
/// Indicates how the product metadata was derived:
/// - Text products: parsed from WMO/AFOS headers
/// - Non-text products: classified from filename patterns
/// - Unknown: unable to determine product type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductEnrichmentSource {
    TextHeader,
    WmoFdBulletin,
    TextPirepBulletin,
    TextSigmetBulletin,
    WmoSigmetBulletin,
    WmoMetarBulletin,
    WmoTafBulletin,
    WmoDcpBulletin,
    WmoUnsupportedBulletin,
    FilenameNonText,
    Unknown,
}

/// Enriched product metadata with classification, headers, and parsed content.
///
/// This struct contains all metadata extracted from a product, including
/// source classification, parsed headers, body elements (VTEC, UGC, polygons),
/// and any issues encountered during processing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProductEnrichment {
    /// How this enrichment was derived
    pub source: ProductEnrichmentSource,
    /// Product family classification (e.g., "nws_text_product", "metar_collective")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<&'static str>,
    /// Human-readable product title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<&'static str>,
    /// Container type ("raw", "zip")
    pub container: &'static str,
    /// Product Identifier Line (e.g., "SVR", "TOR")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pil: Option<String>,
    /// WMO header prefix (e.g., "WU", "WT")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wmo_prefix: Option<&'static str>,
    /// Originating WMO office information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub office: Option<WmoOfficeEntry>,
    /// Parsed text product header (AFOS)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<TextProductHeader>,
    /// Parsed WMO bulletin header
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wmo_header: Option<WmoHeader>,
    /// BBB amendment/correction type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbb_kind: Option<BbbKind>,
    /// Parsed body elements (VTEC, UGC, polygons, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<ProductBody>,
    /// Parsed METAR bulletin (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metar: Option<MetarBulletin>,
    /// Parsed TAF bulletin (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taf: Option<TafBulletin>,
    /// Parsed DCP telemetry bulletin (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dcp: Option<DcpBulletin>,
    /// Parsed FD winds/temps bulletin (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fd: Option<FdBulletin>,
    /// Parsed PIREP bulletin (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pirep: Option<PirepBulletin>,
    /// Parsed SIGMET bulletin (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sigmet: Option<SigmetBulletin>,
    /// Issues encountered during parsing
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub issues: Vec<ProductParseIssue>,
}

/// Enriches a product by running the internal parsing pipeline.
///
/// The public API remains stable while the implementation is staged internally
/// as normalization, envelope construction, classification, and assembly.
pub fn enrich_product(filename: &str, bytes: &[u8]) -> ProductEnrichment {
    let normalized = NormalizedInput::from_input(filename, bytes);
    let raw_bytes = normalized.bytes.clone();
    let envelope = ParsedEnvelope::build(normalized);
    let outcome = classify(&envelope);

    assemble_product_enrichment(outcome, filename, &raw_bytes)
}

#[cfg(test)]
mod tests {
    use crate::MetarBulletin;

    use super::{ProductEnrichmentSource, enrich_product};

    #[test]
    fn text_products_use_header_enrichment() {
        let enrichment =
            enrich_product("TAFPDKGA.TXT", b"000 \nFTUS42 KFFC 022320\nTAFPDK\nBody\n");

        assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
        assert_eq!(enrichment.pil.as_deref(), Some("TAF"));
        assert_eq!(enrichment.wmo_prefix, Some("FT"));
        assert_eq!(
            enrichment.office.as_ref().map(|office| office.code),
            Some("FFC")
        );
        assert_eq!(
            enrichment
                .header
                .as_ref()
                .map(|header| header.afos.as_str()),
            Some("TAFPDK")
        );
        assert!(enrichment.issues.is_empty());
        assert!(enrichment.wmo_header.is_none());
        assert!(enrichment.metar.is_none());
        assert!(enrichment.taf.is_none());
        assert!(enrichment.dcp.is_none());
        let json = serde_json::to_value(&enrichment).expect("enrichment serializes");
        assert!(json.get("flags").is_none());
    }

    #[test]
    fn text_products_do_not_fall_back_to_filename_heuristics() {
        let enrichment = enrich_product("TAFPDKGA.TXT", b"000 \nINVALID HEADER\nTAFPDK\nBody\n");

        assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
        assert_eq!(enrichment.family, Some("nws_text_product"));
        assert_eq!(enrichment.pil, None);
        assert_eq!(enrichment.issues.len(), 1);
        assert_eq!(enrichment.issues[0].code, "invalid_wmo_header");
        assert!(enrichment.wmo_header.is_none());
        assert!(enrichment.metar.is_none());
        assert!(enrichment.taf.is_none());
        assert!(enrichment.dcp.is_none());
        assert!(enrichment.office.is_none());
    }

    #[test]
    fn metar_collectives_use_wmo_fallback_without_afos() {
        let enrichment = enrich_product(
            "SAGL31.TXT",
            b"000 \nSAGL31 BGGH 070200\nMETAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n",
        );

        assert_eq!(enrichment.source, ProductEnrichmentSource::WmoMetarBulletin);
        assert_eq!(enrichment.family, Some("metar_collective"));
        assert_eq!(enrichment.title, Some("METAR bulletin"));
        assert_eq!(enrichment.pil, None);
        assert_eq!(enrichment.wmo_prefix, None);
        assert_eq!(
            enrichment
                .wmo_header
                .as_ref()
                .map(|header| header.ttaaii.as_str()),
            Some("SAGL31")
        );
        assert_eq!(
            enrichment.metar.as_ref().map(MetarBulletin::report_count),
            Some(1)
        );
        assert!(enrichment.office.is_none());
        assert!(enrichment.taf.is_none());
        assert!(enrichment.dcp.is_none());
        assert!(enrichment.issues.is_empty());
    }

    #[test]
    fn taf_bulletins_use_wmo_fallback_without_afos() {
        let enrichment = enrich_product(
            "TAFWBCFJ.TXT",
            b"000 \nFTXX01 KWBC 070200\nTAF AMD\nWBCF 070244Z 0703/0803 18012KT P6SM SCT050\n",
        );

        assert_eq!(enrichment.source, ProductEnrichmentSource::WmoTafBulletin);
        assert_eq!(enrichment.family, Some("taf_bulletin"));
        assert_eq!(enrichment.title, Some("Terminal Aerodrome Forecast"));
        assert_eq!(enrichment.pil, None);
        assert_eq!(
            enrichment
                .wmo_header
                .as_ref()
                .map(|header| header.ttaaii.as_str()),
            Some("FTXX01")
        );
        assert_eq!(
            enrichment.taf.as_ref().map(|taf| taf.station.as_str()),
            Some("WBCF")
        );
        assert_eq!(
            enrichment.taf.as_ref().map(|taf| taf.issue_time.as_str()),
            Some("070244Z")
        );
        assert_eq!(
            enrichment.office.as_ref().map(|office| office.code),
            Some("WBC")
        );
        assert_eq!(
            enrichment
                .taf
                .as_ref()
                .map(|taf| (taf.valid_from.as_deref(), taf.valid_to.as_deref())),
            Some((Some("0703"), Some("0803")))
        );
        assert_eq!(enrichment.taf.as_ref().map(|taf| taf.amendment), Some(true));
        assert!(enrichment.metar.is_none());
        assert!(enrichment.dcp.is_none());
        assert!(enrichment.issues.is_empty());
    }

    #[test]
    fn taf_bulletins_with_marker_line_before_report_use_wmo_fallback() {
        let enrichment = enrich_product(
            "TAFMD1.TXT",
            b"FTVN41 KWBC 070303\nTAF\nTAF SVJC 070400Z 0706/0806 07005KT 9999 FEW013 TX33/0718Z\n      TN23/0708Z\n      TEMPO 0706/0710 08004KT CAVOK\n     FM071100 09006KT 9999 FEW013=\n",
        );

        assert_eq!(enrichment.source, ProductEnrichmentSource::WmoTafBulletin);
        assert_eq!(enrichment.family, Some("taf_bulletin"));
        assert_eq!(
            enrichment
                .wmo_header
                .as_ref()
                .map(|header| header.ttaaii.as_str()),
            Some("FTVN41")
        );
        assert_eq!(
            enrichment.taf.as_ref().map(|taf| taf.station.as_str()),
            Some("SVJC")
        );
        assert_eq!(
            enrichment.taf.as_ref().map(|taf| taf.issue_time.as_str()),
            Some("070400Z")
        );
        assert_eq!(
            enrichment
                .taf
                .as_ref()
                .map(|taf| (taf.valid_from.as_deref(), taf.valid_to.as_deref())),
            Some((Some("0706"), Some("0806")))
        );
        assert!(enrichment.issues.is_empty());
        assert!(enrichment.dcp.is_none());
    }

    #[test]
    fn dcp_bulletins_use_wmo_fallback_without_afos() {
        let enrichment = enrich_product(
            "MISDCPSV.TXT",
            b"SXMS50 KWAL 070258\n83786162 066025814\n16.23\n003\n137\n071\n088\n12.9\n137\n007\n00000\n 42-0NN  45E\n",
        );

        assert_eq!(enrichment.source, ProductEnrichmentSource::WmoDcpBulletin);
        assert_eq!(enrichment.family, Some("dcp_telemetry_bulletin"));
        assert_eq!(enrichment.title, Some("GOES DCP telemetry bulletin"));
        assert_eq!(
            enrichment
                .wmo_header
                .as_ref()
                .map(|header| header.ttaaii.as_str()),
            Some("SXMS50")
        );
        assert_eq!(
            enrichment
                .dcp
                .as_ref()
                .and_then(|bulletin| bulletin.platform_id.as_deref()),
            Some("83786162 066025814")
        );
        assert_eq!(
            enrichment.office.as_ref().map(|office| office.code),
            Some("WAL")
        );
        assert_eq!(
            enrichment.dcp.as_ref().map(|bulletin| bulletin.lines.len()),
            Some(11)
        );
        assert!(enrichment.metar.is_none());
        assert!(enrichment.taf.is_none());
        assert!(enrichment.issues.is_empty());
    }

    #[test]
    fn misa_bulletins_share_wallops_telemetry_fallback() {
        let enrichment = enrich_product(
            "MISA50US.TXT",
            b"SXPA50 KWAL 070309\n\x1eD6805150 066030901 \n05.06 \n008 \n180 \n056 \n098 \n12.8 \n183 \n018 \n00000 \n 39-0NN 141E\n",
        );

        assert_eq!(enrichment.source, ProductEnrichmentSource::WmoDcpBulletin);
        assert_eq!(enrichment.family, Some("dcp_telemetry_bulletin"));
        assert_eq!(
            enrichment
                .dcp
                .as_ref()
                .and_then(|bulletin| bulletin.platform_id.as_deref()),
            Some("D6805150 066030901")
        );
        assert_eq!(
            enrichment.office.as_ref().map(|office| office.code),
            Some("WAL")
        );
        assert!(enrichment.issues.is_empty());
    }

    #[test]
    fn misdcp_inline_telemetry_bulletins_share_wallops_telemetry_fallback() {
        let enrichment = enrich_product(
            "MISDCPNI.TXT",
            b"SXMN20 KWAL 070326\n2211F77E 066032650bB1F@VT@VT@VT@VT@VT@VT@VT@VT@VT@VT@VT@VT@Fx@Fx@Fx@Fx@Fx@Fx@Fx@Fx@Fx@Fx@Fx@Fx@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@Ta@TaJ 40+0NN  57E%\n",
        );

        assert_eq!(enrichment.source, ProductEnrichmentSource::WmoDcpBulletin);
        assert_eq!(enrichment.family, Some("dcp_telemetry_bulletin"));
        assert_eq!(
            enrichment
                .dcp
                .as_ref()
                .and_then(|bulletin| bulletin.platform_id.as_deref()),
            Some("2211F77E 066032650")
        );
        assert_eq!(
            enrichment.office.as_ref().map(|office| office.code),
            Some("WAL")
        );
        assert_eq!(
            enrichment.dcp.as_ref().map(|bulletin| bulletin.lines.len()),
            Some(1)
        );
        assert!(enrichment.issues.is_empty());
    }

    #[test]
    fn international_sigmet_bulletins_use_wmo_fallback_without_afos() {
        let enrichment = enrich_product(
            "WVID21.TXT",
            b"WVID21 WAAA 090100\nWAAF SIGMET 05 VALID 090100/090700 WAAA-\nWAAF UJUNG PANDANG  FIR VA ERUPTION MT IBU PSN N0129 E12738 VA CLD\nOBS AT 0040Z WI N0129 E12737 - N0131 E12738 - N0129 E12751 - N0117\nE12744 - N0129 E12737 SFC/FL070 MOV SE 10KT NC=\n",
        );

        assert_eq!(
            enrichment.source,
            ProductEnrichmentSource::WmoSigmetBulletin
        );
        assert_eq!(enrichment.family, Some("sigmet_bulletin"));
        assert_eq!(
            enrichment.sigmet.as_ref().map(|value| value.sections.len()),
            Some(1)
        );
        assert!(enrichment.issues.is_empty());
    }

    #[test]
    fn corrected_metar_bulletins_use_wmo_fallback_without_afos() {
        let enrichment = enrich_product(
            "SAGG31.TXT",
            b"SAGG31 UGTB 090030 CCA\nMETAR COR UGKO 090030Z 24007KT 9999 SCT030 BKN061 03/01 Q1029 NOSIG=\n",
        );

        assert_eq!(enrichment.source, ProductEnrichmentSource::WmoMetarBulletin);
        assert_eq!(enrichment.family, Some("metar_collective"));
        assert_eq!(
            enrichment.metar.as_ref().map(MetarBulletin::report_count),
            Some(1)
        );
        assert!(enrichment.issues.is_empty());
    }

    #[test]
    fn duplicated_amended_taf_bulletins_use_wmo_fallback_without_afos() {
        let enrichment = enrich_product(
            "FTMX41.TXT",
            b"FTMX41 KWBC 090103 AAA\nTAF AMD\nTAF AMD MMAS 090101Z 0901/0918 23008KT P6SM SCT100 BKN200\n     FM091200 04005KT P6SM SCT200=\n",
        );

        assert_eq!(enrichment.source, ProductEnrichmentSource::WmoTafBulletin);
        assert_eq!(enrichment.family, Some("taf_bulletin"));
        assert_eq!(
            enrichment.taf.as_ref().map(|value| value.station.as_str()),
            Some("MMAS")
        );
        assert!(enrichment.issues.is_empty());
    }

    #[test]
    fn marker_line_then_corrected_taf_bulletins_use_wmo_fallback_without_afos() {
        let enrichment = enrich_product(
            "TAFMDCOR.TXT",
            b"FTXX60 KWBC 110130\nTAF\nTAF COR KSVN 110127Z 1101/1207 17006KT 9999 SKC QNH3008INS\n      BECMG 1117/1118 22009KT 9999 BKN060 QNH3004INS TX29/1117Z\n      TN17/1110Z=\n",
        );

        assert_eq!(enrichment.source, ProductEnrichmentSource::WmoTafBulletin);
        assert_eq!(enrichment.family, Some("taf_bulletin"));
        assert_eq!(
            enrichment.taf.as_ref().map(|value| value.station.as_str()),
            Some("KSVN")
        );
        assert_eq!(
            enrichment.taf.as_ref().map(|value| value.correction),
            Some(true)
        );
        assert!(enrichment.issues.is_empty());
    }

    #[test]
    fn wallops_telemetry_variants_with_symbol_noise_use_wmo_dcp_fallback() {
        for (filename, bytes, platform_id) in [
            (
                "MISA50US.TXT",
                b"SXPA50 KWAL 090055\nCE1107B6 068005524`BCT@Go@Gq@Gq@Gr@Gr@Gr@Gs@Gr@Gs@Gr@Gu@Gt~]w~\\T~^F~bF~d@~eS~gq~jl~l]~mo~sA~wyf 39+0NN  25E\n".as_slice(),
                "CE1107B6 068005524",
            ),
            (
                "MISDCPHN.TXT",
                b"SXHN40 KWAL 090038\n50423782 068003840bB1H_??_??_??_??_??_??_??_??@@@@@r@TaJ 47+0NN 175E\n".as_slice(),
                "50423782 068003840",
            ),
            (
                "MISDCPMG.TXT",
                b"SXMG40 KWAL 090050\n9650D70A 068005040\"A18.34B17.92C18.73D82.73E80.63F84.66G9.70H0.00I10.92J355.59K0.00L824.64M824.67N824.67O11.50P21.30Q0.11R-10.01S2360.16T0.00U1.20 38-0NN 397E\n".as_slice(),
                "9650D70A 068005040",
            ),
            (
                "MISDCPSV.TXT",
                b"SXMS50 KWAL 090100\n3B0190E2 068010020`@aW@ac@]C@aP@\\z@N\\B_G@Dn@]A@A_@FZ@\\~@@@@@@@TiFtd@aY@ae@\\g@aV@\\n@N_B_G@C{@\\h@AQ@Ek@\\i@@@@@@@TmFtd@a[@ai@\\Z@aW@\\\\@N\\B_F@DX@]W@AD@Ez@\\_@@@@@@@TsFtd@a\\@aj@\\L@aW@\\O@NYB_E@C^@]C@AO@Dz@\\U@@@@@B@TxFtd 38+0NN 145E\n".as_slice(),
                "3B0190E2 068010020",
            ),
        ] {
            let enrichment = enrich_product(filename, bytes);
            assert_eq!(enrichment.source, ProductEnrichmentSource::WmoDcpBulletin);
            assert_eq!(enrichment.family, Some("dcp_telemetry_bulletin"));
            assert_eq!(
                enrichment
                    .dcp
                    .as_ref()
                    .and_then(|bulletin| bulletin.platform_id.as_deref()),
                Some(platform_id)
            );
            assert!(enrichment.issues.is_empty());
        }
    }

    #[test]
    fn unsupported_airmet_bulletins_use_wmo_unsupported_source() {
        let enrichment = enrich_product(
            "WAAB31.TXT",
            b"WAAB31 LATI 090038\nLAAA AIRMET 1 VALID 090100/090500 LATI-\nLAAA TIRANA FIR MOD ICE FCST S OF N4110 FL070/120 STNR NC=\n",
        );

        assert_eq!(
            enrichment.source,
            ProductEnrichmentSource::WmoUnsupportedBulletin
        );
        assert_eq!(enrichment.family, Some("unsupported_wmo_bulletin"));
        assert_eq!(enrichment.issues[0].code, "unsupported_airmet_bulletin");
        assert!(enrichment.wmo_header.is_some());
    }

    #[test]
    fn unsupported_canadian_bulletins_use_wmo_unsupported_source() {
        let enrichment = enrich_product(
            "FPCN11.TXT",
            b"FPCN11 CWWG 090059 AAD\nUPDATED FORECASTS FOR SOUTHERN MANITOBA ISSUED BY ENVIRONMENT CANADA\nAT 7:57 P.M. CDT SUNDAY 8 MARCH 2026 FOR TONIGHT MONDAY AND MONDAY\nNIGHT.\n",
        );

        assert_eq!(
            enrichment.source,
            ProductEnrichmentSource::WmoUnsupportedBulletin
        );
        assert_eq!(enrichment.family, Some("unsupported_wmo_bulletin"));
        assert_eq!(
            enrichment.issues[0].code,
            "unsupported_canadian_text_bulletin"
        );
        assert!(enrichment.wmo_header.is_some());
    }

    #[test]
    fn unsupported_surface_observation_bulletins_use_wmo_unsupported_source() {
        let enrichment = enrich_product(
            "SAHOURLY.TXT",
            b"SACN74 CWAO 090000 RRC\n\nNPL SA 0000 AUTO8 M M M 990/-36/-39/2703/M/     7003 61MM=\n",
        );

        assert_eq!(
            enrichment.source,
            ProductEnrichmentSource::WmoUnsupportedBulletin
        );
        assert_eq!(enrichment.family, Some("unsupported_wmo_bulletin"));
        assert_eq!(
            enrichment.issues[0].code,
            "unsupported_surface_observation_bulletin"
        );
        assert!(enrichment.wmo_header.is_some());
    }

    #[test]
    fn body_enrichment_uses_body_text_not_afos_line() {
        let enrichment = enrich_product(
            "RFDLWXVA.TXT",
            b"FNUS41 KLWX 070303\nRFDLWX\nVAZ507-508-071100-\n\nRangeland Fire Danger Forecast\nNational Weather Service Baltimore MD/Washington DC\n1003 PM EST Fri Mar 6 2026\n",
        );

        assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
        assert_eq!(enrichment.pil.as_deref(), Some("RFD"));
        assert!(enrichment.issues.is_empty());
        assert_eq!(
            enrichment
                .body
                .as_ref()
                .and_then(|body| body.ugc.as_ref())
                .map(|sections| sections[0].zones["VA"]
                    .iter()
                    .map(|area| area.id)
                    .collect::<Vec<_>>()),
            Some(vec![507, 508])
        );
    }

    #[test]
    fn current_specialized_afos_products_remain_bodyless() {
        let fd = enrich_product(
            "FD1US1.TXT",
            b"000 \nFTUS80 KWBC 070000\nFD1US1\nDATA BASED ON 070000Z\nVALID 071200Z\nFT 3000 6000\nBOS 9900 2812\n",
        );
        assert!(fd.fd.is_some());
        assert!(fd.body.is_none());

        let pirep = enrich_product(
            "PIRXXX.TXT",
            b"000 \nUAUS01 KBOU 070000\nPIRBOU\nDEN UA /OV 35 SW /TM 1925 /FL050 /TP E145=\n",
        );
        assert!(pirep.pirep.is_some());
        assert!(pirep.body.is_none());

        let sigmet = enrich_product(
            "SIGABC.TXT",
            b"000 \nWSUS31 KKCI 070000\nSIGABC\nCONVECTIVE SIGMET 12C\nVALID UNTIL 2355Z\nIA MO\nFROM 20S DSM-30NW IRK\nAREA EMBD TS MOV FROM 24020KT.\n",
        );
        assert!(sigmet.sigmet.is_some());
        assert!(sigmet.body.is_none());
    }

    #[test]
    fn non_text_products_use_filename_classification() {
        let enrichment = enrich_product("RADUMSVY.GIF", b"ignored");

        assert_eq!(enrichment.source, ProductEnrichmentSource::FilenameNonText);
        assert_eq!(enrichment.family, Some("radar_graphic"));
        assert_eq!(enrichment.title, Some("Radar graphic"));
        assert!(enrichment.office.is_none());
        assert!(enrichment.header.is_none());
        assert!(enrichment.wmo_header.is_none());
        assert!(enrichment.metar.is_none());
        assert!(enrichment.taf.is_none());
        assert!(enrichment.dcp.is_none());
    }

    #[test]
    fn unknown_non_text_products_are_marked_unknown() {
        let enrichment = enrich_product("mystery.bin", b"ignored");

        assert_eq!(enrichment.source, ProductEnrichmentSource::Unknown);
        assert_eq!(enrichment.container, "raw");
        assert!(enrichment.family.is_none());
        assert!(enrichment.office.is_none());
        assert!(enrichment.wmo_header.is_none());
        assert!(enrichment.metar.is_none());
        assert!(enrichment.taf.is_none());
        assert!(enrichment.dcp.is_none());
        let json = serde_json::to_value(&enrichment).expect("enrichment serializes");
        assert!(json.get("flags").is_none());
    }

    #[test]
    fn zip_framed_txt_payload_is_treated_as_unknown_zip() {
        let enrichment = enrich_product("TAFALLUS.TXT", b"PK\x03\x04compressed bytes");

        assert_eq!(enrichment.source, ProductEnrichmentSource::Unknown);
        assert_eq!(enrichment.container, "zip");
        assert!(enrichment.family.is_none());
        assert!(enrichment.office.is_none());
        assert!(enrichment.header.is_none());
        assert!(enrichment.wmo_header.is_none());
        assert!(enrichment.metar.is_none());
        assert!(enrichment.taf.is_none());
        assert!(enrichment.dcp.is_none());
        assert!(enrichment.issues.is_empty());
    }
}
