//! Input normalization primitives for product enrichment.
//!
//! This stage captures the earliest shared facts about an input payload:
//! filename, container type, whether the file should be treated as text, and
//! an owned copy of the raw bytes. It deliberately avoids header parsing so the
//! later envelope stage remains the single place that interprets bulletin
//! structure.

/// Early normalized representation of a product payload.
///
/// This type is intentionally simple and fully owned in phase 1. Later phases
/// can replace the owned fields with borrowed views once the stage boundaries
/// are stable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedInput {
    /// Original filename supplied by the caller.
    pub(crate) filename: String,
    /// Detected container kind derived from filename and magic bytes.
    pub(crate) container: &'static str,
    /// Owned payload bytes preserved for later assembly decisions.
    pub(crate) bytes: Vec<u8>,
    /// UTF-8 lossy text representation for text products when the payload is
    /// not treated as an opaque ZIP container.
    pub(crate) text: Option<String>,
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
        let text = (is_text_product && container != "zip")
            .then(|| String::from_utf8_lossy(bytes).into_owned());

        Self {
            filename: filename.to_string(),
            container,
            bytes: bytes.to_vec(),
            text,
            is_text_product,
        }
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

#[cfg(test)]
mod tests {
    use super::NormalizedInput;

    #[test]
    fn text_filename_is_recognized() {
        let normalized = NormalizedInput::from_input("TAFALLUS.TXT", b"plain text");

        assert!(normalized.is_text_product);
        assert_eq!(normalized.container, "raw");
        assert_eq!(normalized.text.as_deref(), Some("plain text"));
    }

    #[test]
    fn non_text_filename_is_not_recognized_as_text() {
        let normalized = NormalizedInput::from_input("RADUMSVY.GIF", b"ignored");

        assert!(!normalized.is_text_product);
        assert_eq!(normalized.container, "raw");
        assert!(normalized.text.is_none());
    }

    #[test]
    fn zip_payload_with_text_filename_stays_opaque() {
        let normalized = NormalizedInput::from_input("TAFALLUS.TXT", b"PK\x03\x04compressed bytes");

        assert!(normalized.is_text_product);
        assert_eq!(normalized.container, "zip");
        assert!(normalized.text.is_none());
    }
}
