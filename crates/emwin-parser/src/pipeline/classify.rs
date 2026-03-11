//! Strategy-based classification for parsed envelopes.
//!
//! Phase 2 moves specialized parser selection out of assembly and into ordered
//! registries. Each strategy either returns a fully parsed candidate or yields
//! `None`, allowing later strategies to run without panicking or reparsing.

use chrono::{DateTime, Utc};

use crate::cf6::parse_cf6_bulletin;
use crate::cwa::parse_cwa_bulletin;
use crate::data::{
    TextProductCatalogEntry, TextProductRouting, body_extraction_plan_for_entry,
    text_product_catalog_entry,
};
use crate::dcp::parse_dcp_bulletin;
use crate::dsm::parse_dsm_bulletin;
use crate::fd::parse_fd_bulletin;
use crate::hml::parse_hml_bulletin;
use crate::lsr::parse_lsr_bulletin;
use crate::metar::parse_metar_bulletin;
use crate::mos::parse_mos_bulletin;
use crate::pirep::parse_pirep_bulletin;
use crate::sigmet::parse_sigmet_bulletin;
use crate::taf::parse_taf_bulletin;
use crate::wwp::parse_wwp_bulletin;
use crate::{BbbKind, ProductEnrichmentSource, TextProductHeader, WmoHeader, enrich_header};

use super::candidate::{
    BodyContributionRequest, Cf6Candidate, ClassificationCandidate, CwaCandidate, DcpCandidate,
    DsmCandidate, FdCandidate, HmlCandidate, LsrCandidate, MetarCandidate, MosCandidate,
    PirepCandidate, SigmetCandidate, TafCandidate, TextGenericCandidate, UnsupportedWmoCandidate,
    WwpCandidate,
};
use super::{EnvelopeKind, ParsedEnvelope};

type TextStrategy = for<'a> fn(&TextClassificationContext<'a>) -> Option<ClassificationCandidate>;
type WmoStrategy = for<'a> fn(&WmoClassificationContext<'a>) -> Option<ClassificationCandidate>;

const TEXT_STRATEGIES: &[TextStrategy] = &[
    classify_text_fd,
    classify_text_pirep,
    classify_text_sigmet,
    classify_text_lsr,
    classify_text_cwa,
    classify_text_wwp,
    classify_text_cf6,
    classify_text_dsm,
    classify_text_hml,
    classify_text_mos,
];

const WMO_STRATEGIES: &[WmoStrategy] = &[
    classify_wmo_fd,
    classify_wmo_metar,
    classify_wmo_taf,
    classify_wmo_dcp,
    classify_wmo_sigmet,
    classify_wmo_cwa,
    classify_wmo_airmet_unsupported,
    classify_wmo_surface_observation_unsupported,
    classify_wmo_canadian_text_unsupported,
    classify_wmo_unknown_valid,
];

/// Borrowed context shared across AFOS text-product strategies.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TextClassificationContext<'a> {
    /// Original filename used for filename-sensitive parsing rules.
    filename: &'a str,
    /// Parsed AFOS text product header.
    header: &'a TextProductHeader,
    /// Conditioned body text after header removal.
    body_text: &'a str,
    /// Full text-product catalog metadata when the PIL is known.
    metadata: Option<&'static TextProductCatalogEntry>,
    /// Three-character PIL prefix when present.
    pil: Option<String>,
    /// Human-readable title from the PIL catalog.
    title: Option<&'static str>,
    /// Generic body extraction plan derived from the PIL catalog.
    body_plan: Option<crate::body::BodyExtractionPlan>,
    /// BBB meaning for amendment/correction markers.
    bbb_kind: Option<BbbKind>,
    /// Timestamp resolved from the WMO header.
    reference_time: Option<DateTime<Utc>>,
}

/// Borrowed context shared across WMO-only fallback strategies.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WmoClassificationContext<'a> {
    /// Original filename used by routing-sensitive parsers.
    filename: &'a str,
    /// Parsed WMO header without AFOS state.
    header: &'a WmoHeader,
    /// Conditioned body text after header removal.
    body_text: &'a str,
}

/// Classifies an envelope into a fully parsed internal candidate.
///
/// Specialized strategies run in explicit priority order. When no specialized
/// text strategy matches, AFOS payloads fall back to a generic text candidate.
/// WMO-only payloads always end in an unsupported-WMO candidate rather than an
/// untyped kind enum.
pub(crate) fn classify(envelope: &ParsedEnvelope) -> ClassificationCandidate {
    match envelope.kind {
        EnvelopeKind::TextAfos => classify_text_envelope(envelope),
        EnvelopeKind::TextWmoOnly => classify_wmo_envelope(envelope),
        EnvelopeKind::NonText => envelope
            .non_text_meta
            .clone()
            .map(ClassificationCandidate::NonText)
            .unwrap_or(ClassificationCandidate::Unknown),
        EnvelopeKind::Unknown => envelope
            .parse_error
            .clone()
            .map(ClassificationCandidate::TextParseFailure)
            .unwrap_or(ClassificationCandidate::Unknown),
    }
}

