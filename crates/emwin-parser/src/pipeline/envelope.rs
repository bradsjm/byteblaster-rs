//! Envelope construction for product enrichment.
//!
//! The envelope stage turns normalized input into a higher-level shape that the
//! classifier and assembler can reason about without repeating header parsing in
//! the orchestrator.

use crate::ParserError;
use crate::data::{NonTextProductMeta, classify_non_text_product};
use crate::header::{parse_text_product_conditioned, parse_wmo_bulletin_conditioned};
use crate::{TextProductHeader, WmoHeader};

use super::NormalizedInput;

/// High-level parseable shape discovered from normalized input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnvelopeKind {
    /// A text product with a valid WMO header and AFOS line.
    TextAfos,
    /// A text product with a valid WMO header but no AFOS line.
    TextWmoOnly,
    /// A known non-text product classified from the filename.
    NonText,
    /// A payload that could not be classified into a richer shape.
    Unknown,
}

/// Internal parse envelope passed between pipeline stages.
///
/// The envelope preserves enough owned state to reconstruct the existing public
/// `ProductEnrichment` output without forcing the top-level orchestrator to
/// re-run header parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedEnvelope {
    /// Original filename of the payload.
    pub(crate) filename: String,
    /// Container kind determined during normalization.
    pub(crate) container: &'static str,
    /// High-level shape chosen by the envelope builder.
    pub(crate) kind: EnvelopeKind,
    /// Parsed AFOS-aware header for text products.
    pub(crate) header: Option<TextProductHeader>,
    /// Parsed WMO-only header for fallback bulletin handling.
    pub(crate) wmo_header: Option<WmoHeader>,
    /// Body text extracted after conditioning and header removal.
    pub(crate) body_text: Option<String>,
    /// Stored parse failure used to preserve legacy issue reporting.
    pub(crate) parse_error: Option<ParserError>,
    /// Filename-derived metadata for non-text products.
    pub(crate) non_text_meta: Option<NonTextProductMeta>,
}

impl ParsedEnvelope {
    /// Builds an envelope from normalized input using the legacy fallback order.
    ///
    /// The order is intentionally strict:
    /// 1. ZIP-framed text stays unknown.
    /// 2. AFOS-aware text parsing wins for text products.
    /// 3. Missing-AFOS text falls back to WMO-only parsing.
    /// 4. Non-text filename classification runs for non-text products only.
    /// 5. Anything else remains unknown, optionally retaining the text parse
    ///    error so assembly can emit the same issue shape as before.
    pub(crate) fn build(normalized: NormalizedInput) -> Self {
        let NormalizedInput {
            filename,
            container,
            bytes,
            text: _text,
            is_text_product,
        } = normalized;

        if container == "zip" && is_text_product {
            return Self::unknown(filename, container, None);
        }

        if is_text_product {
            match parse_text_product_conditioned(&bytes) {
                Ok(parsed) => {
                    return Self {
                        filename,
                        container,
                        kind: EnvelopeKind::TextAfos,
                        header: Some(parsed.header),
                        wmo_header: None,
                        body_text: Some(parsed.body_text),
                        parse_error: None,
                        non_text_meta: None,
                    };
                }
                Err(error) => {
                    let afos_missing = matches!(
                        error,
                        ParserError::MissingAfosLine | ParserError::MissingAfos { .. }
                    );

                    if afos_missing && let Ok(parsed_wmo) = parse_wmo_bulletin_conditioned(&bytes) {
                        return Self {
                            filename,
                            container,
                            kind: EnvelopeKind::TextWmoOnly,
                            header: None,
                            wmo_header: Some(parsed_wmo.header),
                            body_text: Some(parsed_wmo.body_text),
                            parse_error: Some(error),
                            non_text_meta: None,
                        };
                    }

                    return Self::unknown(filename, container, Some(error));
                }
            }
        }

        if let Some(non_text_meta) = classify_non_text_product(&filename) {
            return Self {
                filename,
                container,
                kind: EnvelopeKind::NonText,
                header: None,
                wmo_header: None,
                body_text: None,
                parse_error: None,
                non_text_meta: Some(non_text_meta),
            };
        }

        Self::unknown(filename, container, None)
    }

    fn unknown(
        filename: String,
        container: &'static str,
        parse_error: Option<ParserError>,
    ) -> Self {
        Self {
            filename,
            container,
            kind: EnvelopeKind::Unknown,
            header: None,
            wmo_header: None,
            body_text: None,
            parse_error,
            non_text_meta: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EnvelopeKind, ParsedEnvelope};
    use crate::ParserError;
    use crate::pipeline::NormalizedInput;

    #[test]
    fn afos_text_builds_text_afos_envelope() {
        let normalized = NormalizedInput::from_input(
            "TAFPDKGA.TXT",
            b"000 \nFTUS42 KFFC 022320\nTAFPDK\nBody\n",
        );
        let envelope = ParsedEnvelope::build(normalized);

        assert_eq!(envelope.kind, EnvelopeKind::TextAfos);
        assert_eq!(
            envelope.header.as_ref().map(|header| header.afos.as_str()),
            Some("TAFPDK")
        );
        assert_eq!(envelope.body_text.as_deref(), Some("Body"));
        assert!(envelope.parse_error.is_none());
    }

    #[test]
    fn missing_afos_builds_text_wmo_only_envelope() {
        let normalized = NormalizedInput::from_input(
            "SAGL31.TXT",
            b"000 \nSAGL31 BGGH 070200\nMETAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n",
        );
        let envelope = ParsedEnvelope::build(normalized);

        assert_eq!(envelope.kind, EnvelopeKind::TextWmoOnly);
        assert_eq!(
            envelope
                .wmo_header
                .as_ref()
                .map(|header| header.ttaaii.as_str()),
            Some("SAGL31")
        );
        assert!(matches!(
            envelope.parse_error,
            Some(ParserError::MissingAfos { .. })
        ));
    }

    #[test]
    fn invalid_text_parse_is_preserved_as_unknown() {
        let normalized =
            NormalizedInput::from_input("TAFPDKGA.TXT", b"000 \nINVALID HEADER\nTAFPDK\nBody\n");
        let envelope = ParsedEnvelope::build(normalized);

        assert_eq!(envelope.kind, EnvelopeKind::Unknown);
        assert!(matches!(
            envelope.parse_error,
            Some(ParserError::InvalidWmoHeader { .. })
        ));
    }

    #[test]
    fn known_non_text_file_builds_non_text_envelope() {
        let normalized = NormalizedInput::from_input("RADUMSVY.GIF", b"ignored");
        let envelope = ParsedEnvelope::build(normalized);

        assert_eq!(envelope.kind, EnvelopeKind::NonText);
        assert_eq!(
            envelope.non_text_meta.as_ref().map(|meta| meta.family),
            Some("radar_graphic")
        );
    }

    #[test]
    fn zip_framed_text_stays_unknown() {
        let normalized = NormalizedInput::from_input("TAFALLUS.TXT", b"PK\x03\x04compressed bytes");
        let envelope = ParsedEnvelope::build(normalized);

        assert_eq!(envelope.kind, EnvelopeKind::Unknown);
        assert_eq!(envelope.container, "zip");
        assert!(envelope.parse_error.is_none());
    }
}
