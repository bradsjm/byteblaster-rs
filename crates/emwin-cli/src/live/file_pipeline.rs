//! Write completed files and their metadata sidecars for live mode.
//!
//! The file pipeline keeps persistence concerns separate from stream rendering so the same
//! assembled payload can be written to disk and broadcast to other consumers.

use emwin_parser::{
    ProductDetailV2, ProductEnrichment, ProductSummaryV2, detail_product_v2, enrich_product,
    summarize_product_v2,
};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use std::path::{Path, PathBuf};

/// Serializable metadata emitted beside a completed output file.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompletedFileMetadata {
    pub(crate) filename: String,
    pub(crate) size: usize,
    pub(crate) timestamp_utc: u64,
    pub(crate) product: ProductEnrichment,
    pub(crate) product_summary: ProductSummaryV2,
    pub(crate) product_detail: ProductDetailV2,
}

impl Serialize for CompletedFileMetadata {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("CompletedFileMetadata", 4)?;
        state.serialize_field("filename", &self.filename)?;
        state.serialize_field("size", &self.size)?;
        state.serialize_field("timestamp_utc", &self.timestamp_utc)?;
        state.serialize_field("product", &self.product_detail)?;
        state.end()
    }
}

/// Paths and metadata returned after a file plus sidecar have been written.
#[derive(Debug, Clone)]
pub(crate) struct CompletedFileRecord {
    pub(crate) path: String,
    pub(crate) metadata_path: String,
    pub(crate) metadata: CompletedFileMetadata,
}

/// Persists an assembled file and returns its displayable path.
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

/// Returns the sibling `.JSON` sidecar path for a completed file.
pub(crate) fn metadata_sidecar_path(output_dir: &Path, filename: &str) -> PathBuf {
    let target = output_dir.join(filename);
    match target.extension() {
        Some(_) => target.with_extension("JSON"),
        None => {
            let mut path = target.into_os_string();
            path.push(".JSON");
            PathBuf::from(path)
        }
    }
}

pub(crate) fn write_completed_metadata_json(
    output_dir: &Path,
    filename: &str,
    metadata: &CompletedFileMetadata,
) -> crate::error::CliResult<String> {
    let target = metadata_sidecar_path(output_dir, filename);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, serde_json::to_vec_pretty(metadata)?)?;
    Ok(target.to_string_lossy().to_string())
}

pub(crate) fn build_completed_file_metadata(
    filename: &str,
    timestamp_utc: u64,
    data: &[u8],
) -> CompletedFileMetadata {
    let product = enrich_product(filename, data);
    CompletedFileMetadata {
        filename: filename.to_string(),
        size: data.len(),
        timestamp_utc,
        product_summary: summarize_product_v2(&product),
        product_detail: detail_product_v2(&product),
        product,
    }
}