fn classify_text_envelope(envelope: &ParsedEnvelope) -> ClassificationCandidate {
    if envelope.text_bytes().is_none() {
        return ClassificationCandidate::Unknown;
    }
    let Some(header) = envelope.header.as_ref() else {
        return ClassificationCandidate::Unknown;
    };
    let Some(body_text) = envelope.body_text() else {
        return ClassificationCandidate::Unknown;
    };
    let header_enrichment = enrich_header(header);
    let metadata = header_enrichment
        .pil_nnn
        .and_then(text_product_catalog_entry);
    let context = TextClassificationContext {
        filename: envelope.filename(),
        pil: header_enrichment.pil_nnn.map(str::to_string),
        title: header_enrichment.pil_description,
        body_plan: metadata.and_then(body_extraction_plan_for_entry),
        metadata,
        bbb_kind: header_enrichment.bbb_kind,
        reference_time: header.timestamp(Utc::now()),
        header,
        body_text,
    };

    for strategy in TEXT_STRATEGIES {
        if let Some(candidate) = strategy(&context) {
            return candidate;
        }
    }

    ClassificationCandidate::TextGeneric(TextGenericCandidate {
        header: context.header.clone(),
        pil: context.pil,
        title: context.title,
        body_request: build_body_request(
            context.body_plan,
            context.body_text,
            context.reference_time,
        ),
        bbb_kind: context.bbb_kind,
        reference_time: context.reference_time,
    })
}

fn classify_wmo_envelope(envelope: &ParsedEnvelope) -> ClassificationCandidate {
    if envelope.text_bytes().is_none() {
        return ClassificationCandidate::Unknown;
    }
    let Some(header) = envelope.wmo_header.as_ref() else {
        return ClassificationCandidate::Unknown;
    };
    let Some(body_text) = envelope.body_text() else {
        return ClassificationCandidate::Unknown;
    };
    let context = WmoClassificationContext {
        filename: envelope.filename(),
        header,
        body_text,
    };

    for strategy in WMO_STRATEGIES {
        if let Some(candidate) = strategy(&context) {
            return candidate;
        }
    }

    ClassificationCandidate::Unknown
}

/// Returns the filename stem without path or extension.
fn filename_stem(filename: &str) -> &str {
    filename
        .rsplit_once('/')
        .map(|(_, tail)| tail)
        .unwrap_or(filename)
        .split_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(filename)
}

/// Detects whether AFOS text resembles an FD bulletin.
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

/// Detects whether a WMO-only bulletin resembles an FD bulletin.
fn looks_like_fd_wmo_bulletin(filename: &str, body_text: &str) -> bool {
    filename_stem(filename).starts_with("FD")
        && body_text.contains("DATA BASED ON ")
        && body_text.contains("VALID ")
        && body_text
            .lines()
            .any(|line| line.trim_start().starts_with("FT "))
}

/// Detects whether AFOS text resembles a PIREP bulletin.
fn looks_like_pirep_text_product(afos: &str, body_text: &str) -> bool {
    afos.starts_with("PIR")
        || afos.eq_ignore_ascii_case("PRCUS")
        || afos.eq_ignore_ascii_case("PIREP")
        || ((body_text.contains("/OV ") || body_text.contains("/OV"))
            && body_text.contains("/TM")
            && (body_text.contains(" UA ") || body_text.contains(" UUA ")))
}

/// Detects whether AFOS text resembles a SIGMET bulletin.
fn looks_like_sigmet_text_product(afos: &str, body_text: &str) -> bool {
    afos.starts_with("SIG")
        || afos.starts_with("WS")
        || body_text.trim_start().starts_with("CONVECTIVE SIGMET ")
        || body_text.trim_start().starts_with("KZAK SIGMET ")
        || body_text.trim_start().starts_with("SIGMET ")
}

fn looks_like_lsr_text_product(afos: &str, body_text: &str) -> bool {
    afos.starts_with("LSR") && body_text.contains("..TIME...") && body_text.contains("..DATE...")
}

