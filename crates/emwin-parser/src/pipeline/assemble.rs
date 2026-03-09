//! Assembly of public `ProductEnrichment` values from classified inputs.
//!
//! Phase 1 keeps the legacy parsing precedence but relocates it behind a
//! dedicated assembly stage. Later phases can replace the hard-coded branching
//! without changing the facade in `product.rs`.

use chrono::Utc;

use crate::data::{NonTextProductMeta, container_from_filename, wmo_office_entry};
use crate::dcp::parse_dcp_bulletin;
use crate::fd::parse_fd_bulletin;
use crate::metar::parse_metar_bulletin;
use crate::pirep::parse_pirep_bulletin;
use crate::sigmet::parse_sigmet_bulletin;
use crate::taf::parse_taf_bulletin;
use crate::{
    ParserError, ProductEnrichment, ProductEnrichmentSource, ProductParseIssue, WmoHeader,
    enrich_body, wmo_prefix_for_pil,
};

use super::classify::{ClassificationOutcome, TextAfosOutcome, TextWmoOutcome};
use super::normalize::detected_container;

/// Assembles the public enrichment result from a classified pipeline outcome.
///
/// The filename and raw bytes remain inputs in phase 1 so unknown-product
/// assembly can preserve the current container semantics.
pub(crate) fn assemble_product_enrichment(
    outcome: ClassificationOutcome,
    filename: &str,
    raw_bytes: &[u8],
) -> ProductEnrichment {
    match outcome {
        ClassificationOutcome::TextAfos(outcome) => assemble_text_afos(outcome, filename),
        ClassificationOutcome::TextWmo(outcome) => assemble_text_wmo(outcome, filename),
        ClassificationOutcome::NonText(meta) => assemble_non_text(meta),
        ClassificationOutcome::TextParseFailure(error) => {
            assemble_text_parse_failure(filename, error)
        }
        ClassificationOutcome::Unknown => assemble_unknown(filename, raw_bytes),
    }
}

