//! Assembly of public `ProductEnrichment` values from parsed candidates.
//!
//! Phase 2 removes parser selection from assembly. The classification stage now
//! owns all specialized parsing, and assembly performs a pure conversion from
//! candidate to the public output model.

use crate::data::{NonTextProductMeta, container_from_filename, wmo_office_entry};
use crate::{
    ParserError, ProductEnrichment, ProductEnrichmentSource, ProductParseIssue, wmo_prefix_for_pil,
};
use crate::{ProductBody, body::enrich_body_from_plan};

use super::ClassificationCandidate;
use super::candidate::{
    BodyContributionRequest, Cf6Candidate, CwaCandidate, DcpCandidate, DsmCandidate, FdCandidate,
    HmlCandidate, LsrCandidate, MetarCandidate, MosCandidate, PirepCandidate, SigmetCandidate,
    TafCandidate, TextGenericCandidate, UnsupportedWmoCandidate, WwpCandidate,
};
use super::normalize::detected_container;

/// Assembles the public enrichment result from a parsed classification candidate.
///
/// The filename and raw bytes remain inputs so the unknown-product path can
/// preserve the existing container detection semantics.
pub(crate) fn assemble_product_enrichment(
    candidate: ClassificationCandidate,
    filename: &str,
    raw_bytes: &[u8],
) -> ProductEnrichment {
    match candidate {
        ClassificationCandidate::TextGeneric(candidate) => {
            assemble_from_text_generic(candidate, filename)
        }
        ClassificationCandidate::Fd(candidate) => assemble_from_fd(candidate, filename),
        ClassificationCandidate::Pirep(candidate) => assemble_from_pirep(candidate, filename),
        ClassificationCandidate::Sigmet(candidate) => assemble_from_sigmet(candidate, filename),
        ClassificationCandidate::Lsr(candidate) => assemble_from_lsr(candidate, filename),
        ClassificationCandidate::Cwa(candidate) => assemble_from_cwa(candidate, filename),
        ClassificationCandidate::Wwp(candidate) => assemble_from_wwp(candidate, filename),
        ClassificationCandidate::Cf6(candidate) => assemble_from_cf6(candidate, filename),
        ClassificationCandidate::Dsm(candidate) => assemble_from_dsm(candidate, filename),
        ClassificationCandidate::Hml(candidate) => assemble_from_hml(candidate, filename),
        ClassificationCandidate::Mos(candidate) => assemble_from_mos(candidate, filename),
        ClassificationCandidate::Metar(candidate) => assemble_from_metar(candidate, filename),
        ClassificationCandidate::Taf(candidate) => assemble_from_taf(candidate, filename),
        ClassificationCandidate::Dcp(candidate) => assemble_from_dcp(candidate, filename),
        ClassificationCandidate::NonText(candidate) => assemble_from_non_text(candidate),
        ClassificationCandidate::UnsupportedWmo(candidate) => {
            assemble_from_unsupported_wmo(candidate, filename)
        }
        ClassificationCandidate::TextParseFailure(error) => {
            assemble_from_text_parse_failure(filename, error)
        }
        ClassificationCandidate::Unknown => assemble_unknown(filename, raw_bytes),
    }
}

/// Assembles a generic AFOS text product and runs body enrichment.
fn assemble_from_text_generic(
    candidate: TextGenericCandidate,
    filename: &str,
) -> ProductEnrichment {
    let TextGenericCandidate {
        header,
        pil,
        title,
        body_request,
        bbb_kind,
        reference_time: _reference_time,
    } = candidate;
    let (body, issues) = assemble_optional_body(body_request);

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
        lsr: None,
        cwa: None,
        wwp: None,
        cf6: None,
        dsm: None,
        hml: None,
        mos: None,
        issues,
    }
}

/// Assembles an FD bulletin candidate without reparsing it.
fn assemble_from_fd(candidate: FdCandidate, filename: &str) -> ProductEnrichment {
    let FdCandidate {
        source,
        family,
        title,
        header,
        wmo_header,
        pil,
        bbb_kind,
        body_request,
        bulletin,
    } = candidate;
    let (body, issues) = assemble_optional_body(body_request);
    let office = header
        .as_ref()
        .and_then(|header| wmo_office_entry(&header.cccc).copied())
        .or_else(|| {
            wmo_header
                .as_ref()
                .and_then(|header| wmo_office_entry(&header.cccc).copied())
        });

    ProductEnrichment {
        source,
        family: Some(family),
        title: Some(title),
        container: container_from_filename(filename),
        pil: pil.clone(),
        wmo_prefix: pil.as_deref().and_then(wmo_prefix_for_pil),
        office,
        header,
        wmo_header,
        bbb_kind,
        body,
        metar: None,
        taf: None,
        dcp: None,
        fd: Some(bulletin),
        pirep: None,
        sigmet: None,
        lsr: None,
        cwa: None,
        wwp: None,
        cf6: None,
        dsm: None,
        hml: None,
        mos: None,
        issues,
    }
}

