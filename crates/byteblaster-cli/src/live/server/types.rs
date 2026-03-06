use crate::cmd::event_output::{frame_event_filename, frame_event_name, frame_event_to_json};
use crate::live::server_support::{RetainedFileMeta, RetainedFiles, file_download_url};
use byteblaster_core::qbt_receiver::QbtFrameEvent;
use byteblaster_core::wxwire_receiver::model::WxWireReceiverFrameEvent;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;
use tokio::sync::{broadcast, watch};

#[derive(Debug, Clone)]
pub(crate) struct BroadcastEvent {
    pub(crate) id: u64,
    pub(crate) kind: EventKind,
}

#[derive(Debug, Clone)]
pub(crate) enum EventKind {
    Connected {
        endpoint: String,
    },
    Disconnected,
    QbtFrame(QbtFrameEvent),
    WxWireFrame(WxWireReceiverFrameEvent),
    FileComplete {
        filename: String,
        size: usize,
        timestamp_utc: u64,
        product: Option<crate::product_meta::ProductMeta>,
        text_product_header: Option<serde_json::Value>,
        text_product_enrichment: Option<serde_json::Value>,
        text_product_warning: Option<serde_json::Value>,
    },
    Telemetry(serde_json::Value),
    Error {
        message: String,
    },
}

impl EventKind {
    pub(crate) fn event_name(&self) -> &'static str {
        match self {
            Self::Connected { .. } => "connected",
            Self::Disconnected => "disconnected",
            Self::QbtFrame(frame) => frame_event_name(frame),
            Self::WxWireFrame(frame) => match frame {
                WxWireReceiverFrameEvent::File(_) => "file",
                WxWireReceiverFrameEvent::Warning(_) => "warning",
                _ => "unknown",
            },
            Self::FileComplete { .. } => "file_complete",
            Self::Telemetry(_) => "telemetry",
            Self::Error { .. } => "error",
        }
    }

    pub(crate) fn filename(&self) -> Option<&str> {
        match self {
            Self::QbtFrame(frame) => frame_event_filename(frame),
            Self::WxWireFrame(frame) => match frame {
                WxWireReceiverFrameEvent::File(file) => Some(file.filename.as_str()),
                WxWireReceiverFrameEvent::Warning(_) => None,
                _ => None,
            },
            Self::FileComplete { filename, .. } => Some(filename.as_str()),
            _ => None,
        }
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Connected { endpoint } => serde_json::json!({ "endpoint": endpoint }),
            Self::Disconnected => serde_json::json!({}),
            Self::QbtFrame(frame) => frame_event_to_json(frame, 0),
            Self::WxWireFrame(frame) => match frame {
                WxWireReceiverFrameEvent::File(file) => serde_json::json!({
                    "type": "file",
                    "filename": file.filename,
                    "length": file.data.len(),
                    "subject": file.subject,
                    "id": file.id,
                    "issue_utc": crate::live::shared::unix_seconds(file.issue_utc),
                    "ttaaii": file.ttaaii,
                    "cccc": file.cccc,
                    "awipsid": file.awipsid,
                }),
                WxWireReceiverFrameEvent::Warning(warning) => serde_json::json!({
                    "type": "warning",
                    "warning": format!("{warning:?}"),
                }),
                _ => serde_json::json!({
                    "type": "unknown",
                }),
            },
            Self::FileComplete {
                filename,
                size,
                timestamp_utc,
                product,
                text_product_header,
                text_product_enrichment,
                text_product_warning,
            } => {
                let mut payload = serde_json::json!({
                    "filename": filename,
                    "size": size,
                    "timestamp_utc": timestamp_utc,
                    "download_url": file_download_url(filename),
                });
                if let Some(product) = product
                    && let Ok(product_json) = serde_json::to_value(product)
                {
                    payload["product"] = product_json;
                }
                if let Some(text_product_header) = text_product_header {
                    payload["text_product_header"] = text_product_header.clone();
                }
                if let Some(text_product_enrichment) = text_product_enrichment {
                    payload["text_product_enrichment"] = text_product_enrichment.clone();
                }
                if let Some(text_product_warning) = text_product_warning {
                    payload["text_product_warning"] = text_product_warning.clone();
                }
                payload
            }
            Self::Telemetry(snapshot) => snapshot.clone(),
            Self::Error { message } => serde_json::json!({ "message": message }),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AppState {
    pub(crate) event_tx: broadcast::Sender<BroadcastEvent>,
    pub(crate) shutdown_rx: watch::Receiver<bool>,
    pub(crate) retained_files: Mutex<RetainedFiles>,
    pub(crate) telemetry: Mutex<serde_json::Value>,
    pub(crate) connected_clients: AtomicUsize,
    pub(crate) max_clients: usize,
    pub(crate) next_event_id: AtomicU64,
    pub(crate) data_blocks_total: AtomicU64,
    pub(crate) current_servers: AtomicUsize,
    pub(crate) current_sat_servers: AtomicUsize,
    pub(crate) started_at: Instant,
    pub(crate) upstream_endpoint: Mutex<Option<String>>,
    pub(crate) quiet: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventsQuery {
    pub(crate) filter: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FilesResponse {
    pub(crate) files: Vec<RetainedFileMeta>,
}

#[derive(Debug, Serialize)]
pub(crate) struct HealthResponse {
    pub(crate) status: &'static str,
    pub(crate) connected_clients: usize,
    pub(crate) retained_files: usize,
    pub(crate) uptime_secs: u64,
    pub(crate) upstream_endpoint: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct EndpointDoc {
    pub(crate) method: &'static str,
    pub(crate) path: &'static str,
    pub(crate) description: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct RootResponse {
    pub(crate) service: &'static str,
    pub(crate) endpoints: Vec<EndpointDoc>,
}

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub receiver: crate::ReceiverKind,
    pub username: String,
    pub password: Option<String>,
    pub raw_servers: Vec<String>,
    pub server_list_path: Option<String>,
    pub bind: String,
    pub cors_origin: Option<String>,
    pub max_clients: usize,
    pub stats_interval_secs: u64,
    pub file_retention_secs: u64,
    pub max_retained_files: usize,
    pub quiet: bool,
}

pub(crate) struct ClientGuard {
    pub(crate) state: Arc<AppState>,
    pub(crate) peer: SocketAddr,
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.state.connected_clients.fetch_sub(1, Ordering::Relaxed);
        super::log_info(
            self.state.quiet,
            &format!("sse client disconnected peer={}", self.peer),
        );
    }
}
