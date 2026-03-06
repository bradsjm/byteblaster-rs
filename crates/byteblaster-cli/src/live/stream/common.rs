use crate::cmd::event_output::text_preview;
use crate::live::file_pipeline::CompletedFileRecord;
use crate::live::shared::unix_seconds;
use crate::product_meta::detect_product_meta;
use byteblaster_core::ingest::{IngestWarning, ProductOrigin, ReceivedProduct};
use byteblaster_core::qbt_receiver::QbtFrameEvent;
use tracing::{info, warn};

#[derive(Debug, Default)]
pub(super) struct LiveStats {
    pub(super) connections_total: u64,
    pub(super) disconnects_total: u64,
    pub(super) products_total: u64,
}

pub(super) fn log_frame_event(frame: &QbtFrameEvent, text_preview_chars: usize) {
    match frame {
        QbtFrameEvent::DataBlock(segment) => {
            let product = detect_product_meta(&segment.filename);
            let preview = text_preview(&segment.filename, &segment.content, text_preview_chars);
            info!(
                event = "data_block",
                filename = %segment.filename,
                block_number = segment.block_number,
                total_blocks = segment.total_blocks,
                bytes = segment.content.len(),
                timestamp_utc = unix_seconds(segment.timestamp_utc),
                product_title = product
                    .as_ref()
                    .map(|meta| meta.title.as_str())
                    .unwrap_or(""),
                preview = preview.as_deref(),
                "frame event"
            );
        }
        QbtFrameEvent::ServerListUpdate(list) => {
            info!(
                event = "server_list",
                servers = list.servers.len(),
                sat_servers = list.sat_servers.len(),
                "frame event"
            );
        }
        QbtFrameEvent::Warning(warning) => {
            warn!(
                event = "warning",
                warning = ?warning,
                "frame warning"
            );
        }
        other => {
            info!(
                event = "other",
                frame = ?other,
                "frame event"
            );
        }
    }
}

pub(super) fn log_completed_file(completed: &CompletedFileRecord) {
    let header = completed.event.get("text_product_header");
    let enrichment = completed.event.get("text_product_enrichment");
    let warning = completed.event.get("text_product_warning");
    info!(
        path = %completed.path,
        filename = %completed.filename,
        timestamp_utc = completed.timestamp_utc,
        text_product_afos = header.and_then(|value| value.get("afos")).and_then(|value| value.as_str()),
        text_product_ttaaii = header.and_then(|value| value.get("ttaaii")).and_then(|value| value.as_str()),
        text_product_pil_nnn = enrichment.and_then(|value| value.get("pil_nnn")).and_then(|value| value.as_str()),
        text_product_warning_code = warning.and_then(|value| value.get("code")).and_then(|value| value.as_str()),
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