/// Assembles a PIREP bulletin candidate without reparsing it.
fn assemble_from_pirep(candidate: PirepCandidate, filename: &str) -> ProductEnrichment {
    let PirepCandidate {
        header,
        pil,
        bbb_kind,
        body_request,
        bulletin,
    } = candidate;
    let (body, issues) = assemble_optional_body(body_request);

    ProductEnrichment {
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
        body,
        metar: None,
        taf: None,
        dcp: None,
        fd: None,
        pirep: Some(bulletin),
        sigmet: None,
        lsr: None,
        cwa: None,
        wwp: None,
        cf6: None,
        dsm: None,
        hml: None,
        mos: None,
        issues,
    }
}

/// Assembles a SIGMET candidate without reparsing it.
fn assemble_from_sigmet(candidate: SigmetCandidate, filename: &str) -> ProductEnrichment {
    let SigmetCandidate {
        source,
        header,
        wmo_header,
        pil,
        bbb_kind,
        body_request,
        bulletin,
        issues,
    } = candidate;
    let (body, mut body_issues) = assemble_optional_body(body_request);
    body_issues.extend(issues);
    let office = header
        .as_ref()
        .and_then(|header| wmo_office_entry(&header.cccc).copied())
        .or_else(|| {
            wmo_header
                .as_ref()
                .and_then(|header| wmo_office_entry(&header.cccc).copied())
        });

    ProductEnrichment {
        source,
        family: Some("sigmet_bulletin"),
        title: Some("SIGMET bulletin"),
        container: container_from_filename(filename),
        pil: pil.clone(),
        wmo_prefix: pil.as_deref().and_then(wmo_prefix_for_pil),
        office,
        header,
        wmo_header,
        bbb_kind,
        body,
        metar: None,
        taf: None,
        dcp: None,
        fd: None,
        pirep: None,
        sigmet: Some(bulletin),
        lsr: None,
        cwa: None,
        wwp: None,
        cf6: None,
        dsm: None,
        hml: None,
        mos: None,
        issues: body_issues,
    }
}

struct SpecializedTextEnvelope {
    source: ProductEnrichmentSource,
    family: &'static str,
    title: &'static str,
    filename: String,
    header: crate::TextProductHeader,
    pil: Option<String>,
    bbb_kind: Option<crate::BbbKind>,
    issues: Vec<ProductParseIssue>,
}

fn assemble_specialized_text(spec: SpecializedTextEnvelope) -> ProductEnrichment {
    ProductEnrichment {
        source: spec.source,
        family: Some(spec.family),
        title: Some(spec.title),
        container: container_from_filename(&spec.filename),
        pil: spec.pil.clone(),
        wmo_prefix: spec.pil.as_deref().and_then(wmo_prefix_for_pil),
        office: wmo_office_entry(&spec.header.cccc).copied(),
        header: Some(spec.header),
        wmo_header: None,
        bbb_kind: spec.bbb_kind,
        body: None,
        metar: None,
        taf: None,
        dcp: None,
        fd: None,
        pirep: None,
        sigmet: None,
        lsr: None,
        cwa: None,
        wwp: None,
        cf6: None,
        dsm: None,
        hml: None,
        mos: None,
        issues: spec.issues,
    }
}

fn assemble_from_lsr(candidate: LsrCandidate, filename: &str) -> ProductEnrichment {
    let mut enrichment = assemble_specialized_text(SpecializedTextEnvelope {
        source: ProductEnrichmentSource::TextLsrBulletin,
        family: "lsr_bulletin",
        title: "Local Storm Report",
        filename: filename.to_string(),
        header: candidate.header,
        pil: candidate.pil,
        bbb_kind: candidate.bbb_kind,
        issues: candidate.issues,
    });
    enrichment.lsr = Some(candidate.bulletin);
    enrichment
}

fn assemble_from_cwa(candidate: CwaCandidate, filename: &str) -> ProductEnrichment {
    let office = candidate
        .header
        .as_ref()
        .and_then(|header| wmo_office_entry(&header.cccc).copied())
        .or_else(|| {
            candidate
                .wmo_header
                .as_ref()
                .and_then(|header| wmo_office_entry(&header.cccc).copied())
        });

    let mut enrichment = ProductEnrichment {
        source: ProductEnrichmentSource::TextCwaBulletin,
        family: Some("cwa_bulletin"),
        title: Some("Center Weather Advisory"),
        container: container_from_filename(filename),
        pil: candidate.pil.clone(),
        wmo_prefix: candidate.pil.as_deref().and_then(wmo_prefix_for_pil),
        office,
        header: candidate.header,
        wmo_header: candidate.wmo_header,
        bbb_kind: candidate.bbb_kind,
        body: None,
        metar: None,
        taf: None,
        dcp: None,
        fd: None,
        pirep: None,
        sigmet: None,
        lsr: None,
        cwa: None,
        wwp: None,
        cf6: None,
        dsm: None,
        hml: None,
        mos: None,
        issues: candidate.issues,
    };
    enrichment.cwa = Some(candidate.bulletin);
    enrichment
}