fn looks_like_cwa_text_product(afos: &str, body_text: &str) -> bool {
    afos.starts_with("CWA")
        || (body_text.contains(" CWA ") && body_text.contains("VALID UNTIL"))
        || body_text
            .lines()
            .next()
            .is_some_and(|line| line.contains(" CWA "))
}

fn looks_like_wwp_text_product(afos: &str, body_text: &str) -> bool {
    afos.starts_with("WWP")
        && body_text.contains("PROBABILITY TABLE:")
        && body_text.contains("ATTRIBUTE TABLE:")
}

fn looks_like_cf6_text_product(afos: &str, body_text: &str) -> bool {
    afos.starts_with("CF6")
        && body_text.contains("PRELIMINARY LOCAL CLIMATOLOGICAL DATA")
        && body_text.contains("MONTH:")
        && body_text.contains("YEAR:")
}

fn looks_like_dsm_text_product(afos: &str, body_text: &str) -> bool {
    afos.starts_with("DSM") && body_text.contains(" DS ") && body_text.contains('=')
}

fn looks_like_hml_text_product(afos: &str, body_text: &str) -> bool {
    afos.starts_with("HML") && body_text.contains("<?xml")
}

fn looks_like_mos_text_product(afos: &str, body_text: &str) -> bool {
    matches!(afos.get(..3), Some("MET" | "MAV" | "MEX" | "FRH" | "FTP"))
        && ((body_text.contains("GUIDANCE")
            && body_text
                .lines()
                .any(|line| line.trim_start().starts_with("HR")))
            || body_text
                .lines()
                .any(|line| line.trim_start().starts_with(".B ")))
}

/// Detects whether WMO-only text resembles a SIGMET bulletin.
fn looks_like_sigmet_wmo_bulletin(body_text: &str) -> bool {
    let Some(first_line) = first_nonempty_line(body_text) else {
        return false;
    };
    first_line.starts_with("SIGMET ")
        || starts_with_icao_sigmet_line(first_line)
        || (first_line.contains(" SIGMET ") && first_line.contains(" VALID "))
}

/// Detects whether WMO-only text resembles an AIRMET bulletin.
fn looks_like_airmet_wmo_bulletin(body_text: &str) -> bool {
    first_nonempty_line(body_text)
        .is_some_and(|line| line.contains(" AIRMET ") && line.contains(" VALID "))
}

/// Detects Canadian Environment Canada text bulletins.
fn looks_like_canadian_text_bulletin(header: &WmoHeader, body_text: &str) -> bool {
    header.cccc.starts_with("CW") || body_text.contains("ENVIRONMENT CANADA")
}

/// Detects unsupported surface observation bulletins.
fn looks_like_surface_observation_bulletin(body_text: &str) -> bool {
    first_nonempty_line(body_text).is_some_and(|line| line.starts_with("NPL SA "))
}

/// Returns the first non-empty line from conditioned body text.
fn first_nonempty_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

/// Checks whether the line begins with `<CCCC> SIGMET`.
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

fn classify_text_fd(context: &TextClassificationContext<'_>) -> Option<ClassificationCandidate> {
    let reference_time = context.reference_time?;
    if context.metadata.map(|entry| entry.routing) != Some(TextProductRouting::Fd) {
        return None;
    }
    if !looks_like_fd_text_product(&context.header.afos, context.body_text) {
        return None;
    }
    let bulletin = parse_fd_bulletin(
        context.body_text,
        Some(context.header.afos.as_str()),
        reference_time,
    )?;

    Some(ClassificationCandidate::Fd(FdCandidate {
        source: ProductEnrichmentSource::TextHeader,
        family: "fd_bulletin",
        title: "Winds and temperatures aloft",
        header: Some(context.header.clone()),
        wmo_header: None,
        pil: context.pil.clone(),
        bbb_kind: context.bbb_kind,
        body_request: build_body_request(
            context.body_plan,
            context.body_text,
            context.reference_time,
        ),
        bulletin,
    }))
}

fn classify_text_pirep(context: &TextClassificationContext<'_>) -> Option<ClassificationCandidate> {
    if context.metadata.map(|entry| entry.routing) != Some(TextProductRouting::Pirep) {
        return None;
    }
    if !looks_like_pirep_text_product(&context.header.afos, context.body_text) {
        return None;
    }
    let bulletin = parse_pirep_bulletin(context.body_text)?;

    Some(ClassificationCandidate::Pirep(PirepCandidate {
        header: context.header.clone(),
        pil: context.pil.clone(),
        bbb_kind: context.bbb_kind,
        body_request: build_body_request(
            context.body_plan,
            context.body_text,
            context.reference_time,
        ),
        bulletin,
    }))
}

