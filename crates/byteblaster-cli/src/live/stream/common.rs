use crate::cmd::event_output::text_preview;
use crate::live::file_pipeline::CompletedFileRecord;
use crate::live::shared::unix_seconds;
use crate::product_meta::detect_product_meta;
use byteblaster_core::ingest::{IngestWarning, ProductOrigin, ReceivedProduct};
use tracing::{info, warn};

#[derive(Debug, Default)]
pub(super) struct LiveStats {
    pub(super) connections_total: u64,
    pub(super) disconnects_total: u64,
    pub(super) products_total: u64,
}

pub(super) fn log_completed_file(completed: &CompletedFileRecord) {
    let metadata = &completed.metadata;
    info!(
        path = %completed.path,
        filename = %metadata.filename,
        timestamp_utc = metadata.timestamp_utc,
        text_product_afos = metadata.text_product_header.as_ref().map(|value| value.afos.as_str()),
        text_product_ttaaii = metadata.text_product_header.as_ref().map(|value| value.ttaaii.as_str()),
        text_product_pil_nnn = metadata.text_product_enrichment.as_ref().and_then(|value| value.pil_nnn.as_deref()),
        text_product_warning_code = metadata.text_product_warning.as_ref().map(|value| value.code),
        "wrote file"
    );
}

pub(super) fn log_product_event(product: &ReceivedProduct, text_preview_chars: usize) {
    let meta = detect_product_meta(&product.filename);
    let preview = text_preview(&product.filename, &product.data, text_preview_chars);
    match &product.origin {
        ProductOrigin::Qbt => {
            info!(
                event = "product",
                source = "qbt",
                filename = %product.filename,
                bytes = product.data.len(),
                timestamp_utc = unix_seconds(product.source_timestamp_utc),
                product_title = meta
                    .as_ref()
                    .map(|value| value.title.as_str())
                    .unwrap_or(""),
                preview = preview.as_deref(),
                "ingest event"
            );
        }
        ProductOrigin::WxWire {
            message_id,
            subject,
            delay_stamp_utc,
        } => {
            info!(
                event = "product",
                source = "wxwire",
                filename = %product.filename,
                bytes = product.data.len(),
                timestamp_utc = unix_seconds(product.source_timestamp_utc),
                message_id = %message_id,
                subject = %subject,
                delay_stamp_utc = delay_stamp_utc.map(unix_seconds),
                product_title = meta
                    .as_ref()
                    .map(|value| value.title.as_str())
                    .unwrap_or(""),
                preview = preview.as_deref(),
                "ingest event"
            );
        }
        _ => {
            info!(
                event = "product",
                source = "unknown",
                filename = %product.filename,
                bytes = product.data.len(),
                timestamp_utc = unix_seconds(product.source_timestamp_utc),
                "ingest event"
            );
        }
    }
}

pub(super) fn log_ingest_warning(warning: &IngestWarning) {
    match warning {
        IngestWarning::Qbt(value) => {
            warn!(event = "warning", source = "qbt", warning = ?value, "ingest warning");
        }
        IngestWarning::WxWire(value) => {
            warn!(event = "warning", source = "wxwire", warning = ?value, "ingest warning");
        }
        _ => {
            warn!(event = "warning", source = "unknown", warning = ?warning, "ingest warning");
        }
    }
}
