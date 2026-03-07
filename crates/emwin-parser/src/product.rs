use crate::data::{classify_non_text_product, container_from_filename, wmo_office_entry};
use crate::dcp::{DcpBulletin, parse_dcp_bulletin};
use crate::header::{parse_text_product_conditioned, parse_wmo_bulletin_conditioned};
use crate::metar::{MetarBulletin, parse_metar_bulletin};
use crate::taf::{TafBulletin, parse_taf_bulletin};
use crate::{
    BbbKind, ParserError, ProductBody, ProductMetadataFlags, ProductParseIssue, TextProductHeader,
    WmoHeader, WmoOfficeEntry, enrich_body, enrich_header, wmo_prefix_for_pil,
};
use chrono::Utc;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductEnrichmentSource {
    TextHeader,
    WmoMetarBulletin,
    WmoTafBulletin,
    WmoDcpBulletin,
    FilenameNonText,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProductEnrichment {
    pub source: ProductEnrichmentSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<&'static str>,
    pub container: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pil: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wmo_prefix: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<ProductMetadataFlags>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub office: Option<WmoOfficeEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<TextProductHeader>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wmo_header: Option<WmoHeader>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbb_kind: Option<BbbKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<ProductBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metar: Option<MetarBulletin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taf: Option<TafBulletin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dcp: Option<DcpBulletin>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub issues: Vec<ProductParseIssue>,
}

pub fn enrich_product(filename: &str, bytes: &[u8]) -> ProductEnrichment {
    if detected_container(filename, bytes) == "zip" && is_text_product(filename) {
        return unknown_product(filename, bytes);
    }

    if is_text_product(filename) {
        return enrich_text_product(filename, bytes);
    }

    if let Some(meta) = classify_non_text_product(filename) {
        return ProductEnrichment {
            source: ProductEnrichmentSource::FilenameNonText,
            family: Some(meta.family),
            title: Some(meta.title),
            container: meta.container,
            pil: meta.pil.map(str::to_string),
            wmo_prefix: meta.wmo_prefix,
            flags: None,
            office: None,
            header: None,
            wmo_header: None,
            bbb_kind: None,
            body: None,
            metar: None,
            taf: None,
            dcp: None,
            issues: Vec::new(),
        };
    }

    unknown_product(filename, bytes)
}

fn enrich_text_product(filename: &str, bytes: &[u8]) -> ProductEnrichment {
    match parse_text_product_conditioned(bytes) {
        Ok(parsed) => {
            let header = parsed.header;
            let header_enrichment = enrich_header(&header);
            let pil = header_enrichment.pil_nnn.map(str::to_string);
            let title = header_enrichment.pil_description;
            let flags = header_enrichment.flags;
            let bbb_kind = header_enrichment.bbb_kind;

            let (body, issues) = if let Some(ref flags) = flags {
                let reference_time = header.timestamp(Utc::now());
                enrich_body(&parsed.body_text, flags, reference_time)
            } else {
                (None, Vec::new())
            };

            ProductEnrichment {
                source: ProductEnrichmentSource::TextHeader,
                family: Some("nws_text_product"),
                title,
                container: container_from_filename(filename),
                pil: pil.clone(),
                wmo_prefix: pil.as_deref().and_then(wmo_prefix_for_pil),
                flags,
                office: wmo_office_entry(&header.cccc).copied(),
                header: Some(header),
                wmo_header: None,
                bbb_kind,
                body,
                metar: None,
                taf: None,
                dcp: None,
                issues,
            }
        }
        Err(error) => enrich_text_product_fallback(filename, bytes, error),
    }
}

fn enrich_text_product_fallback(
    filename: &str,
    bytes: &[u8],
    error: ParserError,
) -> ProductEnrichment {
    if let (ParserError::MissingAfosLine | ParserError::MissingAfos { .. }, Ok(parsed_wmo)) =
        (&error, parse_wmo_bulletin_conditioned(bytes))
        && let Some((metar, issues)) = parse_metar_bulletin(&parsed_wmo.body_text)
    {
        return ProductEnrichment {
            source: ProductEnrichmentSource::WmoMetarBulletin,
            family: Some("metar_collective"),
            title: Some("METAR bulletin"),
            container: container_from_filename(filename),
            pil: None,
            wmo_prefix: None,
            flags: None,
            office: wmo_office_entry(&parsed_wmo.header.cccc).copied(),
            header: None,
            wmo_header: Some(parsed_wmo.header),
            bbb_kind: None,
            body: None,
            metar: Some(metar),
            taf: None,
            dcp: None,
            issues,
        };
    }

    if let (ParserError::MissingAfosLine | ParserError::MissingAfos { .. }, Ok(parsed_wmo)) =
        (&error, parse_wmo_bulletin_conditioned(bytes))
        && let Some(taf) = parse_taf_bulletin(&parsed_wmo.body_text)
    {
        return ProductEnrichment {
            source: ProductEnrichmentSource::WmoTafBulletin,
            family: Some("taf_bulletin"),
            title: Some("Terminal Aerodrome Forecast"),
            container: container_from_filename(filename),
            pil: None,
            wmo_prefix: None,
            flags: None,
            office: wmo_office_entry(&parsed_wmo.header.cccc).copied(),
            header: None,
            wmo_header: Some(parsed_wmo.header),
            bbb_kind: None,
            body: None,
            metar: None,
            taf: Some(taf),
            dcp: None,
            issues: Vec::new(),
        };
    }

    if let (ParserError::MissingAfosLine | ParserError::MissingAfos { .. }, Ok(parsed_wmo)) =
        (&error, parse_wmo_bulletin_conditioned(bytes))
        && let Some(dcp) = parse_dcp_bulletin(filename, &parsed_wmo.header, &parsed_wmo.body_text)
    {
        return ProductEnrichment {
            source: ProductEnrichmentSource::WmoDcpBulletin,
            family: Some("dcp_telemetry_bulletin"),
            title: Some("GOES DCP telemetry bulletin"),
            container: container_from_filename(filename),
            pil: None,
            wmo_prefix: None,
            flags: None,
            office: wmo_office_entry(&parsed_wmo.header.cccc).copied(),
            header: None,
            wmo_header: Some(parsed_wmo.header),
            bbb_kind: None,
            body: None,
            metar: None,
            taf: None,
            dcp: Some(dcp),
            issues: Vec::new(),
        };
    }

    ProductEnrichment {
        source: ProductEnrichmentSource::TextHeader,
        family: Some("nws_text_product"),
        title: None,
        container: container_from_filename(filename),
        pil: None,
        wmo_prefix: None,
        flags: None,
        office: None,
        header: None,
        wmo_header: None,
        bbb_kind: None,
        body: None,
        metar: None,
        taf: None,
        dcp: None,
        issues: vec![ProductParseIssue::new(
            "text_product_parse",
            parser_error_code(&error),
            error.to_string(),
            parser_error_line(&error).map(str::to_string),
        )],
    }
}

fn is_text_product(filename: &str) -> bool {
    let upper = filename.to_ascii_uppercase();
    upper.ends_with(".TXT") || upper.ends_with(".WMO")
}

fn unknown_product(filename: &str, bytes: &[u8]) -> ProductEnrichment {
    ProductEnrichment {
        source: ProductEnrichmentSource::Unknown,
        family: None,
        title: None,
        container: detected_container(filename, bytes),
        pil: None,
        wmo_prefix: None,
        flags: None,
        office: None,
        header: None,
        wmo_header: None,
        bbb_kind: None,
        body: None,
        metar: None,
        taf: None,
        dcp: None,
        issues: Vec::new(),
    }
}

fn detected_container(filename: &str, bytes: &[u8]) -> &'static str {
    if is_zip_payload(bytes) {
        "zip"
    } else {
        container_from_filename(filename)
    }
}