fn classify_text_sigmet(
    context: &TextClassificationContext<'_>,
) -> Option<ClassificationCandidate> {
    if context.metadata.map(|entry| entry.routing) != Some(TextProductRouting::Sigmet) {
        return None;
    }
    if !looks_like_sigmet_text_product(&context.header.afos, context.body_text) {
        return None;
    }
    let bulletin = parse_sigmet_bulletin(context.body_text)?;

    Some(ClassificationCandidate::Sigmet(SigmetCandidate {
        source: ProductEnrichmentSource::TextSigmetBulletin,
        header: Some(context.header.clone()),
        wmo_header: None,
        pil: context.pil.clone(),
        bbb_kind: context.bbb_kind,
        body_request: build_body_request(
            context.body_plan,
            context.body_text,
            context.reference_time,
        ),
        bulletin,
        issues: Vec::new(),
    }))
}

fn classify_text_lsr(context: &TextClassificationContext<'_>) -> Option<ClassificationCandidate> {
    if context.metadata.map(|entry| entry.routing) != Some(TextProductRouting::Lsr) {
        return None;
    }
    if !looks_like_lsr_text_product(&context.header.afos, context.body_text) {
        return None;
    }
    let (bulletin, issues) = parse_lsr_bulletin(context.body_text, context.reference_time?)?;
    Some(ClassificationCandidate::Lsr(LsrCandidate {
        header: context.header.clone(),
        pil: context.pil.clone(),
        bbb_kind: context.bbb_kind,
        body_request: None,
        bulletin,
        issues,
    }))
}

fn classify_text_cwa(context: &TextClassificationContext<'_>) -> Option<ClassificationCandidate> {
    if context.metadata.map(|entry| entry.routing) != Some(TextProductRouting::Cwa) {
        return None;
    }
    if !looks_like_cwa_text_product(&context.header.afos, context.body_text) {
        return None;
    }
    let bulletin = parse_cwa_bulletin(context.body_text, context.reference_time?)?;
    Some(ClassificationCandidate::Cwa(CwaCandidate {
        header: Some(context.header.clone()),
        wmo_header: None,
        pil: context.pil.clone(),
        bbb_kind: context.bbb_kind,
        body_request: None,
        bulletin,
        issues: Vec::new(),
    }))
}

fn classify_text_wwp(context: &TextClassificationContext<'_>) -> Option<ClassificationCandidate> {
    if context.metadata.map(|entry| entry.routing) != Some(TextProductRouting::Wwp) {
        return None;
    }
    if !looks_like_wwp_text_product(&context.header.afos, context.body_text) {
        return None;
    }
    let bulletin = parse_wwp_bulletin(context.body_text)?;
    Some(ClassificationCandidate::Wwp(WwpCandidate {
        header: context.header.clone(),
        pil: context.pil.clone(),
        bbb_kind: context.bbb_kind,
        body_request: None,
        bulletin,
        issues: Vec::new(),
    }))
}

fn classify_text_cf6(context: &TextClassificationContext<'_>) -> Option<ClassificationCandidate> {
    if context.metadata.map(|entry| entry.routing) != Some(TextProductRouting::Cf6) {
        return None;
    }
    if !looks_like_cf6_text_product(&context.header.afos, context.body_text) {
        return None;
    }
    let (bulletin, issues) = parse_cf6_bulletin(context.body_text)?;
    Some(ClassificationCandidate::Cf6(Cf6Candidate {
        header: context.header.clone(),
        pil: context.pil.clone(),
        bbb_kind: context.bbb_kind,
        body_request: None,
        bulletin,
        issues,
    }))
}

fn classify_text_dsm(context: &TextClassificationContext<'_>) -> Option<ClassificationCandidate> {
    if context.metadata.map(|entry| entry.routing) != Some(TextProductRouting::Dsm) {
        return None;
    }
    if !looks_like_dsm_text_product(&context.header.afos, context.body_text) {
        return None;
    }
    let bulletin = parse_dsm_bulletin(
        context.body_text,
        context.reference_time.unwrap_or_else(Utc::now),
    )?;
    Some(ClassificationCandidate::Dsm(DsmCandidate {
        header: context.header.clone(),
        pil: context.pil.clone(),
        bbb_kind: context.bbb_kind,
        body_request: None,
        bulletin,
        issues: Vec::new(),
    }))
}