/// Builds the legacy AFOS text-product enrichment flow, including specialized
/// parsers that historically took precedence over generic body extraction.
fn assemble_text_afos(outcome: TextAfosOutcome, filename: &str) -> ProductEnrichment {
    let TextAfosOutcome {
        header,
        body_text,
        pil,
        title,
        flags,
        bbb_kind,
        reference_time,
    } = outcome;

    if let Some(fd) = reference_time.and_then(|reference_time| {
        looks_like_fd_text_product(&header.afos, &body_text)
            .then(|| parse_fd_bulletin(&body_text, Some(header.afos.as_str()), reference_time))
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

    if let Some(pirep) = looks_like_pirep_text_product(&header.afos, &body_text)
        .then(|| parse_pirep_bulletin(&body_text))
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

    if let Some(sigmet) = looks_like_sigmet_text_product(&header.afos, &body_text)
        .then(|| parse_sigmet_bulletin(&body_text))
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
        enrich_body(&body_text, flags, reference_time)
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

/// Builds the legacy WMO-only fallback result for text bulletins without AFOS.
fn assemble_text_wmo(outcome: TextWmoOutcome, filename: &str) -> ProductEnrichment {
    let TextWmoOutcome { header, body_text } = outcome;

    match classify_wmo_only_bulletin(filename, &header, &body_text) {
        WmoOnlyKind::Fd => {
            let fd = header
                .timestamp(Utc::now())
                .and_then(|reference_time| {
                    parse_fd_bulletin(&body_text, Some(filename_stem(filename)), reference_time)
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
            let (metar, issues) = parse_metar_bulletin(&body_text)
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
            let taf = parse_taf_bulletin(&body_text)
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
            let dcp = parse_dcp_bulletin(filename, &header, &body_text)
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
            let sigmet = parse_sigmet_bulletin(&body_text)
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
            first_nonempty_line(&body_text).map(str::to_string),
        ),
        WmoOnlyKind::CanadianText => unsupported_wmo_bulletin(
            filename,
            header,
            "unsupported_canadian_text_bulletin",
            "recognized valid WMO Canadian text bulletin, but parsing is not implemented",
            first_nonempty_line(&body_text).map(str::to_string),
        ),
        WmoOnlyKind::SurfaceObservation => unsupported_wmo_bulletin(
            filename,
            header,
            "unsupported_surface_observation_bulletin",
            "recognized valid WMO surface observation bulletin, but parsing is not implemented",
            first_nonempty_line(&body_text).map(str::to_string),
        ),
        WmoOnlyKind::UnknownValidWmo => unsupported_wmo_bulletin(
            filename,
            header,
            "unsupported_wmo_bulletin",
            "recognized valid WMO bulletin without AFOS line, but no parser is available",
            first_nonempty_line(&body_text).map(str::to_string),
        ),
    }
}

/// Builds the legacy filename-only non-text classification result.
fn assemble_non_text(meta: NonTextProductMeta) -> ProductEnrichment {
    ProductEnrichment {
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
    }
}

/// Preserves the legacy error reporting shape for unparsed text products.
fn assemble_text_parse_failure(filename: &str, error: ParserError) -> ProductEnrichment {
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

/// Builds the catch-all unknown product result.
fn assemble_unknown(filename: &str, raw_bytes: &[u8]) -> ProductEnrichment {
    ProductEnrichment {
        source: ProductEnrichmentSource::Unknown,
        family: None,
        title: None,
        container: detected_container(filename, raw_bytes),
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

/// Builds an unsupported-but-recognized WMO bulletin result with a stable issue.
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

fn filename_stem(filename: &str) -> &str {
    filename
        .rsplit_once('/')
        .map(|(_, tail)| tail)
        .unwrap_or(filename)
        .split_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(filename)
}

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

fn looks_like_fd_wmo_bulletin(filename: &str, body_text: &str) -> bool {
    filename_stem(filename).starts_with("FD")
        && body_text.contains("DATA BASED ON ")
        && body_text.contains("VALID ")
        && body_text
            .lines()
            .any(|line| line.trim_start().starts_with("FT "))
}

fn looks_like_pirep_text_product(afos: &str, body_text: &str) -> bool {
    afos.starts_with("PIR")
        || afos.eq_ignore_ascii_case("PRCUS")
        || afos.eq_ignore_ascii_case("PIREP")
        || ((body_text.contains("/OV ") || body_text.contains("/OV"))
            && body_text.contains("/TM")
            && (body_text.contains(" UA ") || body_text.contains(" UUA ")))
}

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
    use crate::ParserError;

    use super::assemble_product_enrichment;
    use crate::pipeline::classify::ClassificationOutcome;
    use crate::pipeline::{NormalizedInput, ParsedEnvelope, classify};

    #[test]
    fn assembles_generic_afos_product_shape() {
        let normalized = NormalizedInput::from_input(
            "TAFPDKGA.TXT",
            b"000 \nFTUS42 KFFC 022320\nTAFPDK\nBody\n",
        );
        let envelope = ParsedEnvelope::build(normalized);
        let outcome = classify(&envelope);

        let enrichment = assemble_product_enrichment(outcome, "TAFPDKGA.TXT", b"ignored");

        assert_eq!(
            enrichment.source,
            crate::ProductEnrichmentSource::TextHeader
        );
        assert_eq!(enrichment.pil.as_deref(), Some("TAF"));
        assert_eq!(enrichment.family, Some("nws_text_product"));
    }

    #[test]
    fn assembles_wmo_metar_fallback_shape() {
        let normalized = NormalizedInput::from_input(
            "SAGL31.TXT",
            b"000 \nSAGL31 BGGH 070200\nMETAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n",
        );
        let envelope = ParsedEnvelope::build(normalized);
        let outcome = classify(&envelope);

        let enrichment = assemble_product_enrichment(outcome, "SAGL31.TXT", b"ignored");

        assert_eq!(
            enrichment.source,
            crate::ProductEnrichmentSource::WmoMetarBulletin
        );
        assert_eq!(enrichment.family, Some("metar_collective"));
        assert_eq!(
            enrichment.metar.as_ref().map(MetarBulletin::report_count),
            Some(1)
        );
    }

    #[test]
    fn assembles_text_parse_failure_issue_shape() {
        let enrichment = assemble_product_enrichment(
            ClassificationOutcome::TextParseFailure(ParserError::InvalidWmoHeader {
                line: "INVALID HEADER".to_string(),
            }),
            "TAFPDKGA.TXT",
            b"ignored",
        );

        assert_eq!(
            enrichment.source,
            crate::ProductEnrichmentSource::TextHeader
        );
        assert_eq!(enrichment.issues[0].code, "invalid_wmo_header");
    }

    #[test]
    fn assembles_unknown_non_text_shape() {
        let enrichment =
            assemble_product_enrichment(ClassificationOutcome::Unknown, "mystery.bin", b"ignored");

        assert_eq!(enrichment.source, crate::ProductEnrichmentSource::Unknown);
        assert_eq!(enrichment.container, "raw");
        assert!(enrichment.family.is_none());
    }
}
