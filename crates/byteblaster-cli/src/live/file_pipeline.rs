use crate::product_meta::detect_product_meta;
use byteblaster_parser::{BbbKind, ParserError, enrich_header, parse_text_product};
use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, Clone, Default)]
pub(crate) struct TextProductDetails {
    pub(crate) header: Option<serde_json::Value>,
    pub(crate) enrichment: Option<serde_json::Value>,
    pub(crate) warning: Option<serde_json::Value>,
}

pub(crate) struct CompletedFileRecord {
    pub(crate) filename: String,
    pub(crate) path: String,
    pub(crate) timestamp_utc: u64,
    pub(crate) event: serde_json::Value,
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

pub(crate) fn completed_file_event(
    filename: &str,
    path: &str,
    timestamp_utc: u64,
    data: &[u8],
) -> serde_json::Value {
    let mut file_event = serde_json::json!({
        "filename": filename,
        "path": path,
        "timestamp_utc": timestamp_utc,
    });
    if let Some(product) = detect_product_meta(filename)
        && let Ok(product_json) = serde_json::to_value(product)
    {
        file_event["product"] = product_json;
    }
    let text_details = text_product_details(filename, data);
    if let Some(header) = text_details.header {
        file_event["text_product_header"] = header;
    }
    if let Some(enrichment) = text_details.enrichment {
        file_event["text_product_enrichment"] = enrichment;
    }
    if let Some(warning) = text_details.warning {
        file_event["text_product_warning"] = warning;
    }
    file_event
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
        filename: filename.to_string(),
        event: completed_file_event(filename, &path, timestamp_utc, data),
        path,
        timestamp_utc,
    })
}

pub(crate) fn text_product_details(filename: &str, data: &[u8]) -> TextProductDetails {
    if !is_text_weather_product(filename) {
        return TextProductDetails::default();
    }

    match parse_text_product(data) {
        Ok(parsed) => {
            let enrichment = enrich_header(&parsed);
            let header = serde_json::json!({
                "ttaaii": parsed.ttaaii,
                "cccc": parsed.cccc,
                "ddhhmm": parsed.ddhhmm,
                "bbb": parsed.bbb,
                "afos": parsed.afos,
            });
            let enrichment = serde_json::json!({
                "pil_nnn": enrichment.pil_nnn,
                "pil_description": enrichment.pil_description,
                "bbb_kind": enrichment.bbb_kind.map(bbb_kind_label),
            });
            TextProductDetails {
                header: Some(header),
                enrichment: Some(enrichment),
                warning: None,
            }
        }
        Err(error) => {
            let mut warning = serde_json::json!({
                "kind": "text_product_parse",
                "code": parser_error_code(&error),
                "message": error.to_string(),
            });
            if let Some(line) = parser_error_line(&error) {
                warning["line"] = serde_json::Value::String(line.to_string());
            }
            TextProductDetails {
                header: None,
                enrichment: None,
                warning: Some(warning),
            }
        }
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
    use super::completed_file_event;

    #[test]
    fn completed_event_includes_text_header_and_enrichment() {
        let data = b"000 \nFXUS61 KBOX 022101\nAFDBOX\nBody\n";
        let event = completed_file_event("AFDBOX.TXT", "/tmp/AFDBOX.TXT", 1704070800, data);

        assert_eq!(event["text_product_header"]["ttaaii"], "FXUS61");
        assert_eq!(event["text_product_header"]["afos"], "AFDBOX");
        assert_eq!(event["text_product_enrichment"]["pil_nnn"], "AFD");
        assert_eq!(
            event["text_product_enrichment"]["pil_description"],
            "Area Forecast Discussion"
        );
        assert!(event.get("text_product_warning").is_none());
    }

    #[test]
    fn completed_event_includes_warning_for_parse_failure() {
        let data = b"000 \nINVALID HEADER\nAFDBOX\nBody\n";
        let event = completed_file_event("AFDBOX.TXT", "/tmp/AFDBOX.TXT", 1704070800, data);

        assert_eq!(event["text_product_warning"]["kind"], "text_product_parse");
        assert_eq!(event["text_product_warning"]["code"], "invalid_wmo_header");
        assert_eq!(event["text_product_warning"]["line"], "INVALID HEADER");
        assert!(event.get("text_product_header").is_none());
        assert!(event.get("text_product_enrichment").is_none());
    }
}
