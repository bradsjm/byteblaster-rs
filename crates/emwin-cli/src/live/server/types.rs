use crate::cmd::event_output::{frame_event_name, frame_event_to_json};
use crate::live::file_pipeline::CompletedFileMetadata;
use crate::live::server_support::{RetainedFiles, file_download_url, wildcard_match};
use emwin_parser::{ProductBody, TextProductHeader, UgcSection, VtecCode};
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
                    office: csv_values(query.office.as_deref(), normalize_upper),
                    office_city: csv_values(query.office_city.as_deref(), normalize_lower),
                    office_state: csv_values(query.office_state.as_deref(), normalize_upper),
                },
                header: HeaderFilter {
                    cccc: csv_values(query.cccc.as_deref(), normalize_upper),
                    ttaaii: csv_values(query.ttaaii.as_deref(), normalize_upper),
                    afos: csv_values(query.afos.as_deref(), normalize_upper),
                    bbb: csv_values(query.bbb.as_deref(), normalize_upper),
                },
                geo: GeoFilter {
                    states: csv_values(query.state.as_deref(), normalize_upper),
                    counties: csv_values(query.county.as_deref(), normalize_upper),
                    zones: csv_values(query.zone.as_deref(), normalize_upper),
                    fire_zones: csv_values(query.fire_zone.as_deref(), normalize_upper),
                    marine_zones: csv_values(query.marine_zone.as_deref(), normalize_upper),
                },
                vtec: VtecFilter {
                    phenomena: csv_values(query.vtec_phenomena.as_deref(), normalize_upper),
                    significance: csv_values(query.vtec_significance.as_deref(), normalize_upper),
                    action: csv_values(query.vtec_action.as_deref(), normalize_upper),
                    office: csv_values(query.vtec_office.as_deref(), normalize_upper),
                    etn: csv_numbers(query.etn.as_deref()),
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
    pub(crate) geo: GeoFilter,
    pub(crate) vtec: VtecFilter,
}

impl FileEventFilter {
    fn has_constraints(&self) -> bool {
        self.filename_pattern.is_some()
            || self.size.has_constraints()
            || self.product.has_constraints()
            || self.header.has_constraints()
            || self.geo.has_constraints()
            || self.vtec.has_constraints()
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

        if !self.header.matches(metadata.product.header.as_ref()) {
            return false;
        }

        if !self.geo.matches(metadata.product.body.as_ref()) {
            return false;
        }

        self.vtec.matches(metadata.product.body.as_ref())
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
    pub(crate) office: Option<BTreeSet<String>>,
    pub(crate) office_city: Option<BTreeSet<String>>,
    pub(crate) office_state: Option<BTreeSet<String>>,
}

impl ProductFilter {
    fn has_constraints(&self) -> bool {
        self.pil.is_some()
            || self.family.is_some()
            || self.container.is_some()
            || self.wmo_prefix.is_some()
            || self.office.is_some()
            || self.office_city.is_some()
            || self.office_state.is_some()
    }

