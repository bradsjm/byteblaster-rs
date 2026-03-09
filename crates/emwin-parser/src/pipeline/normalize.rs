//! Input normalization primitives for product enrichment.
//!
//! This stage captures the earliest shared facts about an input payload:
//! filename, container type, whether the file should be treated as text, and a
//! single owned backing buffer for any normalized text content. It deliberately
//! avoids header parsing so the later envelope stage remains the single place
//! that interprets bulletin structure.

use std::ops::Range;

use bstr::ByteSlice;

/// Early normalized representation of a product payload.
///
/// Text payloads use a single normalized backing buffer.
///
/// For text products the buffer strips `\0` and `\r` exactly once, and
/// `text_range` points at the normalized text region within `bytes`. Non-text
/// payloads and opaque ZIP-framed text products keep their raw bytes and leave
/// `text_range` unset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedInput {
    /// Original filename supplied by the caller.
    pub(crate) filename: String,
    /// Detected container kind derived from filename and magic bytes.
    pub(crate) container: &'static str,
    /// Single owned backing buffer preserved for later assembly decisions.
    pub(crate) bytes: Vec<u8>,
    /// Range covering the normalized text view within `bytes`.
    pub(crate) text_range: Option<Range<usize>>,
    /// Whether the filename implies a text/WMO payload.
    pub(crate) is_text_product: bool,
}

impl NormalizedInput {
    /// Builds a normalized view of the caller-provided input.
    ///
    /// ZIP-framed text filenames are intentionally kept opaque so later stages
    /// preserve the existing "unknown zip" behavior.
    pub(crate) fn from_input(filename: &str, bytes: &[u8]) -> Self {
        let is_text_product = is_text_product(filename);
        let container = detected_container(filename, bytes);
        let (bytes, text_range) = if is_text_product && container != "zip" {
            let normalized = normalize_text_bytes(bytes);
            let len = normalized.len();
            (normalized, Some(0..len))
        } else {
            (bytes.to_vec(), None)
        };

        Self {
            filename: filename.to_string(),
            container,
            bytes,
            text_range,
            is_text_product,
        }
    }

    /// Returns the normalized text bytes when this payload is treated as text.
    pub(crate) fn text_bytes(&self) -> Option<&[u8]> {
        let range = self.text_range.as_ref()?;
        self.bytes.get(range.clone())
    }

    /// Returns the normalized text view when the backing bytes are valid UTF-8.
    ///
    /// Text payloads usually satisfy this after conditioning, but callers that
    /// need lossy conversion should do so at the parser boundary instead.
    pub(crate) fn text_str(&self) -> Option<&str> {
        self.text_bytes()?.to_str().ok()
    }
}

/// Returns whether the filename identifies a text bulletin payload.
///
/// EMWIN text products conventionally use `.TXT` or `.WMO` extensions.
pub(crate) fn is_text_product(filename: &str) -> bool {
    let upper = filename.to_ascii_uppercase();
    upper.ends_with(".TXT") || upper.ends_with(".WMO")
}

/// Detects the container type from filename and byte content.
///
/// ZIP magic bytes take precedence over the filename extension so ZIP-framed
/// text files remain opaque to the parser.
pub(crate) fn detected_container(filename: &str, bytes: &[u8]) -> &'static str {
    if is_zip_payload(bytes) {
        "zip"
    } else {
        crate::data::container_from_filename(filename)
    }
}

/// Checks whether the payload begins with a recognized ZIP signature.
fn is_zip_payload(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

/// Normalizes raw text bytes by stripping carriage returns and null bytes.
fn normalize_text_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        if byte != b'\r' && byte != 0 {
            normalized.push(byte);
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::NormalizedInput;

    #[test]
    fn text_filename_is_recognized() {
        let normalized = NormalizedInput::from_input("TAFALLUS.TXT", b"plain text");

        assert!(normalized.is_text_product);
        assert_eq!(normalized.container, "raw");
        assert_eq!(normalized.text_range, Some(0.."plain text".len()));
        assert_eq!(normalized.text_str(), Some("plain text"));
    }

    #[test]
    fn non_text_filename_is_not_recognized_as_text() {
        let normalized = NormalizedInput::from_input("RADUMSVY.GIF", b"ignored");

        assert!(!normalized.is_text_product);
        assert_eq!(normalized.container, "raw");
        assert!(normalized.text_range.is_none());
        assert!(normalized.text_bytes().is_none());
    }

    #[test]
    fn zip_payload_with_text_filename_stays_opaque() {
        let normalized = NormalizedInput::from_input("TAFALLUS.TXT", b"PK\x03\x04compressed bytes");

        assert!(normalized.is_text_product);
        assert_eq!(normalized.container, "zip");
        assert!(normalized.text_range.is_none());
    }

    #[test]
    fn text_range_uses_normalized_bytes() {
        let normalized = NormalizedInput::from_input("TAFALLUS.TXT", b"line1\r\nline\0 2\n");

        assert_eq!(normalized.text_str(), Some("line1\nline 2\n"));
    }
}