fn is_zip_payload(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

fn parser_error_code(error: &ParserError) -> &'static str {
    match error {
        ParserError::EmptyInput => "empty_input",
        ParserError::MissingWmoLine => "missing_wmo_line",
        ParserError::InvalidWmoHeader { .. } => "invalid_wmo_header",
        ParserError::MissingAfosLine => "missing_afos_line",
        ParserError::MissingAfos { .. } => "missing_afos",
    }
}

fn parser_error_line(error: &ParserError) -> Option<&str> {
    match error {
        ParserError::InvalidWmoHeader { line } | ParserError::MissingAfos { line } => Some(line),
        _ => None,
    }
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
        assert_eq!(enrichment.flags.map(|flags| flags.ugc), Some(false));
        assert_eq!(enrichment.flags.map(|flags| flags.vtec), Some(false));
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
    }

    #[test]
    fn text_products_do_not_fall_back_to_filename_heuristics() {
        let enrichment = enrich_product("TAFPDKGA.TXT", b"000 \nINVALID HEADER\nTAFPDK\nBody\n");

        assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
        assert_eq!(enrichment.family, Some("nws_text_product"));
        assert_eq!(enrichment.pil, None);
        assert_eq!(enrichment.flags, None);
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
    fn body_enrichment_uses_body_text_not_afos_line() {
        let enrichment = enrich_product(
            "RECLWXVA.TXT",
            b"SXUS41 KLWX 070303\nRECLWX\nVAZ507-508-071100-\n\nForecast for Shenandoah National Park Above 2000 Feet\nNational Weather Service Baltimore MD/Washington DC\n1003 PM EST Fri Mar 6 2026\n",
        );

        assert_eq!(enrichment.source, ProductEnrichmentSource::TextHeader);
        assert_eq!(enrichment.pil.as_deref(), Some("REC"));
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
    fn non_text_products_use_filename_classification() {
        let enrichment = enrich_product("RADUMSVY.GIF", b"ignored");

        assert_eq!(enrichment.source, ProductEnrichmentSource::FilenameNonText);
        assert_eq!(enrichment.family, Some("radar_graphic"));
        assert_eq!(enrichment.title, Some("Radar graphic"));
        assert_eq!(enrichment.flags, None);
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
        assert_eq!(enrichment.flags, None);
        assert!(enrichment.family.is_none());
        assert!(enrichment.office.is_none());
        assert!(enrichment.wmo_header.is_none());
        assert!(enrichment.metar.is_none());
        assert!(enrichment.taf.is_none());
        assert!(enrichment.dcp.is_none());
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
