//! Common utilities for live stream command implementation.
//!
//! This module provides shared logging and statistics tracking utilities
//! used by the stream command for consistent output formatting.

use crate::cmd::event_output::text_preview;
use crate::live::file_pipeline::CompletedFileRecord;
use crate::live::shared::unix_seconds;
use emwin_parser::enrich_product;
use emwin_protocol::ingest::{IngestWarning, ProductOrigin, ReceivedProduct};
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
        product_source = ?metadata.product.source,
        product_afos = metadata.product.header.as_ref().map(|value| value.afos.as_str()),
        product_ttaaii = metadata.product.header.as_ref().map(|value| value.ttaaii.as_str()),
        product_pil = metadata.product.pil.as_deref(),
        product_issue_code = metadata.product.issues.first().map(|value| value.code),
        "wrote file"
    );
}

pub(super) fn log_product_event(product: &ReceivedProduct, text_preview_chars: usize) {
    let meta = enrich_product(&product.filename, &product.data);
    let preview = text_preview(&product.filename, &product.data, text_preview_chars);
    match &product.origin {
        ProductOrigin::Qbt => {
            if let Some(title) = meta.title {
                info!(
                    event = "product",
                    source = "qbt",
                    filename = %product.filename,
                    bytes = product.data.len(),
                    timestamp_utc = unix_seconds(product.source_timestamp_utc),
                    product_source = ?meta.source,
                    product_title = title,
                    product_pil = meta.pil.as_deref(),
                    product_issue_code = meta.issues.first().map(|value| value.code),
                    preview = preview.as_deref(),
                    "ingest event"
                );
            } else {
                info!(
                    event = "product",
                    source = "qbt",
                    filename = %product.filename,
                    bytes = product.data.len(),
                    timestamp_utc = unix_seconds(product.source_timestamp_utc),
                    product_source = ?meta.source,
                    product_pil = meta.pil.as_deref(),
                    product_issue_code = meta.issues.first().map(|value| value.code),
                    preview = preview.as_deref(),
                    "ingest event"
                );
            }
        }
        ProductOrigin::WxWire {
            message_id,
            subject,
            delay_stamp_utc,
        } => {
            if let Some(title) = meta.title {
                info!(
                    event = "product",
                    source = "wxwire",
                    filename = %product.filename,
                    bytes = product.data.len(),
                    timestamp_utc = unix_seconds(product.source_timestamp_utc),
                    message_id = %message_id,
                    subject = %subject,
                    delay_stamp_utc = delay_stamp_utc.map(unix_seconds),
                    product_source = ?meta.source,
                    product_title = title,
                    product_pil = meta.pil.as_deref(),
                    product_issue_code = meta.issues.first().map(|value| value.code),
                    preview = preview.as_deref(),
                    "ingest event"
                );
            } else {
                info!(
                    event = "product",
                    source = "wxwire",
                    filename = %product.filename,
                    bytes = product.data.len(),
                    timestamp_utc = unix_seconds(product.source_timestamp_utc),
                    message_id = %message_id,
                    subject = %subject,
                    delay_stamp_utc = delay_stamp_utc.map(unix_seconds),
                    product_source = ?meta.source,
                    product_pil = meta.pil.as_deref(),
                    product_issue_code = meta.issues.first().map(|value| value.code),
                    preview = preview.as_deref(),
                    "ingest event"
                );
            }
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
