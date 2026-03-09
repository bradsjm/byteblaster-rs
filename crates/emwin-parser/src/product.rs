//! Product enrichment and classification.
//!
//! This module provides product type detection and enrichment for both text and non-text
//! products received from EMWIN. It parses product headers, extracts metadata, and classifies
//! products into families (text products, METAR bulletins, TAF bulletins, radar graphics, etc.).
//!
//! ## Product Types
//!
//! - **Text products**: WMO/AFOS formatted text with headers and PILs
//! - **METAR bulletins**: Collective METAR reports in WMO format
//! - **TAF bulletins**: Terminal aerodrome forecasts
//! - **DCP bulletins**: GOES DCP telemetry data
//! - **FD bulletins**: Winds and temperatures aloft
//! - **PIREP bulletins**: Pilot reports
//! - **SIGMET bulletins**: Significant meteorological information
//! - **Graphics**: Radar images, satellite imagery, etc.
//!
//! ## Enrichment Process
//!
//! 1. Detect container type (ZIP or raw)
//! 2. Parse WMO headers and AFOS PILs for text products
//! 3. Fall back to filename heuristics for non-text products
//! 4. Extract body elements (VTEC, UGC, polygons, etc.) based on product type
//! 5. Build [`ProductEnrichment`] with all discovered metadata

use crate::data::{classify_non_text_product, container_from_filename, wmo_office_entry};
use crate::dcp::{DcpBulletin, parse_dcp_bulletin};
use crate::fd::{FdBulletin, parse_fd_bulletin};
use crate::header::{parse_text_product_conditioned, parse_wmo_bulletin_conditioned};
use crate::metar::{MetarBulletin, parse_metar_bulletin};
use crate::pirep::{PirepBulletin, parse_pirep_bulletin};
use crate::sigmet::{SigmetBulletin, parse_sigmet_bulletin};
use crate::taf::{TafBulletin, parse_taf_bulletin};
use crate::{
    BbbKind, ParserError, ProductBody, ProductParseIssue, TextProductHeader, WmoHeader,
    WmoOfficeEntry, enrich_body, enrich_header, wmo_prefix_for_pil,
};
use chrono::Utc;
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

/// Enriches a product by parsing its headers and classifying its type.
///
/// This is the main entry point for product enrichment. It detects the product type,
/// parses appropriate headers, extracts metadata, and returns a comprehensive
/// `ProductEnrichment` struct.
///
/// # Arguments
///
/// * `filename` - Original filename of the product
/// * `bytes` - Raw product content as bytes
///
/// # Returns
///
/// A `ProductEnrichment` containing all parsed metadata and any issues encountered
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
            office: None,
            header: None,
            wmo_header: None,
            bbb_kind: None,
            body: None,
            metar: None,
            taf: None,
            dcp: None,
            fd: None,
            pirep: None,
            sigmet: None,
            issues: Vec::new(),
        };
    }

    unknown_product(filename, bytes)
}

