//! Envelope construction for product enrichment.
//!
//! The envelope stage turns normalized input into a higher-level shape that the
//! classifier and assembler can reason about without repeating header parsing in
//! the orchestrator.

use std::ops::Range;

use crate::ParserError;
use crate::data::{NonTextProductMeta, classify_non_text_product};
use crate::header::{
    ParsedTextProductRef, ParsedWmoBulletinRef, condition_text_bytes,
    parse_text_product_conditioned_ref, parse_wmo_bulletin_conditioned_ref,
};
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
/// The envelope owns the normalized backing buffer and stores body ranges into
/// that buffer instead of rebuilding body strings. Headers remain owned in this
/// phase so the phase-2 candidate model can stay unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedEnvelope {
    /// Normalized input buffer carried through the pipeline.
    pub(crate) normalized: NormalizedInput,
    /// High-level shape chosen by the envelope builder.
    pub(crate) kind: EnvelopeKind,
    /// Parsed AFOS-aware header for text products.
    pub(crate) header: Option<TextProductHeader>,
    /// Parsed WMO-only header for fallback bulletin handling.
    pub(crate) wmo_header: Option<WmoHeader>,
    /// Range into the normalized backing buffer for body text.
    pub(crate) body_range: Option<Range<usize>>,
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
    pub(crate) fn build(mut normalized: NormalizedInput) -> Self {
        if normalized.container == "zip" && normalized.is_text_product {
            return Self::unknown(normalized, None);
        }

        if normalized.is_text_product {
            let Some(text_bytes) = normalized.text_bytes() else {
                return Self::unknown(normalized, None);
            };
            let preferred_bytes = normalized.text_str().map_or(text_bytes, str::as_bytes);
            let conditioned = match condition_text_bytes(preferred_bytes) {
                Ok(conditioned) => conditioned,
                Err(error) => return Self::unknown(normalized, Some(error)),
            };

            match parse_text_product_conditioned_ref(&conditioned) {
                Ok(parsed) => {
                    let header = parsed.header.to_owned();
                    let body_range = body_range(&conditioned, parsed);
                    normalized.bytes = conditioned.into_bytes();
                    normalized.text_range = Some(0..normalized.bytes.len());
                    return Self {
                        normalized,
                        kind: EnvelopeKind::TextAfos,
                        header: Some(header),
                        wmo_header: None,
                        body_range,
                        parse_error: None,
                        non_text_meta: None,
                    };
                }
                Err(error) => {
                    let afos_missing = matches!(
                        error,
                        ParserError::MissingAfosLine | ParserError::MissingAfos { .. }
                    );

                    if afos_missing
                        && let Ok(parsed_wmo) = parse_wmo_bulletin_conditioned_ref(&conditioned)
                    {
                        let wmo_header = parsed_wmo.header.to_owned();
                        let body_range = body_range(&conditioned, parsed_wmo);
                        normalized.bytes = conditioned.into_bytes();
                        normalized.text_range = Some(0..normalized.bytes.len());
                        return Self {
                            normalized,
                            kind: EnvelopeKind::TextWmoOnly,
                            header: None,
                            wmo_header: Some(wmo_header),
                            body_range,
                            parse_error: Some(error),
                            non_text_meta: None,
                        };
                    }

                    return Self::unknown(normalized, Some(error));
                }
            }
        }

        if let Some(non_text_meta) = classify_non_text_product(&normalized.filename) {
            return Self {
                normalized,
                kind: EnvelopeKind::NonText,
                header: None,
                wmo_header: None,
                body_range: None,
                parse_error: None,
                non_text_meta: Some(non_text_meta),
            };
        }

        Self::unknown(normalized, None)
    }

    /// Returns the normalized filename.
    pub(crate) fn filename(&self) -> &str {
        &self.normalized.filename
    }

    /// Returns the conditioned text bytes, if the envelope carries text.
    pub(crate) fn text_bytes(&self) -> Option<&[u8]> {
        self.normalized.text_bytes()
    }

    /// Returns the conditioned body text as a borrowed slice of the normalized buffer.
    pub(crate) fn body_text(&self) -> Option<&str> {
        let range = self.body_range.as_ref()?;
        let bytes = self.normalized.bytes.get(range.clone())?;
        std::str::from_utf8(bytes).ok()
    }

    fn unknown(normalized: NormalizedInput, parse_error: Option<ParserError>) -> Self {
        Self {
            normalized,
            kind: EnvelopeKind::Unknown,
            header: None,
            wmo_header: None,
            body_range: None,
            parse_error,
            non_text_meta: None,
        }
    }
}

/// Computes the body slice range within the conditioned backing buffer.
fn body_range<T>(conditioned: &str, parsed: T) -> Option<Range<usize>>
where
    T: BorrowedBodyText,
{
    let body = parsed.body_text();
    let start = body.as_ptr() as usize - conditioned.as_ptr() as usize;
    let end = start + body.len();
    (end <= conditioned.len()).then_some(start..end)
}

trait BorrowedBodyText {
    fn body_text(&self) -> &str;
}

impl BorrowedBodyText for ParsedTextProductRef<'_> {
    fn body_text(&self) -> &str {
        self.body_text
    }
}

impl BorrowedBodyText for ParsedWmoBulletinRef<'_> {
    fn body_text(&self) -> &str {
        self.body_text
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
        assert_eq!(envelope.body_text(), Some("Body\n"));
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
        assert!(envelope.body_range.is_none());
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
        assert_eq!(envelope.normalized.container, "zip");
        assert!(envelope.parse_error.is_none());
    }

    #[test]
    fn wmo_only_envelope_tracks_body_range() {
        let normalized = NormalizedInput::from_input(
            "SAGL31.TXT",
            b"000 \nSAGL31 BGGH 070200\nMETAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n",
        );
        let envelope = ParsedEnvelope::build(normalized);

        assert_eq!(
            envelope.body_text(),
            Some("METAR BGKK 070220Z AUTO VRB02KT 9999NDV OVC043/// M03/M08 Q0967=\n")
        );
    }
}
