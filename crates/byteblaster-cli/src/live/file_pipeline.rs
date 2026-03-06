use crate::product_meta::{ProductMeta, detect_product_meta};
use byteblaster_parser::{BbbKind, ParserError, enrich_header, parse_text_product};
use serde::Serialize;
use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct TextProductHeaderPayload {
    pub(crate) ttaaii: String,
    pub(crate) cccc: String,
    pub(crate) ddhhmm: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bbb: Option<String>,
    pub(crate) afos: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct TextProductEnrichmentPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pil_nnn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pil_description: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bbb_kind: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct TextProductWarningPayload {
    pub(crate) kind: &'static str,
    pub(crate) code: &'static str,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) line: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub(crate) struct TextProductDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) header: Option<TextProductHeaderPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) enrichment: Option<TextProductEnrichmentPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) warning: Option<TextProductWarningPayload>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct CompletedFileMetadata {
    pub(crate) filename: String,
    pub(crate) size: usize,
    pub(crate) timestamp_utc: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) product: Option<ProductMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text_product_header: Option<TextProductHeaderPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text_product_enrichment: Option<TextProductEnrichmentPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text_product_warning: Option<TextProductWarningPayload>,
}

#[derive(Debug, Clone)]
pub(crate) struct CompletedFileRecord {
    pub(crate) path: String,
    pub(crate) metadata: CompletedFileMetadata,
}

pub(crate) fn write_completed_file(
    output_dir: &Path,
    filename: &str,
    data: &[u8],
) -> crate::error::CliResult<String> {
    let target = output_dir.join(filename);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, data)?;
    Ok(target.to_string_lossy().to_string())
}

pub(crate) fn build_completed_file_metadata(
    filename: &str,
    timestamp_utc: u64,
    data: &[u8],
) -> CompletedFileMetadata {
    let text_details = text_product_details(filename, data);
    CompletedFileMetadata {
        filename: filename.to_string(),
        size: data.len(),
        timestamp_utc,
        product: detect_product_meta(filename),
        text_product_header: text_details.header,
        text_product_enrichment: text_details.enrichment,
        text_product_warning: text_details.warning,
    }
}

pub(crate) fn persist_completed_file(
    output_dir: &Path,
    filename: &str,
    data: &[u8],
    timestamp: SystemTime,
) -> crate::error::CliResult<CompletedFileRecord> {
    let path = write_completed_file(output_dir, filename, data)?;
    let timestamp_utc = crate::live::shared::unix_seconds(timestamp);
    Ok(CompletedFileRecord {
        path,
        metadata: build_completed_file_metadata(filename, timestamp_utc, data),
    })
}

pub(crate) fn text_product_details(filename: &str, data: &[u8]) -> TextProductDetails {
    if !is_text_weather_product(filename) {
        return TextProductDetails::default();
    }

    match parse_text_product(data) {
        Ok(parsed) => {
            let enrichment = enrich_header(&parsed);
            let pil_nnn = enrichment.pil_nnn.map(str::to_string);
            let pil_description = enrichment.pil_description;
            let bbb_kind = enrichment.bbb_kind.map(bbb_kind_label);
            TextProductDetails {
                header: Some(TextProductHeaderPayload {
                    ttaaii: parsed.ttaaii,
                    cccc: parsed.cccc,
                    ddhhmm: parsed.ddhhmm,
                    bbb: parsed.bbb,
                    afos: parsed.afos,
                }),
                enrichment: Some(TextProductEnrichmentPayload {
                    pil_nnn,
                    pil_description,
                    bbb_kind,
                }),
                warning: None,
            }
        }
        Err(error) => TextProductDetails {
            header: None,
            enrichment: None,
            warning: Some(TextProductWarningPayload {
                kind: "text_product_parse",
                code: parser_error_code(&error),
                message: error.to_string(),
                line: parser_error_line(&error).map(str::to_string),
            }),
        },
    }
}

fn is_text_weather_product(filename: &str) -> bool {
    let upper = filename.to_ascii_uppercase();
    upper.ends_with(".TXT") || upper.ends_with(".WMO")
}

fn bbb_kind_label(kind: BbbKind) -> &'static str {
    match kind {
        BbbKind::Amendment => "amendment",
        BbbKind::Correction => "correction",
        BbbKind::DelayedRepeat => "delayed_repeat",
        BbbKind::Other => "other",
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
    use super::{build_completed_file_metadata, text_product_details};

    #[test]
    fn completed_metadata_includes_enrichment_for_valid_text_products() {
        let metadata = build_completed_file_metadata(
            "AFDBOX.TXT",
            1704070800,
            b"000 \nFXUS61 KBOX 022101\nAFDBOX\nBody\n",
        );

        assert_eq!(
            metadata
                .text_product_header
                .as_ref()
                .map(|value| value.afos.as_str()),
            Some("AFDBOX")
        );
        assert_eq!(
            metadata
                .text_product_enrichment
                .as_ref()
                .and_then(|value| value.pil_nnn.as_deref()),
            Some("AFD")
        );
        assert!(metadata.text_product_warning.is_none());
    }

    #[test]
    fn text_products_emit_typed_parse_warnings() {
        let details = text_product_details("AFDBOX.TXT", b"000 \nINVALID HEADER\nAFDBOX\nBody\n");

        assert_eq!(
            details.warning.as_ref().map(|value| value.code),
            Some("invalid_wmo_header")
        );
        assert_eq!(
            details
                .warning
                .as_ref()
                .and_then(|value| value.line.as_deref()),
            Some("INVALID HEADER")
        );
        assert!(details.header.is_none());
        assert!(details.enrichment.is_none());
    }
}