fn classify_text_hml(context: &TextClassificationContext<'_>) -> Option<ClassificationCandidate> {
    if context.metadata.map(|entry| entry.routing) != Some(TextProductRouting::Hml) {
        return None;
    }
    if !looks_like_hml_text_product(&context.header.afos, context.body_text) {
        return None;
    }
    let bulletin = parse_hml_bulletin(context.body_text)?;
    Some(ClassificationCandidate::Hml(HmlCandidate {
        header: context.header.clone(),
        pil: context.pil.clone(),
        bbb_kind: context.bbb_kind,
        body_request: None,
        bulletin,
        issues: Vec::new(),
    }))
}

fn classify_text_mos(context: &TextClassificationContext<'_>) -> Option<ClassificationCandidate> {
    if context.metadata.map(|entry| entry.routing) != Some(TextProductRouting::Mos) {
        return None;
    }
    if !looks_like_mos_text_product(&context.header.afos, context.body_text) {
        return None;
    }
    let bulletin = parse_mos_bulletin(context.body_text, context.reference_time?)?;
    Some(ClassificationCandidate::Mos(MosCandidate {
        header: context.header.clone(),
        pil: context.pil.clone(),
        bbb_kind: context.bbb_kind,
        body_request: None,
        bulletin,
        issues: Vec::new(),
    }))
}

fn build_body_request(
    body_plan: Option<crate::body::BodyExtractionPlan>,
    body_text: &str,
    reference_time: Option<DateTime<Utc>>,
) -> Option<BodyContributionRequest> {
    body_plan.map(|plan| BodyContributionRequest {
        text: body_text.to_string(),
        plan,
        reference_time,
    })
}

fn classify_wmo_fd(context: &WmoClassificationContext<'_>) -> Option<ClassificationCandidate> {
    let reference_time = context.header.timestamp(Utc::now())?;
    if !looks_like_fd_wmo_bulletin(context.filename, context.body_text) {
        return None;
    }
    let bulletin = parse_fd_bulletin(
        context.body_text,
        Some(filename_stem(context.filename)),
        reference_time,
    )?;

    Some(ClassificationCandidate::Fd(FdCandidate {
        source: ProductEnrichmentSource::WmoFdBulletin,
        family: "fd_bulletin",
        title: "Winds and temperatures aloft",
        header: None,
        wmo_header: Some(context.header.clone()),
        pil: None,
        bbb_kind: None,
        body_request: None,
        bulletin,
    }))
}

fn classify_wmo_metar(context: &WmoClassificationContext<'_>) -> Option<ClassificationCandidate> {
    let (bulletin, issues) = parse_metar_bulletin(context.body_text)?;

    Some(ClassificationCandidate::Metar(MetarCandidate {
        header: context.header.clone(),
        bulletin,
        issues,
    }))
}

fn classify_wmo_taf(context: &WmoClassificationContext<'_>) -> Option<ClassificationCandidate> {
    let bulletin = parse_taf_bulletin(context.body_text)?;

    Some(ClassificationCandidate::Taf(TafCandidate {
        header: context.header.clone(),
        bulletin,
    }))
}

fn classify_wmo_dcp(context: &WmoClassificationContext<'_>) -> Option<ClassificationCandidate> {
    let bulletin = parse_dcp_bulletin(context.filename, context.header, context.body_text)?;

    Some(ClassificationCandidate::Dcp(DcpCandidate {
        header: context.header.clone(),
        bulletin,
    }))
}

fn classify_wmo_sigmet(context: &WmoClassificationContext<'_>) -> Option<ClassificationCandidate> {
    if !looks_like_sigmet_wmo_bulletin(context.body_text) {
        return None;
    }
    let bulletin = parse_sigmet_bulletin(context.body_text)?;

    Some(ClassificationCandidate::Sigmet(SigmetCandidate {
        source: ProductEnrichmentSource::WmoSigmetBulletin,
        header: None,
        wmo_header: Some(context.header.clone()),
        pil: None,
        bbb_kind: None,
        body_request: None,
        bulletin,
        issues: Vec::new(),
    }))
}

fn classify_wmo_cwa(context: &WmoClassificationContext<'_>) -> Option<ClassificationCandidate> {
    if !looks_like_cwa_text_product("", context.body_text) {
        return None;
    }
    let bulletin = parse_cwa_bulletin(context.body_text, context.header.timestamp(Utc::now())?)?;

    Some(ClassificationCandidate::Cwa(CwaCandidate {
        header: None,
        wmo_header: Some(context.header.clone()),
        pil: Some("CWA".to_string()),
        bbb_kind: None,
        body_request: None,
        bulletin,
        issues: Vec::new(),
    }))
}

