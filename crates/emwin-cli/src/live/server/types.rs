use crate::cmd::event_output::{frame_event_name, frame_event_to_json};
use crate::live::file_pipeline::CompletedFileMetadata;
use crate::live::server_support::{RetainedFiles, file_download_url, wildcard_match};
use emwin_protocol::qbt_receiver::{QbtFrameEvent, QbtReceiverTelemetrySnapshot};
use emwin_protocol::wxwire_receiver::{WxWireReceiverFrameEvent, WxWireReceiverTelemetrySnapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
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

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CompletedFilePayload {
    #[serde(flatten)]
    pub(crate) metadata: CompletedFileMetadata,
    pub(crate) download_url: String,
}

impl CompletedFilePayload {
    pub(crate) fn from_metadata(metadata: CompletedFileMetadata) -> Self {
        let download_url = file_download_url(&metadata.filename);
        Self {
            metadata,
            download_url,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "receiver", rename_all = "snake_case")]
pub(crate) enum TelemetryPayload {
    Unavailable,
    Qbt(QbtReceiverTelemetrySnapshot),
    WxWire(WxWireReceiverTelemetrySnapshot),
}

#[derive(Debug, Clone)]
pub(crate) enum EventKind {
    Connected { endpoint: String },
    Disconnected,
    QbtFrame(QbtFrameEvent),
    WxWireFrame(WxWireReceiverFrameEvent),
    FileComplete(Box<CompletedFilePayload>),
    Telemetry(TelemetryPayload),
    Error { message: String },
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
            Self::FileComplete(_) => "file_complete",
            Self::Telemetry(_) => "telemetry",
            Self::Error { .. } => "error",
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
            Self::FileComplete(file) => {
                serde_json::to_value(file).unwrap_or_else(|_| serde_json::json!({}))
            }
            Self::Telemetry(snapshot) => {
                serde_json::to_value(snapshot).unwrap_or_else(|_| serde_json::json!({}))
            }
            Self::Error { message } => serde_json::json!({ "message": message }),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EventFilter {
    pub(crate) event_names: Option<BTreeSet<String>>,
    pub(crate) file: FileEventFilter,
}

impl EventFilter {
    pub(crate) fn from_query(query: EventsQuery) -> Self {
        Self {
            event_names: csv_values(query.event.as_deref(), normalize_lower),
            file: FileEventFilter {
                filename_pattern: query.filename,
                size: SizeRange {
                    min: query.min_size,
                    max: query.max_size,
                },
                product: ProductFilter {
                    pil: csv_values(query.pil.as_deref(), normalize_upper),
                    family: csv_values(query.family.as_deref(), normalize_lower),
                    container: csv_values(query.container.as_deref(), normalize_lower),
                    wmo_prefix: csv_values(query.wmo_prefix.as_deref(), normalize_upper),
                },
                header: HeaderFilter {
                    cccc: csv_values(query.cccc.as_deref(), normalize_upper),
                    ttaaii: csv_values(query.ttaaii.as_deref(), normalize_upper),
                    afos: csv_values(query.afos.as_deref(), normalize_upper),
                    bbb: csv_values(query.bbb.as_deref(), normalize_upper),
                },
            },
        }
    }

    pub(crate) fn matches(&self, event: &EventKind) -> bool {
        if let Some(event_names) = &self.event_names {
            let event_name = normalize_lower(event.event_name());
            if !event_names.contains(&event_name) {
                return false;
            }
        }

        if !self.file.has_constraints() {
            return true;
        }

        match event {
            EventKind::FileComplete(file) => self.file.matches_metadata(&file.metadata),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FileEventFilter {
    pub(crate) filename_pattern: Option<String>,
    pub(crate) size: SizeRange,
    pub(crate) product: ProductFilter,
    pub(crate) header: HeaderFilter,
}

impl FileEventFilter {
    fn has_constraints(&self) -> bool {
        self.filename_pattern.is_some()
            || self.size.has_constraints()
            || self.product.has_constraints()
            || self.header.has_constraints()
    }

    fn matches_metadata(&self, metadata: &CompletedFileMetadata) -> bool {
        if let Some(pattern) = self.filename_pattern.as_deref()
            && !wildcard_match(pattern, &metadata.filename)
        {
            return false;
        }

        if !self.size.matches(metadata.size) {
            return false;
        }

        if !self.product.matches(&metadata.product) {
            return false;
        }

        self.header.matches(metadata.product.header.as_ref())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SizeRange {
    pub(crate) min: Option<usize>,
    pub(crate) max: Option<usize>,
}

impl SizeRange {
    fn has_constraints(&self) -> bool {
        self.min.is_some() || self.max.is_some()
    }

    fn matches(&self, size: usize) -> bool {
        if let Some(min) = self.min
            && size < min
        {
            return false;
        }

        if let Some(max) = self.max
            && size > max
        {
            return false;
        }

        true
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProductFilter {
    pub(crate) pil: Option<BTreeSet<String>>,
    pub(crate) family: Option<BTreeSet<String>>,
    pub(crate) container: Option<BTreeSet<String>>,
    pub(crate) wmo_prefix: Option<BTreeSet<String>>,
}

impl ProductFilter {
    fn has_constraints(&self) -> bool {
        self.pil.is_some()
            || self.family.is_some()
            || self.container.is_some()
            || self.wmo_prefix.is_some()
    }

    fn matches(&self, product: &emwin_parser::ProductEnrichment) -> bool {
        matches_option_set(&self.pil, product.pil.as_deref(), normalize_upper)
            && matches_option_set(&self.family, product.family, normalize_lower)
            && matches_option_set(&self.container, Some(product.container), normalize_lower)
            && matches_option_set(&self.wmo_prefix, product.wmo_prefix, normalize_upper)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HeaderFilter {
    pub(crate) cccc: Option<BTreeSet<String>>,
    pub(crate) ttaaii: Option<BTreeSet<String>>,
    pub(crate) afos: Option<BTreeSet<String>>,
    pub(crate) bbb: Option<BTreeSet<String>>,
}

impl HeaderFilter {
    fn has_constraints(&self) -> bool {
        self.cccc.is_some() || self.ttaaii.is_some() || self.afos.is_some() || self.bbb.is_some()
    }

    fn matches(&self, header: Option<&emwin_parser::TextProductHeader>) -> bool {
        if !self.has_constraints() {
            return true;
        }

        let Some(header) = header else {
            return false;
        };

        matches_option_set(&self.cccc, Some(header.cccc.as_str()), normalize_upper)
            && matches_option_set(&self.ttaaii, Some(header.ttaaii.as_str()), normalize_upper)
            && matches_option_set(&self.afos, Some(header.afos.as_str()), normalize_upper)
            && matches_option_set(&self.bbb, header.bbb.as_deref(), normalize_upper)
    }
}

fn matches_option_set(
    allowed: &Option<BTreeSet<String>>,
    value: Option<&str>,
    normalize: fn(&str) -> String,
) -> bool {
    match allowed {
        Some(allowed) => value
            .map(normalize)
            .map(|normalized| allowed.contains(&normalized))
            .unwrap_or(false),
        None => true,
    }
}

fn csv_values(raw: Option<&str>, normalize: fn(&str) -> String) -> Option<BTreeSet<String>> {
    let values = raw
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize)
        .collect::<BTreeSet<_>>();

    (!values.is_empty()).then_some(values)
}

fn normalize_upper(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

fn normalize_lower(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[derive(Debug)]
pub(crate) struct AppState {
    pub(crate) event_tx: broadcast::Sender<BroadcastEvent>,
    pub(crate) shutdown_rx: watch::Receiver<bool>,
    pub(crate) retained_files: Mutex<RetainedFiles>,
    pub(crate) telemetry: Mutex<TelemetryPayload>,
    pub(crate) connected_clients: AtomicUsize,
    pub(crate) max_clients: usize,
    pub(crate) next_event_id: AtomicU64,
    pub(crate) data_blocks_total: AtomicU64,
    pub(crate) received_servers: AtomicUsize,
    pub(crate) received_sat_servers: AtomicUsize,
    pub(crate) started_at: Instant,
    pub(crate) upstream_endpoint: Mutex<Option<String>>,
    pub(crate) quiet: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventsQuery {
    pub(crate) event: Option<String>,
    pub(crate) filename: Option<String>,
    pub(crate) pil: Option<String>,
    pub(crate) family: Option<String>,
    pub(crate) container: Option<String>,
    pub(crate) wmo_prefix: Option<String>,
    pub(crate) cccc: Option<String>,
    pub(crate) ttaaii: Option<String>,
    pub(crate) afos: Option<String>,
    pub(crate) bbb: Option<String>,
    pub(crate) min_size: Option<usize>,
    pub(crate) max_size: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FilesResponse {
    pub(crate) files: Vec<CompletedFilePayload>,
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