fn assemble_from_wwp(candidate: WwpCandidate, filename: &str) -> ProductEnrichment {
    let mut enrichment = assemble_specialized_text(SpecializedTextEnvelope {
        source: ProductEnrichmentSource::TextWwpBulletin,
        family: "wwp_bulletin",
        title: "Severe Thunderstorm Watch Probabilities",
        filename: filename.to_string(),
        header: candidate.header,
        pil: candidate.pil,
        bbb_kind: candidate.bbb_kind,
        issues: candidate.issues,
    });
    enrichment.wwp = Some(candidate.bulletin);
    enrichment
}

fn assemble_from_cf6(candidate: Cf6Candidate, filename: &str) -> ProductEnrichment {
    let mut enrichment = assemble_specialized_text(SpecializedTextEnvelope {
        source: ProductEnrichmentSource::TextCf6Bulletin,
        family: "cf6_bulletin",
        title: "Climate F-6 products",
        filename: filename.to_string(),
        header: candidate.header,
        pil: candidate.pil,
        bbb_kind: candidate.bbb_kind,
        issues: candidate.issues,
    });
    enrichment.cf6 = Some(candidate.bulletin);
    enrichment
}

fn assemble_from_dsm(candidate: DsmCandidate, filename: &str) -> ProductEnrichment {
    let mut enrichment = assemble_specialized_text(SpecializedTextEnvelope {
        source: ProductEnrichmentSource::TextDsmBulletin,
        family: "dsm_bulletin",
        title: "Asos Daily Summary",
        filename: filename.to_string(),
        header: candidate.header,
        pil: candidate.pil,
        bbb_kind: candidate.bbb_kind,
        issues: candidate.issues,
    });
    enrichment.dsm = Some(candidate.bulletin);
    enrichment
}

fn assemble_from_hml(candidate: HmlCandidate, filename: &str) -> ProductEnrichment {
    let mut enrichment = assemble_specialized_text(SpecializedTextEnvelope {
        source: ProductEnrichmentSource::TextHmlBulletin,
        family: "hml_bulletin",
        title: "Hyrdo Obs/Forecasts XML",
        filename: filename.to_string(),
        header: candidate.header,
        pil: candidate.pil,
        bbb_kind: candidate.bbb_kind,
        issues: candidate.issues,
    });
    enrichment.hml = Some(candidate.bulletin);
    enrichment
}

fn assemble_from_mos(candidate: MosCandidate, filename: &str) -> ProductEnrichment {
    let mut enrichment = assemble_specialized_text(SpecializedTextEnvelope {
        source: ProductEnrichmentSource::TextMosBulletin,
        family: "mos_bulletin",
        title: "MOS guidance bulletin",
        filename: filename.to_string(),
        header: candidate.header,
        pil: candidate.pil,
        bbb_kind: candidate.bbb_kind,
        issues: candidate.issues,
    });
    enrichment.mos = Some(candidate.bulletin);
    enrichment
}

fn assemble_optional_body(
    request: Option<BodyContributionRequest>,
) -> (Option<ProductBody>, Vec<ProductParseIssue>) {
    match request {
        Some(request) => {
            let outcome =
                enrich_body_from_plan(&request.text, &request.plan, request.reference_time);
            (outcome.body, outcome.issues)
        }
        None => (None, Vec::new()),
    }
}

/// Assembles a parsed METAR candidate.
fn assemble_from_metar(candidate: MetarCandidate, filename: &str) -> ProductEnrichment {
    let MetarCandidate {
        header,
        bulletin,
        issues,
    } = candidate;

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
        metar: Some(bulletin),
        taf: None,
        dcp: None,
        fd: None,
        pirep: None,
        sigmet: None,
        lsr: None,
        cwa: None,
        wwp: None,
        cf6: None,
        dsm: None,
        hml: None,
        mos: None,
        issues,
    }
}

/// Assembles a parsed TAF candidate.
fn assemble_from_taf(candidate: TafCandidate, filename: &str) -> ProductEnrichment {
    let TafCandidate { header, bulletin } = candidate;

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
        taf: Some(bulletin),
        dcp: None,
        fd: None,
        pirep: None,
        sigmet: None,
        lsr: None,
        cwa: None,
        wwp: None,
        cf6: None,
        dsm: None,
        hml: None,
        mos: None,
        issues: Vec::new(),
    }
}

/// Assembles a parsed DCP candidate.
fn assemble_from_dcp(candidate: DcpCandidate, filename: &str) -> ProductEnrichment {
    let DcpCandidate { header, bulletin } = candidate;

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
        dcp: Some(bulletin),
        fd: None,
        pirep: None,
        sigmet: None,
        lsr: None,
        cwa: None,
        wwp: None,
        cf6: None,
        dsm: None,
        hml: None,
        mos: None,
        issues: Vec::new(),
    }
}

/// Assembles a non-text filename-classified candidate.
fn assemble_from_non_text(candidate: NonTextProductMeta) -> ProductEnrichment {
    ProductEnrichment {
        source: ProductEnrichmentSource::FilenameNonText,
        family: Some(candidate.family),
        title: Some(candidate.title),
        container: candidate.container,
        pil: candidate.pil.map(str::to_string),
        wmo_prefix: candidate.wmo_prefix,
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
        lsr: None,
        cwa: None,
        wwp: None,
        cf6: None,
        dsm: None,
        hml: None,
        mos: None,
        issues: Vec::new(),
    }
}