/// Enriches a text product by parsing its headers and body.
///
/// Attempts to parse text products with AFOS headers. Falls back to WMO-only
/// parsing if AFOS line is missing but WMO headers are present.
///
/// # Arguments
///
/// * `filename` - Original filename
/// * `bytes` - Raw product content
///
/// # Returns
///
/// A `ProductEnrichment` with parsed headers, body elements, and any issues
fn enrich_text_product(filename: &str, bytes: &[u8]) -> ProductEnrichment {
    match parse_text_product_conditioned(bytes) {
        Ok(parsed) => {
            let header = parsed.header;
            let header_enrichment = enrich_header(&header);
            let pil = header_enrichment.pil_nnn.map(str::to_string);
            let title = header_enrichment.pil_description;
            let flags = header_enrichment.flags;
            let bbb_kind = header_enrichment.bbb_kind;
            let reference_time = header.timestamp(Utc::now());

            if let Some(fd) = reference_time.and_then(|reference_time| {
                looks_like_fd_text_product(&header.afos, &parsed.body_text)
                    .then(|| {
                        parse_fd_bulletin(
                            &parsed.body_text,
                            Some(header.afos.as_str()),
                            reference_time,
                        )
                    })
                    .flatten()
            }) {
                return ProductEnrichment {
                    source: ProductEnrichmentSource::TextHeader,
                    family: Some("fd_bulletin"),
                    title: Some("Winds and temperatures aloft"),
                    container: container_from_filename(filename),
                    pil: pil.clone(),
                    wmo_prefix: pil.as_deref().and_then(wmo_prefix_for_pil),
                    office: wmo_office_entry(&header.cccc).copied(),
                    header: Some(header),
                    wmo_header: None,
                    bbb_kind,
                    body: None,
                    metar: None,
                    taf: None,
                    dcp: None,
                    fd: Some(fd),
                    pirep: None,
                    sigmet: None,
                    issues: Vec::new(),
                };
            }

            if let Some(pirep) = looks_like_pirep_text_product(&header.afos, &parsed.body_text)
                .then(|| parse_pirep_bulletin(&parsed.body_text))
                .flatten()
            {
                return ProductEnrichment {
                    source: ProductEnrichmentSource::TextPirepBulletin,
                    family: Some("pirep_bulletin"),
                    title: Some("Pilot report bulletin"),
                    container: container_from_filename(filename),
                    pil: pil.clone(),
                    wmo_prefix: pil.as_deref().and_then(wmo_prefix_for_pil),
                    office: wmo_office_entry(&header.cccc).copied(),
                    header: Some(header),
                    wmo_header: None,
                    bbb_kind,
                    body: None,
                    metar: None,
                    taf: None,
                    dcp: None,
                    fd: None,
                    pirep: Some(pirep),
                    sigmet: None,
                    issues: Vec::new(),
                };
            }

            if let Some(sigmet) = looks_like_sigmet_text_product(&header.afos, &parsed.body_text)
                .then(|| parse_sigmet_bulletin(&parsed.body_text))
                .flatten()
            {
                return ProductEnrichment {
                    source: ProductEnrichmentSource::TextSigmetBulletin,
                    family: Some("sigmet_bulletin"),
                    title: Some("SIGMET bulletin"),
                    container: container_from_filename(filename),
                    pil: pil.clone(),
                    wmo_prefix: pil.as_deref().and_then(wmo_prefix_for_pil),
                    office: wmo_office_entry(&header.cccc).copied(),
                    header: Some(header),
                    wmo_header: None,
                    bbb_kind,
                    body: None,
                    metar: None,
                    taf: None,
                    dcp: None,
                    fd: None,
                    pirep: None,
                    sigmet: Some(sigmet),
                    issues: Vec::new(),
                };
            }

            let (body, issues) = if let Some(ref flags) = flags {
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
                office: wmo_office_entry(&header.cccc).copied(),
                header: Some(header),
                wmo_header: None,
                bbb_kind,
                body,
                metar: None,
                taf: None,
                dcp: None,
                fd: None,
                pirep: None,
                sigmet: None,
                issues,
            }
        }
        Err(error) => enrich_text_product_fallback(filename, bytes, error),
    }
}

/// Fallback enrichment for text products that failed initial parsing.
///
/// Attempts to parse as WMO bulletin without AFOS line. Falls back to METAR,
/// TAF, DCP, or FD bulletin parsing if appropriate patterns are detected.
///
/// # Arguments
///
/// * `filename` - Original filename
/// * `bytes` - Raw product content
/// * `error` - The parse error from the initial text product parsing attempt
///
/// # Returns
///
/// A `ProductEnrichment` derived from WMO header or filename classification
fn enrich_text_product_fallback(
    filename: &str,
    bytes: &[u8],
    error: ParserError,
) -> ProductEnrichment {
    let afos_missing = matches!(
        error,
        ParserError::MissingAfosLine | ParserError::MissingAfos { .. }
    );

    if afos_missing && let Ok(parsed_wmo) = parse_wmo_bulletin_conditioned(bytes) {
        return enrich_wmo_only_bulletin(filename, parsed_wmo.header, &parsed_wmo.body_text);
    }

    ProductEnrichment {
        source: ProductEnrichmentSource::TextHeader,
        family: Some("nws_text_product"),
        title: None,
        container: container_from_filename(filename),
        pil: None,
        wmo_prefix: None,
        office: None,
        header: None,
        wmo_header: None,
        bbb_kind: None,
        body: None,
        metar: None,
        taf: None,
        dcp: None,
        fd: None,
        pirep: None,
        sigmet: None,
        issues: vec![ProductParseIssue::new(
            "text_product_parse",
            parser_error_code(&error),
            error.to_string(),
            parser_error_line(&error).map(str::to_string),
        )],
    }
}

/// Checks if a filename indicates a text product.
///
/// Text products have `.TXT` or `.WMO` extensions (case-insensitive).
fn is_text_product(filename: &str) -> bool {
    let upper = filename.to_ascii_uppercase();
    upper.ends_with(".TXT") || upper.ends_with(".WMO")
}