fn classify_wmo_airmet_unsupported(
    context: &WmoClassificationContext<'_>,
) -> Option<ClassificationCandidate> {
    looks_like_airmet_wmo_bulletin(context.body_text).then(|| {
        ClassificationCandidate::UnsupportedWmo(UnsupportedWmoCandidate {
            header: context.header.clone(),
            code: "unsupported_airmet_bulletin",
            message:
                "recognized valid WMO AIRMET bulletin, but textual AIRMET parsing is not implemented",
            line: first_nonempty_line(context.body_text).map(str::to_string),
        })
    })
}

fn classify_wmo_surface_observation_unsupported(
    context: &WmoClassificationContext<'_>,
) -> Option<ClassificationCandidate> {
    looks_like_surface_observation_bulletin(context.body_text).then(|| {
        ClassificationCandidate::UnsupportedWmo(UnsupportedWmoCandidate {
            header: context.header.clone(),
            code: "unsupported_surface_observation_bulletin",
            message:
                "recognized valid WMO surface observation bulletin, but parsing is not implemented",
            line: first_nonempty_line(context.body_text).map(str::to_string),
        })
    })
}

fn classify_wmo_canadian_text_unsupported(
    context: &WmoClassificationContext<'_>,
) -> Option<ClassificationCandidate> {
    looks_like_canadian_text_bulletin(context.header, context.body_text).then(|| {
        ClassificationCandidate::UnsupportedWmo(UnsupportedWmoCandidate {
            header: context.header.clone(),
            code: "unsupported_canadian_text_bulletin",
            message: "recognized valid WMO Canadian text bulletin, but parsing is not implemented",
            line: first_nonempty_line(context.body_text).map(str::to_string),
        })
    })
}

