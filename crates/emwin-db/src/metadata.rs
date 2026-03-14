use emwin_parser::{
    ProductDetailV2, ProductEnrichment, ProductSummaryV2, detail_product_v2, enrich_product,
    summarize_product_v2,
};
use emwin_protocol::ingest::ProductOrigin;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

/// Shared completed-file metadata used by the CLI and persistence sinks.
///
/// This keeps the live server payload shape stable while adding the upstream origin needed by
/// database sinks.
#[derive(Debug, Clone, PartialEq)]
pub struct CompletedFileMetadata {
    pub filename: String,
    pub size: usize,
    pub timestamp_utc: u64,
    pub origin: ProductOrigin,
    pub product: ProductEnrichment,
    pub product_summary: ProductSummaryV2,
    pub product_detail: ProductDetailV2,
}

impl CompletedFileMetadata {
    /// Builds the canonical metadata bundle from one delivered product payload.
    pub fn build(filename: &str, timestamp_utc: u64, origin: ProductOrigin, data: &[u8]) -> Self {
        let product = enrich_product(filename, data);
        Self {
            filename: filename.to_string(),
            size: data.len(),
            timestamp_utc,
            origin,
            product_summary: summarize_product_v2(&product),
            product_detail: detail_product_v2(&product),
            product,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::CompletedFileMetadata;
    use emwin_protocol::ingest::ProductOrigin;

    #[test]
    fn serialization_preserves_existing_sidecar_shape() {
        let metadata = CompletedFileMetadata::build(
            "AFDBOX.TXT",
            1,
            ProductOrigin::Qbt,
            b"000 \nFXUS61 KBOX 022101\nAFDBOX\nBody\n",
        );

        let value = serde_json::to_value(&metadata).expect("metadata should serialize");
        assert_eq!(value["filename"], "AFDBOX.TXT");
        assert_eq!(value["size"], metadata.size);
        assert_eq!(value["timestamp_utc"], 1);
        assert_eq!(value["product"]["schema_version"], 2);
        assert!(value.get("origin").is_none());
        assert!(value.get("product_summary").is_none());
    }
}
