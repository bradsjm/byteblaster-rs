use emwin_parser::{ProductEnrichment, enrich_product};
use serde::Serialize;
use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct CompletedFileMetadata {
    pub(crate) filename: String,
    pub(crate) size: usize,
    pub(crate) timestamp_utc: u64,
    pub(crate) product: ProductEnrichment,
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
    CompletedFileMetadata {
        filename: filename.to_string(),
        size: data.len(),
        timestamp_utc,
        product: enrich_product(filename, data),
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

#[cfg(test)]
mod tests {
    use emwin_parser::ProductEnrichmentSource;

    use super::build_completed_file_metadata;

    #[test]
    fn completed_metadata_uses_header_enrichment_for_valid_text_products() {
        let metadata = build_completed_file_metadata(
            "AFDBOX.TXT",
            1704070800,
            b"000 \nFXUS61 KBOX 022101\nAFDBOX\nBody\n",
        );

        assert_eq!(metadata.product.source, ProductEnrichmentSource::TextHeader);
        assert_eq!(metadata.product.pil.as_deref(), Some("AFD"));
        assert_eq!(metadata.product.title, Some("Area Forecast Discussion"));
        assert!(metadata.product.issues.is_empty());
    }

    #[test]
    fn completed_metadata_surfaces_text_parse_warnings() {
        let metadata = build_completed_file_metadata(
            "AFDBOX.TXT",
            1704070800,
            b"000 \nINVALID HEADER\nAFDBOX\nBody\n",
        );

        assert_eq!(metadata.product.source, ProductEnrichmentSource::TextHeader);
        assert_eq!(
            metadata.product.issues.first().map(|value| value.code),
            Some("invalid_wmo_header")
        );
        assert!(metadata.product.header.is_none());
    }

    #[test]
    fn completed_metadata_treats_zip_framed_txt_payload_as_unknown_zip() {
        let metadata = build_completed_file_metadata(
            "TAFALLUS.TXT",
            1704070800,
            b"PK\x03\x04compressed bytes",
        );

        assert_eq!(metadata.product.source, ProductEnrichmentSource::Unknown);
        assert_eq!(metadata.product.container, "zip");
        assert!(metadata.product.issues.is_empty());
        assert!(metadata.product.header.is_none());
    }
}