/// Assembles a recognized unsupported WMO bulletin candidate.
fn assemble_from_unsupported_wmo(
    candidate: UnsupportedWmoCandidate,
    filename: &str,
) -> ProductEnrichment {
    let UnsupportedWmoCandidate {
        header,
        code,
        message,
        line,
    } = candidate;

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
        lsr: None,
        cwa: None,
        wwp: None,
        cf6: None,
        dsm: None,
        hml: None,
        mos: None,
        issues: vec![ProductParseIssue::new(
            "wmo_bulletin_parse",
            code,
            message,
            line,
        )],
    }
}

/// Preserves the legacy issue shape for AFOS text parse failures.
fn assemble_from_text_parse_failure(filename: &str, error: ParserError) -> ProductEnrichment {
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
        lsr: None,
        cwa: None,
        wwp: None,
        cf6: None,
        dsm: None,
        hml: None,
        mos: None,
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
        lsr: None,
        cwa: None,
        wwp: None,
        cf6: None,
        dsm: None,
        hml: None,
        mos: None,
        issues: Vec::new(),
    }
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
    use std::collections::BTreeMap;

    use chrono::Utc;

    use crate::ParserError;
    use crate::specialized::dcp::parse_dcp_bulletin;
    use crate::specialized::fd::parse_fd_bulletin;
    use crate::specialized::metar::{MetarBulletin, parse_metar_bulletin};
    use crate::specialized::pirep::parse_pirep_bulletin;
    use crate::specialized::sigmet::parse_sigmet_bulletin;
    use crate::specialized::taf::parse_taf_bulletin;
    use crate::{
        Cf6Bulletin, Cf6DayRow, CwaBulletin, CwaGeometry, CwaGeometryKind, DsmBulletin, DsmSummary,
        GeoPoint, HmlBulletin, HmlDatum, HmlDocument, HmlSeries, LsrBulletin, LsrReport,
        MosBulletin, MosForecastRow, MosSection, ProductParseIssue, WwpBulletin, WwpWatchType,
    };

    use super::assemble_product_enrichment;
    use crate::pipeline::candidate::{
        BodyContributionRequest, Cf6Candidate, ClassificationCandidate, CwaCandidate, DcpCandidate,
        DsmCandidate, FdCandidate, HmlCandidate, LsrCandidate, MetarCandidate, MosCandidate,
        PirepCandidate, SigmetCandidate, TafCandidate, TextGenericCandidate,
        UnsupportedWmoCandidate, WwpCandidate,
    };

    fn text_header(afos: &str) -> crate::TextProductHeader {
        crate::TextProductHeader {
            ttaaii: "FTUS42".to_string(),
            cccc: "KFFC".to_string(),
            ddhhmm: "022320".to_string(),
            bbb: None,
            afos: afos.to_string(),
        }
    }

    fn wmo_header(ttaaii: &str, cccc: &str) -> crate::WmoHeader {
        crate::WmoHeader {
            ttaaii: ttaaii.to_string(),
            cccc: cccc.to_string(),
            ddhhmm: "070200".to_string(),
            bbb: None,
        }
    }

    #[test]
    fn assembles_text_generic_product_shape() {
        let candidate = ClassificationCandidate::TextGeneric(TextGenericCandidate {
            header: text_header("TAFPDK"),
            pil: Some("TAF".to_string()),
            title: Some("Terminal Aerodrome Forecast"),
            body_request: None,
            bbb_kind: None,
            reference_time: Some(Utc::now()),
        });

        let enrichment = assemble_product_enrichment(candidate, "TAFPDKGA.TXT", b"ignored");

        assert_eq!(
            enrichment.source,
            crate::ProductEnrichmentSource::TextHeader
        );
        assert_eq!(enrichment.pil.as_deref(), Some("TAF"));
        assert_eq!(enrichment.family, Some("nws_text_product"));
    }

    #[test]
    fn assembles_fd_candidate_shape() {
        let bulletin = parse_fd_bulletin(
            "DATA BASED ON 070000Z\nVALID 071200Z\nFT 3000 6000\nBOS 9900 2812\n",
            Some("FD1US1"),
            Utc::now(),
        )
        .expect("fd bulletin should parse");
        let candidate = ClassificationCandidate::Fd(FdCandidate {
            source: crate::ProductEnrichmentSource::TextHeader,
            family: "fd_bulletin",
            title: "Winds and temperatures aloft",
            header: Some(text_header("FD1US1")),
            wmo_header: None,
            pil: Some("FD1".to_string()),
            bbb_kind: None,
            body_request: None,
            bulletin,
        });

        let enrichment = assemble_product_enrichment(candidate, "FD1US1.TXT", b"ignored");

        assert_eq!(enrichment.family, Some("fd_bulletin"));
        assert!(enrichment.fd.is_some());
    }

    #[test]
    fn assembles_pirep_candidate_shape() {
        let bulletin = parse_pirep_bulletin("DEN UA /OV 35 SW /TM 1925 /FL050 /TP E145=\n")
            .expect("pirep bulletin should parse");
        let candidate = ClassificationCandidate::Pirep(PirepCandidate {
            header: text_header("PIRBOU"),
            pil: Some("PIR".to_string()),
            bbb_kind: None,
            body_request: None,
            bulletin,
        });

        let enrichment = assemble_product_enrichment(candidate, "PIRBOU.TXT", b"ignored");

        assert_eq!(
            enrichment.source,
            crate::ProductEnrichmentSource::TextPirepBulletin
        );
        assert!(enrichment.pirep.is_some());
    }

    #[test]
    fn assembles_sigmet_candidate_shape() {
        let bulletin = parse_sigmet_bulletin(
            "CONVECTIVE SIGMET 12C\nVALID UNTIL 2355Z\nIA MO\nFROM 20S DSM-30NW IRK\nAREA EMBD TS MOV FROM 24020KT.\n",
        )
        .expect("sigmet bulletin should parse");
        let candidate = ClassificationCandidate::Sigmet(SigmetCandidate {
            source: crate::ProductEnrichmentSource::TextSigmetBulletin,
            header: Some(text_header("SIGABC")),
            wmo_header: None,
            pil: Some("SIG".to_string()),
            bbb_kind: None,
            body_request: None,
            bulletin,
            issues: Vec::new(),
        });

        let enrichment = assemble_product_enrichment(candidate, "SIGABC.TXT", b"ignored");

        assert_eq!(enrichment.family, Some("sigmet_bulletin"));
        assert!(enrichment.sigmet.is_some());
    }

    #[test]
    fn assembles_lsr_candidate_shape() {
        let issues = vec![ProductParseIssue::new(
            "lsr_parse",
            "invalid_lsr_report",
            "could not parse LSR report block",
            Some("bad chunk".to_string()),
        )];
        let bulletin = LsrBulletin {
            reports: vec![LsrReport {
                valid: "2026-03-10T01:50:00+00:00".to_string(),
                event_text: "HAIL".to_string(),
                city: "BROOKSVILLE".to_string(),
                county: Some("WINSTON".to_string()),
                state: Some("AL".to_string()),
                latitude: 34.40,
                longitude: -87.70,
                source: Some("PUBLIC".to_string()),
                remark: Some("QUARTER SIZE HAIL".to_string()),
                magnitude_value: Some(1.0),
                magnitude_units: Some("IN".to_string()),
                magnitude_qualifier: None,
            }],
            is_summary: false,
        };
        let candidate = ClassificationCandidate::Lsr(LsrCandidate {
            header: text_header("LSRBMX"),
            pil: Some("LSR".to_string()),
            bbb_kind: None,
            body_request: None,
            bulletin,
            issues,
        });

        let enrichment = assemble_product_enrichment(candidate, "LSRBMX.TXT", b"ignored");

        assert_eq!(
            enrichment.source,
            crate::ProductEnrichmentSource::TextLsrBulletin
        );
        assert_eq!(enrichment.family, Some("lsr_bulletin"));
        assert!(enrichment.lsr.is_some());
        assert!(enrichment.cwa.is_none());
        assert!(enrichment.body.is_none());
        assert_eq!(enrichment.issues.len(), 1);
    }

    #[test]
    fn assembles_cwa_candidate_shape() {
        let bulletin = CwaBulletin {
            center: "ZLC".to_string(),
            number: 202,
            issue_time: "2026-03-10T02:29:00+00:00".to_string(),
            expire_time: "2026-03-10T04:30:00+00:00".to_string(),
            is_corrected: false,
            is_cancelled: false,
            narrative: Some("AREA TS.".to_string()),
            geometry: Some(CwaGeometry {
                kind: CwaGeometryKind::Polygon,
                points: vec![
                    GeoPoint {
                        lat: 40.7884,
                        lon: -111.9778,
                    },
                    GeoPoint {
                        lat: 44.7692,
                        lon: -106.9803,
                    },
                ],
            }),
        };
        let candidate = ClassificationCandidate::Cwa(CwaCandidate {
            header: None,
            wmo_header: Some(wmo_header("FAUS22", "KZLC")),
            pil: Some("CWA".to_string()),
            bbb_kind: None,
            body_request: None,
            bulletin,
            issues: Vec::new(),
        });

        let enrichment = assemble_product_enrichment(candidate, "CWAZLC.TXT", b"ignored");

        assert_eq!(
            enrichment.source,
            crate::ProductEnrichmentSource::TextCwaBulletin
        );
        assert_eq!(enrichment.family, Some("cwa_bulletin"));
        assert!(enrichment.header.is_none());
        assert!(enrichment.wmo_header.is_some());
        assert!(enrichment.cwa.is_some());
        assert!(enrichment.wwp.is_none());
        assert!(enrichment.body.is_none());
    }

    #[test]
    fn assembles_wwp_candidate_shape() {
        let bulletin = WwpBulletin {
            watch_type: WwpWatchType::Tornado,
            watch_number: 31,
            prob_tornadoes_2_or_more: 20,
            prob_tornadoes_1_or_more_strong: 10,
            prob_severe_wind_10_or_more: 70,
            prob_wind_1_or_more_65kt: 40,
            prob_severe_hail_10_or_more: 60,
            prob_hail_1_or_more_2inch: 30,
            prob_combined_hail_wind_6_or_more: 95,
            max_hail_inches: 2.0,
            max_wind_gust_knots: 70,
            max_tops_feet: 50_000,
            storm_motion_degrees: 240,
            storm_motion_knots: 35,
            is_pds: false,
        };
        let candidate = ClassificationCandidate::Wwp(WwpCandidate {
            header: text_header("WWP1"),
            pil: Some("WWP".to_string()),
            bbb_kind: None,
            body_request: None,
            bulletin,
            issues: Vec::new(),
        });

        let enrichment = assemble_product_enrichment(candidate, "WWP1.TXT", b"ignored");

        assert_eq!(
            enrichment.source,
            crate::ProductEnrichmentSource::TextWwpBulletin
        );
        assert_eq!(enrichment.family, Some("wwp_bulletin"));
        assert!(enrichment.wwp.is_some());
        assert!(enrichment.cf6.is_none());
        assert!(enrichment.body.is_none());
    }

    #[test]
    fn assembles_cf6_candidate_shape() {
        let issues = vec![ProductParseIssue::new(
            "cf6_parse",
            "trace_value_flattened",
            "trace precipitation or snow value flattened to 0.0",
            Some("1 70 50 ...".to_string()),
        )];
        let bulletin = Cf6Bulletin {
            station: "TEST STATION".to_string(),
            month: 3,
            year: 2026,
            rows: vec![Cf6DayRow {
                day: 1,
                max_temp_f: Some(70),
                min_temp_f: Some(50),
                avg_temp_f: Some(60),
                departure_f: Some(0),
                heating_degree_days: Some(5),
                cooling_degree_days: Some(0),
                precip_inches: Some(0.0),
                snow_inches: Some(0.0),
                snow_depth_inches: Some(0.0),
                avg_wind_mph: Some(8.5),
                max_wind_mph: Some(20),
                avg_wind_dir_degrees: Some(180),
                minutes_sunshine: Some(600),
                possible_sunshine_minutes: Some(720),
                sky_cover: Some("CLR".to_string()),
                weather_codes: Some("RA".to_string()),
                gust_mph: Some(30),
                gust_dir_degrees: Some(190),
            }],
        };
        let candidate = ClassificationCandidate::Cf6(Cf6Candidate {
            header: text_header("CF6GSN"),
            pil: Some("CF6".to_string()),
            bbb_kind: None,
            body_request: None,
            bulletin,
            issues,
        });

        let enrichment = assemble_product_enrichment(candidate, "CF6GSN.TXT", b"ignored");

        assert_eq!(
            enrichment.source,
            crate::ProductEnrichmentSource::TextCf6Bulletin
        );
        assert_eq!(enrichment.family, Some("cf6_bulletin"));
        assert!(enrichment.cf6.is_some());
        assert!(enrichment.dsm.is_none());
        assert!(enrichment.body.is_none());
        assert_eq!(enrichment.issues.len(), 1);
    }

    #[test]
    fn assembles_dsm_candidate_shape() {
        let bulletin = DsmBulletin {
            summaries: vec![DsmSummary {
                station: "KCQC".to_string(),
                date: "2026-03-10".to_string(),
                max_temp_f: Some(63),
                max_temp_time: Some("2026-03-10T15:53:00+00:00".to_string()),
                min_temp_f: Some(40),
                min_temp_time: Some("2026-03-10T06:27:00+00:00".to_string()),
                coop_max_temp_f: Some(63),
                coop_min_temp_f: Some(40),
                min_sea_level_pressure_mb_tenths: Some(9671),
                min_slp_time: Some("2026-03-10T06:08:00+00:00".to_string()),
                precip_day_inches: Some(0.0),
                hourly_precip_inches: vec![Some(0.0); 24],
                avg_wind_mph: Some(28.0),
                max_wind_mph: Some(28.0),
                max_wind_time: Some("2026-03-10T20:59:00+00:00".to_string()),
                max_wind_dir_degrees: Some(280),
                max_gust_mph: Some(43.0),
                max_gust_time: Some("2026-03-10T15:31:00+00:00".to_string()),
                max_gust_dir_degrees: Some(290),
            }],
        };
        let candidate = ClassificationCandidate::Dsm(DsmCandidate {
            header: text_header("DSMCQC"),
            pil: Some("DSM".to_string()),
            bbb_kind: None,
            body_request: None,
            bulletin,
            issues: Vec::new(),
        });

        let enrichment = assemble_product_enrichment(candidate, "DSMCQC.TXT", b"ignored");

        assert_eq!(
            enrichment.source,
            crate::ProductEnrichmentSource::TextDsmBulletin
        );
        assert_eq!(enrichment.family, Some("dsm_bulletin"));
        assert!(enrichment.dsm.is_some());
        assert!(enrichment.hml.is_none());
        assert!(enrichment.body.is_none());
    }

    #[test]
    fn assembles_hml_candidate_shape() {
        let bulletin = HmlBulletin {
            documents: vec![HmlDocument {
                station_id: "AAMC1".to_string(),
                station_name: Some("ARROYO SECO".to_string()),
                originator: Some("MTR".to_string()),
                generation_time: Some("2026-03-10T00:02:00Z".to_string()),
                observed: Some(HmlSeries {
                    issued: Some("2026-03-10T00:00:00Z".to_string()),
                    primary_name: Some("Stage".to_string()),
                    primary_units: Some("FT".to_string()),
                    secondary_name: None,
                    secondary_units: None,
                    rows: vec![HmlDatum {
                        valid: "2026-03-10T00:00:00Z".to_string(),
                        primary: Some(2.5),
                        secondary: None,
                    }],
                }),
                forecast: None,
            }],
        };
        let candidate = ClassificationCandidate::Hml(HmlCandidate {
            header: text_header("HMLMTR"),
            pil: Some("HML".to_string()),
            bbb_kind: None,
            body_request: None,
            bulletin,
            issues: Vec::new(),
        });

        let enrichment = assemble_product_enrichment(candidate, "HMLMTR.TXT", b"ignored");

        assert_eq!(
            enrichment.source,
            crate::ProductEnrichmentSource::TextHmlBulletin
        );
        assert_eq!(enrichment.family, Some("hml_bulletin"));
        assert!(enrichment.hml.is_some());
        assert!(enrichment.mos.is_none());
        assert!(enrichment.body.is_none());
    }

    #[test]
    fn assembles_mos_candidate_shape() {
        let mut values = BTreeMap::new();
        values.insert("TMP".to_string(), "20".to_string());
        values.insert("WSP".to_string(), "05".to_string());
        let bulletin = MosBulletin {
            sections: vec![MosSection {
                station: "KBCK".to_string(),
                model: "NAM".to_string(),
                runtime: "2026-03-10T00:00:00Z".to_string(),
                forecasts: vec![MosForecastRow {
                    valid: "2026-03-10T00:00:00Z".to_string(),
                    values,
                }],
            }],
        };
        let candidate = ClassificationCandidate::Mos(MosCandidate {
            header: text_header("METNC1"),
            pil: Some("MET".to_string()),
            bbb_kind: None,
            body_request: None,
            bulletin,
            issues: Vec::new(),
        });

        let enrichment = assemble_product_enrichment(candidate, "METNC1.TXT", b"ignored");

        assert_eq!(
            enrichment.source,
            crate::ProductEnrichmentSource::TextMosBulletin
        );
        assert_eq!(enrichment.family, Some("mos_bulletin"));
        assert!(enrichment.mos.is_some());
        assert!(enrichment.lsr.is_none());
        assert!(enrichment.body.is_none());
    }

    #[test]
    fn assembles_metar_candidate_shape() {
        let (bulletin, issues) = parse_metar_bulletin(
            "METAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n",
        )
        .expect("metar bulletin should parse");
        let candidate = ClassificationCandidate::Metar(MetarCandidate {
            header: wmo_header("SAGL31", "BGGH"),
            bulletin,
            issues,
        });

        let enrichment = assemble_product_enrichment(candidate, "SAGL31.TXT", b"ignored");

        assert_eq!(
            enrichment.metar.as_ref().map(MetarBulletin::report_count),
            Some(1)
        );
    }

    #[test]
    fn assembles_taf_candidate_shape() {
        let bulletin = parse_taf_bulletin("TAF AMD\nWBCF 070244Z 0703/0803 18012KT P6SM SCT050\n")
            .expect("taf bulletin should parse");
        let candidate = ClassificationCandidate::Taf(TafCandidate {
            header: wmo_header("FTXX01", "KWBC"),
            bulletin,
        });

        let enrichment = assemble_product_enrichment(candidate, "TAFWBCFJ.TXT", b"ignored");

        assert_eq!(enrichment.family, Some("taf_bulletin"));
        assert!(enrichment.taf.is_some());
    }

    #[test]
    fn assembles_dcp_candidate_shape() {
        let header = wmo_header("SXMS50", "KWAL");
        let bulletin = parse_dcp_bulletin(
            "MISDCPSV.TXT",
            &header,
            "83786162 066025814\n16.23\n003\n137\n071\n088\n12.9\n137\n007\n00000\n 42-0NN  45E\n",
        )
        .expect("dcp bulletin should parse");
        let candidate = ClassificationCandidate::Dcp(DcpCandidate { header, bulletin });

        let enrichment = assemble_product_enrichment(candidate, "MISDCPSV.TXT", b"ignored");

        assert_eq!(enrichment.family, Some("dcp_telemetry_bulletin"));
        assert!(enrichment.dcp.is_some());
    }

    #[test]
    fn assembles_unsupported_wmo_candidate_shape() {
        let candidate = ClassificationCandidate::UnsupportedWmo(UnsupportedWmoCandidate {
            header: wmo_header("WAAB31", "LATI"),
            code: "unsupported_airmet_bulletin",
            message: "recognized valid WMO AIRMET bulletin, but textual AIRMET parsing is not implemented",
            line: Some("LAAA AIRMET 1 VALID 090100/090500 LATI-".to_string()),
        });

        let enrichment = assemble_product_enrichment(candidate, "WAAB31.TXT", b"ignored");

        assert_eq!(
            enrichment.source,
            crate::ProductEnrichmentSource::WmoUnsupportedBulletin
        );
        assert_eq!(enrichment.issues[0].code, "unsupported_airmet_bulletin");
    }

    #[test]
    fn assembles_text_parse_failure_issue_shape() {
        let enrichment = assemble_product_enrichment(
            ClassificationCandidate::TextParseFailure(ParserError::InvalidWmoHeader {
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
        let enrichment = assemble_product_enrichment(
            ClassificationCandidate::Unknown,
            "mystery.bin",
            b"ignored",
        );

        assert_eq!(enrichment.source, crate::ProductEnrichmentSource::Unknown);
        assert_eq!(enrichment.container, "raw");
        assert!(enrichment.family.is_none());
    }

    #[test]
    fn text_generic_candidate_assembles_body_from_plan() {
        let candidate = ClassificationCandidate::TextGeneric(TextGenericCandidate {
            header: text_header("TAFPDK"),
            pil: Some("TAF".to_string()),
            title: Some("Terminal Aerodrome Forecast"),
            body_request: Some(BodyContributionRequest {
                text: "/O.NEW.KDMX.TO.W.0001.250301T1200Z-250301T1300Z/".to_string(),
                plan: crate::data::text_product_catalog_entry("SVR")
                    .and_then(crate::data::body_extraction_plan_for_entry)
                    .expect("SVR should have body extraction plan"),
                reference_time: Some(Utc::now()),
            }),
            bbb_kind: None,
            reference_time: Some(Utc::now()),
        });

        let enrichment = assemble_product_enrichment(candidate, "TAFPDKGA.TXT", b"ignored");

        assert!(enrichment.body.is_some());
        assert!(
            enrichment
                .body
                .as_ref()
                .and_then(|body| body.as_vtec_event())
                .is_some()
        );
    }

    #[test]
    fn specialized_candidates_without_body_request_remain_bodyless() {
        let bulletin = parse_pirep_bulletin("DEN UA /OV 35 SW /TM 1925 /FL050 /TP E145=\n")
            .expect("pirep bulletin should parse");
        let candidate = ClassificationCandidate::Pirep(PirepCandidate {
            header: text_header("PIRBOU"),
            pil: Some("PIR".to_string()),
            bbb_kind: None,
            body_request: None,
            bulletin,
        });

        let enrichment = assemble_product_enrichment(candidate, "PIRBOU.TXT", b"ignored");

        assert!(enrichment.body.is_none());
        assert!(enrichment.issues.is_empty());
    }

    #[test]
    fn body_request_issues_are_appended_to_text_generic_output() {
        let candidate = ClassificationCandidate::TextGeneric(TextGenericCandidate {
            header: text_header("ZZZBOX"),
            pil: None,
            title: None,
            body_request: Some(BodyContributionRequest {
                text: "plain text".to_string(),
                plan: crate::body::body_extraction_plan(&[
                    crate::body::BodyExtractorId::TimeMotLoc,
                ]),
                reference_time: None,
            }),
            bbb_kind: None,
            reference_time: None,
        });

        let enrichment = assemble_product_enrichment(candidate, "ZZZBOX.TXT", b"ignored");

        assert_eq!(enrichment.issues.len(), 1);
        assert_eq!(enrichment.issues[0].code, "missing_reference_time");
    }

    #[test]
    fn specialized_candidate_with_body_request_assembles_both_artifact_and_body() {
        let bulletin = parse_sigmet_bulletin(
            "CONVECTIVE SIGMET 12C\nVALID UNTIL 2355Z\nIA MO\nFROM 20S DSM-30NW IRK\nAREA EMBD TS MOV FROM 24020KT.\n",
        )
        .expect("sigmet bulletin should parse");
        let candidate = ClassificationCandidate::Sigmet(SigmetCandidate {
            source: crate::ProductEnrichmentSource::TextSigmetBulletin,
            header: Some(text_header("SIGABC")),
            wmo_header: None,
            pil: Some("SIG".to_string()),
            bbb_kind: None,
            body_request: Some(BodyContributionRequest {
                text: "IAC001-011300-\n/O.NEW.KDMX.TO.W.0001.250301T1200Z-250301T1300Z/\nLAT...LON 4143 9613 4145 9610 4140 9608 4138 9612\n".to_string(),
                plan: crate::body::body_extraction_plan(&[
                    crate::body::BodyExtractorId::VtecEvents,
                ]),
                reference_time: Some(Utc::now()),
            }),
            bulletin,
            issues: Vec::new(),
        });

        let enrichment = assemble_product_enrichment(candidate, "SIGABC.TXT", b"ignored");

        assert!(enrichment.sigmet.is_some());
        assert!(enrichment.body.is_some());
        assert!(
            enrichment
                .body
                .as_ref()
                .and_then(|body| body.as_vtec_event())
                .is_some()
        );
    }
}