    fn matches(&self, product: &emwin_parser::ProductEnrichment) -> bool {
        matches_option_set(&self.pil, product.pil.as_deref(), normalize_upper)
            && matches_option_set(&self.family, product.family, normalize_lower)
            && matches_option_set(&self.container, Some(product.container), normalize_lower)
            && matches_option_set(&self.wmo_prefix, product.wmo_prefix, normalize_upper)
            && matches_option_set(
                &self.office,
                product.office.as_ref().map(|office| office.code),
                normalize_upper,
            )
            && matches_option_set(
                &self.office_city,
                product.office.as_ref().map(|office| office.city),
                normalize_lower,
            )
            && matches_option_set(
                &self.office_state,
                product.office.as_ref().map(|office| office.state),
                normalize_upper,
            )
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

    fn matches(&self, header: Option<&TextProductHeader>) -> bool {
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct GeoFilter {
    pub(crate) states: Option<BTreeSet<String>>,
    pub(crate) counties: Option<BTreeSet<String>>,
    pub(crate) zones: Option<BTreeSet<String>>,
    pub(crate) fire_zones: Option<BTreeSet<String>>,
    pub(crate) marine_zones: Option<BTreeSet<String>>,
}

impl GeoFilter {
    fn has_constraints(&self) -> bool {
        self.states.is_some()
            || self.counties.is_some()
            || self.zones.is_some()
            || self.fire_zones.is_some()
            || self.marine_zones.is_some()
    }

    fn matches(&self, body: Option<&ProductBody>) -> bool {
        if !self.has_constraints() {
            return true;
        }

        let Some(body) = body else {
            return false;
        };
        let Some(sections) = body.ugc.as_deref() else {
            return false;
        };

        matches_geo_states(&self.states, sections)
            && matches_enriched_ugc_codes(
                &self.counties,
                sections,
                |section| &section.counties,
                'C',
            )
            && matches_enriched_ugc_codes(&self.zones, sections, |section| &section.zones, 'Z')
            && matches_enriched_ugc_codes(
                &self.fire_zones,
                sections,
                |section| &section.fire_zones,
                'F',
            )
            && matches_enriched_ugc_codes(
                &self.marine_zones,
                sections,
                |section| &section.marine_zones,
                'Z',
            )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct VtecFilter {
    pub(crate) phenomena: Option<BTreeSet<String>>,
    pub(crate) significance: Option<BTreeSet<String>>,
    pub(crate) action: Option<BTreeSet<String>>,
    pub(crate) office: Option<BTreeSet<String>>,
    pub(crate) etn: Option<BTreeSet<u32>>,
}

impl VtecFilter {
    fn has_constraints(&self) -> bool {
        self.phenomena.is_some()
            || self.significance.is_some()
            || self.action.is_some()
            || self.office.is_some()
            || self.etn.is_some()
    }

    fn matches(&self, body: Option<&ProductBody>) -> bool {
        if !self.has_constraints() {
            return true;
        }

        let Some(body) = body else {
            return false;
        };
        let Some(vtec_codes) = body.vtec.as_deref() else {
            return false;
        };

        vtec_codes.iter().any(|code| self.matches_code(code))
    }

    fn matches_code(&self, code: &VtecCode) -> bool {
        matches_option_set(
            &self.phenomena,
            Some(code.phenomena.as_str()),
            normalize_upper,
        ) && matches_char_set(&self.significance, code.significance)
            && matches_option_set(&self.action, Some(code.action.as_str()), normalize_upper)
            && matches_option_set(&self.office, Some(code.office.as_str()), normalize_upper)
            && matches_number_set(&self.etn, code.etn)
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

fn matches_char_set(allowed: &Option<BTreeSet<String>>, value: char) -> bool {
    match allowed {
        Some(allowed) => allowed.contains(&value.to_ascii_uppercase().to_string()),
        None => true,
    }
}

fn matches_number_set(allowed: &Option<BTreeSet<u32>>, value: u32) -> bool {
    match allowed {
        Some(allowed) => allowed.contains(&value),
        None => true,
    }
}

fn matches_geo_states(allowed: &Option<BTreeSet<String>>, sections: &[UgcSection]) -> bool {
    match allowed {
        Some(allowed) => sections.iter().any(|section| {
            section.counties.keys().any(|state| allowed.contains(state))
                || section.zones.keys().any(|state| allowed.contains(state))
                || section
                    .fire_zones
                    .keys()
                    .any(|state| allowed.contains(state))
                || section
                    .marine_zones
                    .keys()
                    .any(|state| allowed.contains(state))
        }),
        None => true,
    }
}

fn matches_enriched_ugc_codes(
    allowed: &Option<BTreeSet<String>>,
    sections: &[UgcSection],
    select: fn(&UgcSection) -> &std::collections::BTreeMap<String, Vec<emwin_parser::UgcArea>>,
    class_code: char,
) -> bool {
    match allowed {
        Some(allowed) => sections.iter().any(|section| {
            select(section).iter().any(|(state, areas)| {
                areas
                    .iter()
                    .any(|area| allowed.contains(&format!("{state}{class_code}{:03}", area.id)))
            })
        }),
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

fn csv_numbers(raw: Option<&str>) -> Option<BTreeSet<u32>> {
    let values = raw
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter_map(|value| value.parse::<u32>().ok())
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
    pub(crate) office: Option<String>,
    pub(crate) office_city: Option<String>,
    pub(crate) office_state: Option<String>,
    pub(crate) cccc: Option<String>,
    pub(crate) ttaaii: Option<String>,
    pub(crate) afos: Option<String>,
    pub(crate) bbb: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) county: Option<String>,
    pub(crate) zone: Option<String>,
    pub(crate) fire_zone: Option<String>,
    pub(crate) marine_zone: Option<String>,
    pub(crate) vtec_phenomena: Option<String>,
    pub(crate) vtec_significance: Option<String>,
    pub(crate) vtec_action: Option<String>,
    pub(crate) vtec_office: Option<String>,
    pub(crate) etn: Option<String>,
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