pub(crate) fn persist_completed_record(
    output_dir: &Path,
    filename: &str,
    data: &[u8],
    metadata: CompletedFileMetadata,
) -> crate::error::CliResult<CompletedFileRecord> {
    let path = write_completed_file(output_dir, filename, data)?;
    let metadata_path = write_completed_metadata_json(output_dir, filename, &metadata)?;
    Ok(CompletedFileRecord {
        path,
        metadata_path,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use crate::live::archive_postprocess::post_process_archive;
    use emwin_parser::ProductEnrichmentSource;

    use super::{
        build_completed_file_metadata, metadata_sidecar_path, persist_completed_record,
        write_completed_metadata_json,
    };
    use std::path::{Path, PathBuf};

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

    #[test]
    fn completed_metadata_parses_extracted_archive_content_like_plain_text() {
        let archive = {
            let cursor = std::io::Cursor::new(Vec::new());
            let mut writer = zip::ZipWriter::new(cursor);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            use std::io::Write;
            writer
                .start_file("nested/AFDBOX.TXT", options)
                .expect("start file should succeed");
            writer
                .write_all(b"000 \nFXUS61 KBOX 022101\nAFDBOX\nBody\n")
                .expect("write should succeed");
            writer.finish().expect("finish should succeed").into_inner()
        };
        let delivered = post_process_archive(true, "AFDBOX.ZIP", &archive)
            .expect("archive post-processing should succeed");

        let metadata =
            build_completed_file_metadata(&delivered.filename, 1704070800, &delivered.data);

        assert_eq!(metadata.filename, "nested/AFDBOX.TXT");
        assert_eq!(metadata.product.source, ProductEnrichmentSource::TextHeader);
        assert_eq!(metadata.product.pil.as_deref(), Some("AFD"));
        assert_eq!(metadata.product.container, "raw");
    }

    #[test]
    fn metadata_sidecar_replaces_extension() {
        let path = metadata_sidecar_path(Path::new("/tmp/out"), "nested/AFDBOX.TXT");
        assert_eq!(path, PathBuf::from("/tmp/out/nested/AFDBOX.JSON"));
    }

    #[test]
    fn metadata_sidecar_appends_when_no_extension_exists() {
        let path = metadata_sidecar_path(Path::new("/tmp/out"), "nested/AFDBOX");
        assert_eq!(path, PathBuf::from("/tmp/out/nested/AFDBOX.JSON"));
    }

    #[test]
    fn metadata_json_round_trips() {
        let tmp = tempfile::tempdir().expect("tempdir should exist");
        let metadata = build_completed_file_metadata(
            "AFDBOX.TXT",
            1704070800,
            b"000 \nINVALID HEADER\nAFDBOX\nBody\n",
        );

        let path = write_completed_metadata_json(tmp.path(), "AFDBOX.TXT", &metadata)
            .expect("metadata should write");
        let decoded: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).expect("metadata file should be readable"))
                .expect("metadata json should decode");

        assert_eq!(decoded["filename"], "AFDBOX.TXT");
        assert_eq!(decoded["size"], metadata.size);
        assert_eq!(decoded["product"]["schema_version"], 2);
        assert_eq!(
            decoded["product"]["issues"][0]["code"],
            "invalid_wmo_header"
        );
        assert!(decoded["product"].get("parsed").is_none());
    }

    #[test]
    fn persist_completed_record_writes_payload_and_metadata_sidecar() {
        let tmp = tempfile::tempdir().expect("tempdir should exist");
        let payload = b"000 \nFXUS61 KBOX 022101\nAFDBOX\nBody\n";
        let metadata = build_completed_file_metadata("nested/AFDBOX.TXT", 1704070800, payload);

        let record = persist_completed_record(tmp.path(), "nested/AFDBOX.TXT", payload, metadata)
            .expect("completed record should persist");

        assert_eq!(
            std::fs::read(&record.path).expect("payload file should be readable"),
            payload
        );

        let sidecar: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&record.metadata_path).expect("metadata file should be readable"),
        )
        .expect("metadata json should decode");

        assert_eq!(sidecar["filename"], "nested/AFDBOX.TXT");
        assert_eq!(
            PathBuf::from(&record.metadata_path),
            tmp.path().join("nested/AFDBOX.JSON")
        );
    }

    #[test]
    fn persist_completed_record_uses_extracted_filename_and_bytes() {
        let archive = {
            let cursor = std::io::Cursor::new(Vec::new());
            let mut writer = zip::ZipWriter::new(cursor);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            use std::io::Write;
            writer
                .start_file("nested/AFDBOX.TXT", options)
                .expect("start file should succeed");
            writer
                .write_all(b"000 \nFXUS61 KBOX 022101\nAFDBOX\nBody\n")
                .expect("write should succeed");
            writer.finish().expect("finish should succeed").into_inner()
        };
        let delivered = post_process_archive(true, "AFDBOX.ZIP", &archive)
            .expect("archive post-processing should succeed");
        let metadata =
            build_completed_file_metadata(&delivered.filename, 1704070800, &delivered.data);
        let tmp = tempfile::tempdir().expect("tempdir should exist");

        let record =
            persist_completed_record(tmp.path(), &delivered.filename, &delivered.data, metadata)
                .expect("completed record should persist");

        assert_eq!(
            std::fs::read(&record.path).expect("payload should be readable"),
            b"000 \nFXUS61 KBOX 022101\nAFDBOX\nBody\n"
        );
        assert_eq!(
            PathBuf::from(&record.path),
            tmp.path().join("nested/AFDBOX.TXT")
        );
        assert_eq!(
            PathBuf::from(&record.metadata_path),
            tmp.path().join("nested/AFDBOX.JSON")
        );
    }
}
