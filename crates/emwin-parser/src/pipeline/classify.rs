//! Classification handoff between envelope construction and result assembly.
//!
//! Phase 1 keeps classification intentionally shallow. It computes the metadata
//! needed to assemble the existing public output types while leaving specialized
//! parsing in the assembly stage.

use chrono::{DateTime, Utc};

use crate::data::NonTextProductMeta;
use crate::{
    BbbKind, ParserError, ProductMetadataFlags, TextProductHeader, WmoHeader, enrich_header,
};

use super::{EnvelopeKind, ParsedEnvelope};

/// Typed output of the classification stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClassificationOutcome {
    /// Parsed AFOS text product with derived catalog metadata.
    TextAfos(TextAfosOutcome),
    /// Parsed WMO-only text bulletin that must use fallback assembly.
    TextWmo(TextWmoOutcome),
    /// Known non-text product classified from filename metadata.
    NonText(NonTextProductMeta),
    /// Text product parse failure that should surface as a legacy issue.
    TextParseFailure(ParserError),
    /// Payload with no richer classification available.
    Unknown,
}

/// Classification payload for AFOS text products.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextAfosOutcome {
    /// Parsed text product header.
    pub(crate) header: TextProductHeader,
    /// Conditioned body text after stripping header lines.
    pub(crate) body_text: String,
    /// Three-character PIL prefix when present.
    pub(crate) pil: Option<String>,
    /// Human-readable title from the PIL catalog.
    pub(crate) title: Option<&'static str>,
    /// Generic body parsing flags from the PIL catalog.
    pub(crate) flags: Option<ProductMetadataFlags>,
    /// Classified BBB meaning for amendment/correction markers.
    pub(crate) bbb_kind: Option<BbbKind>,
    /// Resolved timestamp used by downstream time-aware body parsers.
    pub(crate) reference_time: Option<DateTime<Utc>>,
}

/// Classification payload for WMO-only text products.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextWmoOutcome {
    /// Parsed WMO header without AFOS information.
    pub(crate) header: WmoHeader,
    /// Conditioned body text after stripping the WMO header.
    pub(crate) body_text: String,
}

/// Classifies an envelope into the next-stage representation used by assembly.
///
/// This stage computes inexpensive derived metadata only. Specialized bulletin
/// parsing remains in assembly during phase 1 so the behavior stays aligned with
/// the pre-pipeline implementation.
pub(crate) fn classify(envelope: &ParsedEnvelope) -> ClassificationOutcome {
    match envelope.kind {
        EnvelopeKind::TextAfos => {
            let Some(header) = envelope.header.clone() else {
                return ClassificationOutcome::Unknown;
            };
            let Some(body_text) = envelope.body_text.clone() else {
                return ClassificationOutcome::Unknown;
            };
            let header_enrichment = enrich_header(&header);

            ClassificationOutcome::TextAfos(TextAfosOutcome {
                pil: header_enrichment.pil_nnn.map(str::to_string),
                title: header_enrichment.pil_description,
                flags: header_enrichment.flags,
                bbb_kind: header_enrichment.bbb_kind,
                reference_time: header.timestamp(Utc::now()),
                header,
                body_text,
            })
        }
        EnvelopeKind::TextWmoOnly => {
            match (envelope.wmo_header.clone(), envelope.body_text.clone()) {
                (Some(header), Some(body_text)) => {
                    ClassificationOutcome::TextWmo(TextWmoOutcome { header, body_text })
                }
                _ => ClassificationOutcome::Unknown,
            }
        }
        EnvelopeKind::NonText => envelope
            .non_text_meta
            .clone()
            .map(ClassificationOutcome::NonText)
            .unwrap_or(ClassificationOutcome::Unknown),
        EnvelopeKind::Unknown => envelope
            .parse_error
            .clone()
            .map(ClassificationOutcome::TextParseFailure)
            .unwrap_or(ClassificationOutcome::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::{ClassificationOutcome, classify};
    use crate::ParserError;
    use crate::pipeline::{NormalizedInput, ParsedEnvelope};

    #[test]
    fn afos_envelope_yields_enriched_text_outcome() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "TAFPDKGA.TXT",
            b"000 \nFTUS42 KFFC 022320\nTAFPDK\nBody\n",
        ));

        let outcome = classify(&envelope);
        let ClassificationOutcome::TextAfos(outcome) = outcome else {
            panic!("expected AFOS classification");
        };

        assert_eq!(outcome.pil.as_deref(), Some("TAF"));
        assert_eq!(outcome.title, Some("Terminal Aerodrome Forecast"));
        assert_eq!(outcome.flags.map(|flags| flags.ugc), Some(false));
        assert!(outcome.reference_time.is_some());
        assert_eq!(outcome.header.afos, "TAFPDK");
    }

    #[test]
    fn wmo_only_envelope_yields_text_wmo_outcome() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "SAGL31.TXT",
            b"000 \nSAGL31 BGGH 070200\nMETAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n",
        ));

        let outcome = classify(&envelope);
        let ClassificationOutcome::TextWmo(outcome) = outcome else {
            panic!("expected WMO-only classification");
        };

        assert_eq!(outcome.header.ttaaii, "SAGL31");
        assert!(outcome.body_text.contains("METAR BGKK"));
    }

    #[test]
    fn parse_error_unknown_becomes_text_parse_failure() {
        let envelope = ParsedEnvelope::build(NormalizedInput::from_input(
            "TAFPDKGA.TXT",
            b"000 \nINVALID HEADER\nTAFPDK\nBody\n",
        ));

        let outcome = classify(&envelope);

        assert!(matches!(
            outcome,
            ClassificationOutcome::TextParseFailure(ParserError::InvalidWmoHeader { .. })
        ));
    }

    #[test]
    fn pure_unknown_stays_unknown() {
        let envelope =
            ParsedEnvelope::build(NormalizedInput::from_input("mystery.bin", b"ignored"));

        assert!(matches!(
            classify(&envelope),
            ClassificationOutcome::Unknown
        ));
    }
}