fn classify_wmo_unknown_valid(
    context: &WmoClassificationContext<'_>,
) -> Option<ClassificationCandidate> {
    Some(ClassificationCandidate::UnsupportedWmo(
        UnsupportedWmoCandidate {
            header: context.header.clone(),
            code: "unsupported_wmo_bulletin",
            message: "recognized valid WMO bulletin without AFOS line, but no parser is available",
            line: first_nonempty_line(context.body_text).map(str::to_string),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::{TextClassificationContext, build_body_request, classify, classify_text_fd};
    use crate::body::{BodyExtractorId, body_extraction_plan};
    use crate::data::text_product_catalog_entry;
    use crate::header::BbbKind;
    use crate::pipeline::candidate::{ClassificationCandidate, FdCandidate};
    use crate::pipeline::{NormalizedInput, ParsedEnvelope};
    use crate::{ProductEnrichmentSource, TextProductHeader};
    use chrono::Utc;

    #[test]
    fn afos_fd_strategy_returns_fd_candidate() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "FD1US1.TXT",
            b"000 \nFTUS80 KWBC 070000\nFD1US1\nDATA BASED ON 070000Z\nVALID 071200Z\nFT 3000 6000\nBOS 9900 2812\n",
        ));

        assert!(matches!(
            classify(&envelope),
            ClassificationCandidate::Fd(_)
        ));
    }

    #[test]
    fn afos_pirep_strategy_returns_pirep_candidate() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "PIRXXX.TXT",
            b"000 \nUAUS01 KBOU 070000\nPIRBOU\nDEN UA /OV 35 SW /TM 1925 /FL050 /TP E145=\n",
        ));

        assert!(matches!(
            classify(&envelope),
            ClassificationCandidate::Pirep(_)
        ));
    }

    #[test]
    fn afos_sigmet_strategy_returns_sigmet_candidate() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "SIGABC.TXT",
            b"000 \nWSUS31 KKCI 070000\nSIGABC\nCONVECTIVE SIGMET 12C\nVALID UNTIL 2355Z\nIA MO\nFROM 20S DSM-30NW IRK\nAREA EMBD TS MOV FROM 24020KT.\n",
        ));

        assert!(matches!(
            classify(&envelope),
            ClassificationCandidate::Sigmet(_)
        ));
    }

    #[test]
    fn afos_generic_fallback_returns_text_generic_candidate() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "TAFPDKGA.TXT",
            b"000 \nFTUS42 KFFC 022320\nTAFPDK\nBody\n",
        ));

        assert!(matches!(
            classify(&envelope),
            ClassificationCandidate::TextGeneric(_)
        ));
    }

    #[test]
    fn text_generic_candidate_uses_catalog_body_behavior() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "SVRDMX.TXT",
            b"000 \nWUUS53 KDMX 022320\nSVRDMX\n/O.NEW.KDMX.SV.W.0001.250301T1200Z-250301T1300Z/\n",
        ));

        let ClassificationCandidate::TextGeneric(candidate) = classify(&envelope) else {
            panic!("expected generic text candidate");
        };

        assert!(candidate.body_request.is_some());
    }

    #[test]
    fn text_generic_candidate_omits_body_request_when_catalog_body_behavior_is_never() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "ZZZXXX.TXT",
            b"000 \nFXUS61 KBOX 022101\nZZZBOX\nBody\n",
        ));

        let ClassificationCandidate::TextGeneric(candidate) = classify(&envelope) else {
            panic!("expected generic text candidate");
        };

        assert!(candidate.body_request.is_none());
    }

    #[test]
    fn afos_strategy_precedence_prefers_pirep_when_catalog_routing_matches() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "PIRXXX.TXT",
            b"000 \nUAUS01 KBOU 070000\nPIRBOU\nDEN UA /OV 35 SW /TM 1925 /FL050 /TP E145=\nCONVECTIVE SIGMET 12C\nVALID UNTIL 2355Z\nIA MO\nFROM 20S DSM-30NW IRK\nAREA EMBD TS MOV FROM 24020KT.\n",
        ));

        assert!(matches!(
            classify(&envelope),
            ClassificationCandidate::Pirep(_)
        ));
    }

    #[test]
    fn fd_candidate_has_no_body_request_by_default() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "FD1US1.TXT",
            b"000 \nFTUS80 KWBC 070000\nFD1US1\nDATA BASED ON 070000Z\nVALID 071200Z\nFT 3000 6000\nBOS 9900 2812\n",
        ));

        let ClassificationCandidate::Fd(candidate) = classify(&envelope) else {
            panic!("expected fd candidate");
        };

        assert!(candidate.body_request.is_none());
    }

    #[test]
    fn fd_candidate_body_request_follows_catalog_policy() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "FD1US1.TXT",
            b"000 \nFTUS80 KWBC 070000\nFD1US1\nDATA BASED ON 070000Z\nVALID 071200Z\nFT 3000 6000\nBOS 9900 2812\n",
        ));

        let ClassificationCandidate::Fd(candidate) = classify(&envelope) else {
            panic!("expected fd candidate");
        };

        assert!(candidate.body_request.is_none());
    }

    #[test]
    fn pirep_candidate_body_request_follows_catalog_policy() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "PIRXXX.TXT",
            b"000 \nUAUS01 KBOU 070000\nPIRBOU\nDEN UA /OV 35 SW /TM 1925 /FL050 /TP E145=\n",
        ));

        let ClassificationCandidate::Pirep(candidate) = classify(&envelope) else {
            panic!("expected pirep candidate");
        };

        assert!(candidate.body_request.is_none());
    }

    #[test]
    fn sigmet_candidate_body_request_follows_catalog_policy() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "SIGABC.TXT",
            b"000 \nWSUS31 KKCI 070000\nSIGABC\nCONVECTIVE SIGMET 12C\nVALID UNTIL 2355Z\nIA MO\nFROM 20S DSM-30NW IRK\nAREA EMBD TS MOV FROM 24020KT.\n",
        ));

        let ClassificationCandidate::Sigmet(candidate) = classify(&envelope) else {
            panic!("expected sigmet candidate");
        };

        assert!(candidate.body_request.is_none());
    }

    #[test]
    fn specialized_strategy_requires_matching_catalog_routing() {
        let metadata = text_product_catalog_entry("PIR").expect("expected catalog entry");
        let header = TextProductHeader {
            ttaaii: "FTUS80".to_string(),
            cccc: "KWBC".to_string(),
            ddhhmm: "070000".to_string(),
            bbb: None,
            afos: "FD1US1".to_string(),
        };
        let context = TextClassificationContext {
            filename: "FD1US1.TXT",
            header: &header,
            body_text: "DATA BASED ON 070000Z\nVALID 071200Z\nFT 3000 6000\nBOS 9900 2812\n",
            metadata: Some(metadata),
            pil: Some("FD1".to_string()),
            title: Some("Winds and Temperatures Aloft"),
            body_plan: None,
            bbb_kind: None,
            reference_time: Some(Utc::now()),
        };

        assert!(classify_text_fd(&context).is_none());
    }

    #[test]
    fn specialized_candidate_can_carry_body_request_when_metadata_enables_catalog_body_behavior() {
        let request = build_body_request(
            Some(body_extraction_plan(&[BodyExtractorId::Vtec])),
            "/O.NEW.KDMX.TO.W.0001.250301T1200Z-250301T1300Z/",
            Some(Utc::now()),
        );

        let candidate = ClassificationCandidate::Fd(FdCandidate {
            source: ProductEnrichmentSource::TextHeader,
            family: "fd_bulletin",
            title: "Winds and temperatures aloft",
            header: Some(TextProductHeader {
                ttaaii: "FTUS80".to_string(),
                cccc: "KWBC".to_string(),
                ddhhmm: "070000".to_string(),
                bbb: None,
                afos: "FD1US1".to_string(),
            }),
            wmo_header: None,
            pil: Some("FD1".to_string()),
            bbb_kind: Some(BbbKind::Amendment),
            body_request: request,
            bulletin: crate::fd::parse_fd_bulletin(
                "DATA BASED ON 070000Z\nVALID 071200Z\nFT 3000 6000\nBOS 9900 2812\n",
                Some("FD1US1"),
                Utc::now(),
            )
            .expect("fd bulletin should parse"),
        });

        let ClassificationCandidate::Fd(candidate) = candidate else {
            panic!("expected fd candidate");
        };
        assert!(candidate.body_request.is_some());
    }

    #[test]
    fn wmo_metar_strategy_returns_metar_candidate() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "SAGL31.TXT",
            b"000 \nSAGL31 BGGH 070200\nMETAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n",
        ));

        assert!(matches!(
            classify(&envelope),
            ClassificationCandidate::Metar(_)
        ));
    }

    #[test]
    fn wmo_taf_strategy_returns_taf_candidate() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "TAFWBCFJ.TXT",
            b"000 \nFTXX01 KWBC 070200\nTAF AMD\nWBCF 070244Z 0703/0803 18012KT P6SM SCT050\n",
        ));

        assert!(matches!(
            classify(&envelope),
            ClassificationCandidate::Taf(_)
        ));
    }

    #[test]
    fn wmo_dcp_strategy_returns_dcp_candidate() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "MISDCPSV.TXT",
            b"SXMS50 KWAL 070258\n83786162 066025814\n16.23\n003\n137\n071\n088\n12.9\n137\n007\n00000\n 42-0NN  45E\n",
        ));

        assert!(matches!(
            classify(&envelope),
            ClassificationCandidate::Dcp(_)
        ));
    }

    #[test]
    fn wmo_sigmet_strategy_returns_sigmet_candidate() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "WVID21.TXT",
            b"WVID21 WAAA 090100\nWAAF SIGMET 05 VALID 090100/090700 WAAA-\nWAAF UJUNG PANDANG FIR VA ERUPTION MT IBU=\n",
        ));

        assert!(matches!(
            classify(&envelope),
            ClassificationCandidate::Sigmet(_)
        ));
    }

    #[test]
    fn wmo_airmet_returns_unsupported_candidate() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "WAAB31.TXT",
            b"WAAB31 LATI 090038\nLAAA AIRMET 1 VALID 090100/090500 LATI-\nLAAA TIRANA FIR MOD ICE=\n",
        ));

        assert!(matches!(
            classify(&envelope),
            ClassificationCandidate::UnsupportedWmo(_)
        ));
    }

    #[test]
    fn wmo_canadian_text_returns_unsupported_candidate() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "FPCN11.TXT",
            b"FPCN11 CWWG 090059 AAD\nUPDATED FORECASTS FOR SOUTHERN MANITOBA ISSUED BY ENVIRONMENT CANADA\n",
        ));

        assert!(matches!(
            classify(&envelope),
            ClassificationCandidate::UnsupportedWmo(_)
        ));
    }

    #[test]
    fn wmo_surface_observation_returns_unsupported_candidate() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "SAHOURLY.TXT",
            b"SACN74 CWAO 090000 RRC\n\nNPL SA 0000 AUTO8 M M M=\n",
        ));

        assert!(matches!(
            classify(&envelope),
            ClassificationCandidate::UnsupportedWmo(_)
        ));
    }

    #[test]
    fn unknown_valid_wmo_returns_generic_unsupported_candidate() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "UNKNOWN.TXT",
            b"ABCD12 EFGH 090000\nSOME UNKNOWN BODY\n",
        ));

        let ClassificationCandidate::UnsupportedWmo(candidate) = classify(&envelope) else {
            panic!("expected unsupported WMO candidate");
        };

        assert_eq!(candidate.code, "unsupported_wmo_bulletin");
    }

    #[test]
    fn failed_fd_parse_falls_through_to_metar_candidate() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "FDFAIL.TXT",
            b"000 \nSAGL31 BGGH 070200\nDATA BASED ON 070000Z\nVALID 071200Z\nFT 3000 6000\nMETAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n",
        ));

        assert!(matches!(
            classify(&envelope),
            ClassificationCandidate::Metar(_)
        ));
    }
}