/// Extracts the filename stem (without extension and path).
///
/// # Examples
///
/// * `"path/to/file.TXT"` -> `"file"`
/// * `"data.gz"` -> `"data"`
fn filename_stem(filename: &str) -> &str {
    filename
        .rsplit_once('/')
        .map(|(_, tail)| tail)
        .unwrap_or(filename)
        .split_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(filename)
}

/// Detects if a text product appears to be an FD (winds/temps) bulletin.
///
/// Checks AFOS code pattern (FD0, FD1, etc.) or body content markers.
fn looks_like_fd_text_product(afos: &str, body_text: &str) -> bool {
    matches!(
        afos.get(..3),
        Some("FD0" | "FD1" | "FD2" | "FD3" | "FD8" | "FD9" | "FDI")
    ) || body_text.contains("DATA BASED ON ")
        && body_text.contains("VALID ")
        && body_text
            .lines()
            .any(|line| line.trim_start().starts_with("FT "))
}

/// Detects if a WMO bulletin appears to be an FD (winds/temps) bulletin.
///
/// Checks filename stem starts with "FD" and body contains "DATA BASED ON" and "FT".
fn looks_like_fd_wmo_bulletin(filename: &str, body_text: &str) -> bool {
    filename_stem(filename).starts_with("FD")
        && body_text.contains("DATA BASED ON ")
        && body_text.contains("VALID ")
        && body_text
            .lines()
            .any(|line| line.trim_start().starts_with("FT "))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WmoOnlyKind {
    Fd,
    Metar,
    Taf,
    Dcp,
    Sigmet,
    Airmet,
    CanadianText,
    SurfaceObservation,
    UnknownValidWmo,
}

fn enrich_wmo_only_bulletin(
    filename: &str,
    header: WmoHeader,
    body_text: &str,
) -> ProductEnrichment {
    match classify_wmo_only_bulletin(filename, &header, body_text) {
        WmoOnlyKind::Fd => {
            let fd = header
                .timestamp(Utc::now())
                .and_then(|reference_time| {
                    parse_fd_bulletin(body_text, Some(filename_stem(filename)), reference_time)
                })
                .expect("wmo fd classification must yield parsable bulletin");

            ProductEnrichment {
                source: ProductEnrichmentSource::WmoFdBulletin,
                family: Some("fd_bulletin"),
                title: Some("Winds and temperatures aloft"),
                container: container_from_filename(filename),
                pil: None,
                wmo_prefix: None,
                office: wmo_office_entry(&header.cccc).copied(),
                header: None,
                wmo_header: Some(header),
                bbb_kind: None,
                body: None,
                metar: None,
                taf: None,
                dcp: None,
                fd: Some(fd),
                pirep: None,
                sigmet: None,
                issues: Vec::new(),
            }
        }
        WmoOnlyKind::Metar => {
            let (metar, issues) = parse_metar_bulletin(body_text)
                .expect("wmo metar classification must yield parsable bulletin");

            ProductEnrichment {
                source: ProductEnrichmentSource::WmoMetarBulletin,
                family: Some("metar_collective"),
                title: Some("METAR bulletin"),
                container: container_from_filename(filename),
                pil: None,
                wmo_prefix: None,
                office: wmo_office_entry(&header.cccc).copied(),
                header: None,
                wmo_header: Some(header),
                bbb_kind: None,
                body: None,
                metar: Some(metar),
                taf: None,
                dcp: None,
                fd: None,
                pirep: None,
                sigmet: None,
                issues,
            }
        }
        WmoOnlyKind::Taf => {
            let taf = parse_taf_bulletin(body_text)
                .expect("wmo taf classification must yield parsable bulletin");

            ProductEnrichment {
                source: ProductEnrichmentSource::WmoTafBulletin,
                family: Some("taf_bulletin"),
                title: Some("Terminal Aerodrome Forecast"),
                container: container_from_filename(filename),
                pil: None,
                wmo_prefix: None,
                office: wmo_office_entry(&header.cccc).copied(),
                header: None,
                wmo_header: Some(header),
                bbb_kind: None,
                body: None,
                metar: None,
                taf: Some(taf),
                dcp: None,
                fd: None,
                pirep: None,
                sigmet: None,
                issues: Vec::new(),
            }
        }
        WmoOnlyKind::Dcp => {
            let dcp = parse_dcp_bulletin(filename, &header, body_text)
                .expect("wmo dcp classification must yield parsable bulletin");

            ProductEnrichment {
                source: ProductEnrichmentSource::WmoDcpBulletin,
                family: Some("dcp_telemetry_bulletin"),
                title: Some("GOES DCP telemetry bulletin"),
                container: container_from_filename(filename),
                pil: None,
                wmo_prefix: None,
                office: wmo_office_entry(&header.cccc).copied(),
                header: None,
                wmo_header: Some(header),
                bbb_kind: None,
                body: None,
                metar: None,
                taf: None,
                dcp: Some(dcp),
                fd: None,
                pirep: None,
                sigmet: None,
                issues: Vec::new(),
            }
        }
        WmoOnlyKind::Sigmet => {
            let sigmet = parse_sigmet_bulletin(body_text)
                .expect("wmo sigmet classification must yield parsable bulletin");

            ProductEnrichment {
                source: ProductEnrichmentSource::WmoSigmetBulletin,
                family: Some("sigmet_bulletin"),
                title: Some("SIGMET bulletin"),
                container: container_from_filename(filename),
                pil: None,
                wmo_prefix: None,
                office: wmo_office_entry(&header.cccc).copied(),
                header: None,
                wmo_header: Some(header),
                bbb_kind: None,
                body: None,
                metar: None,
                taf: None,
                dcp: None,
                fd: None,
                pirep: None,
                sigmet: Some(sigmet),
                issues: Vec::new(),
            }
        }
        WmoOnlyKind::Airmet => unsupported_wmo_bulletin(
            filename,
            header,
            "unsupported_airmet_bulletin",
            "recognized valid WMO AIRMET bulletin, but textual AIRMET parsing is not implemented",
            first_nonempty_line(body_text).map(str::to_string),
        ),
        WmoOnlyKind::CanadianText => unsupported_wmo_bulletin(
            filename,
            header,
            "unsupported_canadian_text_bulletin",
            "recognized valid WMO Canadian text bulletin, but parsing is not implemented",
            first_nonempty_line(body_text).map(str::to_string),
        ),
        WmoOnlyKind::SurfaceObservation => unsupported_wmo_bulletin(
            filename,
            header,
            "unsupported_surface_observation_bulletin",
            "recognized valid WMO surface observation bulletin, but parsing is not implemented",
            first_nonempty_line(body_text).map(str::to_string),
        ),
        WmoOnlyKind::UnknownValidWmo => unsupported_wmo_bulletin(
            filename,
            header,
            "unsupported_wmo_bulletin",
            "recognized valid WMO bulletin without AFOS line, but no parser is available",
            first_nonempty_line(body_text).map(str::to_string),
        ),
    }
}

fn classify_wmo_only_bulletin(filename: &str, header: &WmoHeader, body_text: &str) -> WmoOnlyKind {
    if let Some(reference_time) = header.timestamp(Utc::now())
        && looks_like_fd_wmo_bulletin(filename, body_text)
        && parse_fd_bulletin(body_text, Some(filename_stem(filename)), reference_time).is_some()
    {
        return WmoOnlyKind::Fd;
    }
    if parse_metar_bulletin(body_text).is_some() {
        return WmoOnlyKind::Metar;
    }
    if parse_taf_bulletin(body_text).is_some() {
        return WmoOnlyKind::Taf;
    }
    if parse_dcp_bulletin(filename, header, body_text).is_some() {
        return WmoOnlyKind::Dcp;
    }
    if looks_like_sigmet_wmo_bulletin(body_text) && parse_sigmet_bulletin(body_text).is_some() {
        return WmoOnlyKind::Sigmet;
    }
    if looks_like_airmet_wmo_bulletin(body_text) {
        return WmoOnlyKind::Airmet;
    }
    if looks_like_surface_observation_bulletin(body_text) {
        return WmoOnlyKind::SurfaceObservation;
    }
    if looks_like_canadian_text_bulletin(header, body_text) {
        return WmoOnlyKind::CanadianText;
    }
    WmoOnlyKind::UnknownValidWmo
}

fn unsupported_wmo_bulletin(
    filename: &str,
    header: WmoHeader,
    code: &'static str,
    message: &'static str,
    line: Option<String>,
) -> ProductEnrichment {
    ProductEnrichment {
        source: ProductEnrichmentSource::WmoUnsupportedBulletin,
        family: Some("unsupported_wmo_bulletin"),
        title: None,
        container: container_from_filename(filename),
        pil: None,
        wmo_prefix: None,
        office: wmo_office_entry(&header.cccc).copied(),
        header: None,
        wmo_header: Some(header),
        bbb_kind: None,
        body: None,
        metar: None,
        taf: None,
        dcp: None,
        fd: None,
        pirep: None,
        sigmet: None,
        issues: vec![ProductParseIssue::new(
            "wmo_bulletin_parse",
            code,
            message,
            line,
        )],
    }
}

/// Detects if a text product appears to be a PIREP bulletin.
///
/// Checks AFOS code pattern (PIR, PRCUS, PIREP) or body content markers
/// (/OV, /TM, UA, UUA).
fn looks_like_pirep_text_product(afos: &str, body_text: &str) -> bool {
    afos.starts_with("PIR")
        || afos.eq_ignore_ascii_case("PRCUS")
        || afos.eq_ignore_ascii_case("PIREP")
        || ((body_text.contains("/OV ") || body_text.contains("/OV"))
            && body_text.contains("/TM")
            && (body_text.contains(" UA ") || body_text.contains(" UUA ")))
}

/// Detects if a text product appears to be a SIGMET bulletin.
///
/// Checks AFOS code pattern (SIG, WS) or body content markers
/// (CONVECTIVE SIGMET, KZAK SIGMET, SIGMET).
fn looks_like_sigmet_text_product(afos: &str, body_text: &str) -> bool {
    afos.starts_with("SIG")
        || afos.starts_with("WS")
        || body_text.trim_start().starts_with("CONVECTIVE SIGMET ")
        || body_text.trim_start().starts_with("KZAK SIGMET ")
        || body_text.trim_start().starts_with("SIGMET ")
}

fn looks_like_sigmet_wmo_bulletin(body_text: &str) -> bool {
    let Some(first_line) = first_nonempty_line(body_text) else {
        return false;
    };
    first_line.starts_with("SIGMET ")
        || starts_with_icao_sigmet_line(first_line)
        || (first_line.contains(" SIGMET ") && first_line.contains(" VALID "))
}

fn starts_with_icao_sigmet_line(line: &str) -> bool {
    let mut parts = line.split_whitespace();
    let Some(origin) = parts.next() else {
        return false;
    };
    let Some(sigmet) = parts.next() else {
        return false;
    };
    origin.len() == 4 && origin.chars().all(|ch| ch.is_ascii_uppercase()) && sigmet == "SIGMET"
}

fn looks_like_airmet_wmo_bulletin(body_text: &str) -> bool {
    first_nonempty_line(body_text)
        .is_some_and(|line| line.contains(" AIRMET ") && line.contains(" VALID "))
}

fn looks_like_canadian_text_bulletin(header: &WmoHeader, body_text: &str) -> bool {
    header.cccc.starts_with("CW") || body_text.contains("ENVIRONMENT CANADA")
}

fn looks_like_surface_observation_bulletin(body_text: &str) -> bool {
    first_nonempty_line(body_text).is_some_and(|line| line.starts_with("NPL SA "))
}

fn first_nonempty_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

/// Creates an enrichment for an unknown product type.
///
/// Detects container type but sets all metadata to None/Unknown.
fn unknown_product(filename: &str, bytes: &[u8]) -> ProductEnrichment {
    ProductEnrichment {
        source: ProductEnrichmentSource::Unknown,
        family: None,
        title: None,
        container: detected_container(filename, bytes),
        pil: None,
        wmo_prefix: None,
        office: None,
        header: None,
        wmo_header: None,
        bbb_kind: None,
        body: None,
        metar: None,
        taf: None,
        dcp: None,
        fd: None,
        pirep: None,
        sigmet: None,
        issues: Vec::new(),
    }
}

/// Detects the container type from filename and byte content.
///
/// Checks for ZIP magic bytes first, then falls back to filename extension.
fn detected_container(filename: &str, bytes: &[u8]) -> &'static str {
    if is_zip_payload(bytes) {
        "zip"
    } else {
        container_from_filename(filename)
    }
}

/// Checks if byte content appears to be a ZIP archive.
///
/// Validates ZIP magic bytes (PK\x03\x04, PK\x05\x06, or PK\x07\x08).
fn is_zip_payload(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

/// Maps a `ParserError` to a machine-readable error code.
fn parser_error_code(error: &ParserError) -> &'static str {
    match error {
        ParserError::EmptyInput => "empty_input",
        ParserError::MissingWmoLine => "missing_wmo_line",
        ParserError::InvalidWmoHeader { .. } => "invalid_wmo_header",
        ParserError::MissingAfosLine => "missing_afos_line",
        ParserError::MissingAfos { .. } => "missing_afos",
    }
}

/// Extracts the problematic line content from a `ParserError`.
///
/// Returns `Some(line)` for `InvalidWmoHeader` and `MissingAfos` errors.
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
