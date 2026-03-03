use crate::output::{label_error, label_info, label_ok, label_stats, label_warn};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use byteblaster_core::{
    ByteBlasterClient, Client, ClientConfig, ClientEvent, ClientTelemetrySnapshot, DecodeConfig,
    FileAssembler, FrameEvent, SegmentAssembler, parse_server,
};
use futures::Stream;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch};
use tower_http::cors::{Any, CorsLayer};

const EVENT_CHANNEL_CAPACITY: usize = 4096;

#[derive(Debug, Clone)]
struct BroadcastEvent {
    id: u64,
    kind: EventKind,
}

#[derive(Debug, Clone)]
enum EventKind {
    Connected {
        endpoint: String,
    },
    Disconnected,
    DataBlock {
        filename: String,
        block_number: u32,
        total_blocks: u32,
        length: usize,
        version: String,
    },
    FileComplete {
        filename: String,
        size: usize,
    },
    ServerList {
        servers: Vec<(String, u16)>,
        sat_servers: Vec<(String, u16)>,
    },
    Telemetry(ClientTelemetrySnapshot),
    Warning {
        warning: String,
    },
    Error {
        message: String,
    },
}

impl EventKind {
    fn event_name(&self) -> &'static str {
        match self {
            Self::Connected { .. } => "connected",
            Self::Disconnected => "disconnected",
            Self::DataBlock { .. } => "data_block",
            Self::FileComplete { .. } => "file_complete",
            Self::ServerList { .. } => "server_list",
            Self::Telemetry(_) => "telemetry",
            Self::Warning { .. } => "warning",
            Self::Error { .. } => "error",
        }
    }

    fn filename(&self) -> Option<&str> {
        match self {
            Self::DataBlock { filename, .. } | Self::FileComplete { filename, .. } => {
                Some(filename.as_str())
            }
            _ => None,
        }
    }

    fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Connected { endpoint } => serde_json::json!({ "endpoint": endpoint }),
            Self::Disconnected => serde_json::json!({}),
            Self::DataBlock {
                filename,
                block_number,
                total_blocks,
                length,
                version,
            } => serde_json::json!({
                "filename": filename,
                "block_number": block_number,
                "total_blocks": total_blocks,
                "length": length,
                "version": version,
            }),
            Self::FileComplete { filename, size } => serde_json::json!({
                "filename": filename,
                "size": size,
            }),
            Self::ServerList {
                servers,
                sat_servers,
            } => serde_json::json!({
                "servers": servers,
                "sat_servers": sat_servers,
            }),
            Self::Telemetry(snapshot) => serde_json::json!(snapshot),
            Self::Warning { warning } => serde_json::json!({ "warning": warning }),
            Self::Error { message } => serde_json::json!({ "message": message }),
        }
    }
}

#[derive(Debug, Clone)]
struct RetainedFile {
    filename: String,
    data: Vec<u8>,
    completed_at: SystemTime,
}

impl RetainedFile {
    fn size(&self) -> usize {
        self.data.len()
    }
}

#[derive(Debug)]
struct RetainedFiles {
    by_name: HashMap<String, RetainedFile>,
    order: VecDeque<String>,
    max_entries: usize,
    ttl: Duration,
}

impl RetainedFiles {
    fn new(max_entries: usize, ttl: Duration) -> Self {
        Self {
            by_name: HashMap::new(),
            order: VecDeque::new(),
            max_entries: max_entries.max(1),
            ttl: ttl.max(Duration::from_secs(1)),
        }
    }

    fn insert(&mut self, filename: String, data: Vec<u8>, completed_at: SystemTime) {
        self.evict_expired();

        if self.by_name.contains_key(&filename) {
            self.order.retain(|name| name != &filename);
        }
        self.order.push_back(filename.clone());
        self.by_name.insert(
            filename.clone(),
            RetainedFile {
                filename,
                data,
                completed_at,
            },
        );

        while self.by_name.len() > self.max_entries {
            if let Some(oldest) = self.order.pop_front() {
                self.by_name.remove(&oldest);
            } else {
                break;
            }
        }
    }

    fn list(&mut self) -> Vec<RetainedFileMeta> {
        self.evict_expired();
        self.order
            .iter()
            .rev()
            .filter_map(|name| self.by_name.get(name))
            .map(|file| RetainedFileMeta {
                filename: file.filename.clone(),
                size: file.size(),
                completed_at_unix_secs: file
                    .completed_at
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            })
            .collect()
    }

    fn get(&mut self, filename: &str) -> Option<RetainedFile> {
        self.evict_expired();
        self.by_name.get(filename).cloned()
    }

    fn len(&mut self) -> usize {
        self.evict_expired();
        self.by_name.len()
    }

    fn evict_expired(&mut self) {
        let now = SystemTime::now();
        self.order.retain(|name| {
            let Some(file) = self.by_name.get(name) else {
                return false;
            };
            let age = now
                .duration_since(file.completed_at)
                .unwrap_or(Duration::from_secs(0));
            if age > self.ttl {
                self.by_name.remove(name);
                return false;
            }
            true
        });
    }
}

#[derive(Debug)]
struct AppState {
    event_tx: broadcast::Sender<BroadcastEvent>,
    retained_files: Mutex<RetainedFiles>,
    telemetry: Mutex<ClientTelemetrySnapshot>,
    connected_clients: AtomicUsize,
    max_clients: usize,
    next_event_id: AtomicU64,
    started_at: Instant,
    upstream_endpoint: Mutex<Option<String>>,
    quiet: bool,
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    filter: Option<String>,
}

#[derive(Debug, Serialize)]
struct RetainedFileMeta {
    filename: String,
    size: usize,
    completed_at_unix_secs: u64,
}

#[derive(Debug, Serialize)]
struct FilesResponse {
    files: Vec<RetainedFileMeta>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    connected_clients: usize,
    retained_files: usize,
    uptime_secs: u64,
    upstream_endpoint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub email: String,
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

pub async fn run(options: ServerOptions) -> anyhow::Result<()> {
    let servers = parse_servers_or_default(&options.raw_servers)?;
    let bind_addr = SocketAddr::from_str(&options.bind)
        .map_err(|err| anyhow::anyhow!("invalid --bind value {}: {err}", options.bind))?;

    let state = Arc::new(AppState {
        event_tx: broadcast::channel(EVENT_CHANNEL_CAPACITY).0,
        retained_files: Mutex::new(RetainedFiles::new(
            options.max_retained_files.max(1),
            Duration::from_secs(options.file_retention_secs.max(1)),
        )),
        telemetry: Mutex::new(ClientTelemetrySnapshot::default()),
        connected_clients: AtomicUsize::new(0),
        max_clients: options.max_clients.max(1),
        next_event_id: AtomicU64::new(1),
        started_at: Instant::now(),
        upstream_endpoint: Mutex::new(None),
        quiet: options.quiet,
    });

    let app = build_router(Arc::clone(&state), options.cors_origin)?;

    let listener = TcpListener::bind(bind_addr).await?;
    log_info(
        options.quiet,
        &format!("{} server listening addr={bind_addr}", label_ok()),
    );

    let config = ClientConfig {
        email: options.email,
        servers,
        server_list_path: options.server_list_path.map(PathBuf::from),
        reconnect_delay_secs: 5,
        connection_timeout_secs: 5,
        watchdog_timeout_secs: 20,
        max_exceptions: 10,
        decode: DecodeConfig::default(),
    };

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let ingest_task = tokio::spawn(run_ingest_loop(
        config,
        Arc::clone(&state),
        shutdown_rx.clone(),
    ));
    let stats_task = tokio::spawn(run_stats_loop(
        Arc::clone(&state),
        options.stats_interval_secs,
        shutdown_rx,
    ));

    let serve = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    });

    let serve_result = serve.await;
    let _ = shutdown_tx.send(true);

    let ingest_result = ingest_task.await;
    let stats_result = stats_task.await;

    if let Err(err) = serve_result {
        return Err(anyhow::anyhow!("http server failed: {err}"));
    }
    if let Err(err) = ingest_result {
        return Err(anyhow::anyhow!("ingest task join failed: {err}"));
    }
    if let Err(err) = stats_result {
        return Err(anyhow::anyhow!("stats task join failed: {err}"));
    }

    Ok(())
}

fn build_router(state: Arc<AppState>, cors_origin: Option<String>) -> anyhow::Result<Router> {
    Ok(Router::new()
        .route("/events", get(events_handler))
        .route("/files", get(files_handler))
        .route("/files/*filename", get(file_download_handler))
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .layer(build_cors_layer(cors_origin)?)
        .with_state(state))
}

async fn run_ingest_loop(
    config: ClientConfig,
    state: Arc<AppState>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut assembler = FileAssembler::new(100);
    let mut client = match Client::builder(config).build() {
        Ok(client) => client,
        Err(err) => {
            log_error(&format!("failed to build client: {err}"));
            return;
        }
    };

    if let Err(err) = client.start() {
        log_error(&format!("failed to start client: {err}"));
        return;
    }

    let mut events = client.events();
    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                break;
            }
            item = events.next() => {
                let Some(item) = item else {
                    break;
                };
                handle_client_event(item, &state, &mut assembler);
            }
        }
    }

    drop(events);
    if let Err(err) = client.stop().await {
        log_error(&format!("failed to stop client: {err}"));
    }
}

fn handle_client_event(
    item: Result<ClientEvent, byteblaster_core::CoreError>,
    state: &Arc<AppState>,
    assembler: &mut FileAssembler,
) {
    match item {
        Ok(ClientEvent::Connected(endpoint)) => {
            {
                let mut guard = state
                    .upstream_endpoint
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *guard = Some(endpoint.clone());
            }
            log_info(
                state.quiet,
                &format!("{} upstream connected endpoint={endpoint}", label_ok()),
            );
            publish(
                state,
                EventKind::Connected {
                    endpoint: endpoint.clone(),
                },
            );
        }
        Ok(ClientEvent::Disconnected) => {
            {
                let mut guard = state
                    .upstream_endpoint
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *guard = None;
            }
            log_info(
                state.quiet,
                &format!("{} upstream disconnected", label_warn()),
            );
            publish(state, EventKind::Disconnected);
        }
        Ok(ClientEvent::Telemetry(snapshot)) => {
            {
                let mut guard = state
                    .telemetry
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *guard = snapshot.clone();
            }
            publish(state, EventKind::Telemetry(snapshot));
        }
        Ok(ClientEvent::Frame(frame)) => match frame {
            FrameEvent::DataBlock(segment) => {
                let segment_for_event = segment.clone();
                publish(
                    state,
                    EventKind::DataBlock {
                        filename: segment_for_event.filename,
                        block_number: segment_for_event.block_number,
                        total_blocks: segment_for_event.total_blocks,
                        length: segment_for_event.content.len(),
                        version: format!("{:?}", segment_for_event.version),
                    },
                );

                if let Ok(Some(file)) = assembler.push(segment) {
                    let completed_at = SystemTime::now();
                    {
                        let mut guard = state
                            .retained_files
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        guard.insert(file.filename.clone(), file.data.to_vec(), completed_at);
                    }
                    log_info(
                        state.quiet,
                        &format!(
                            "{} file complete name={} bytes={}",
                            label_info(),
                            file.filename,
                            file.data.len()
                        ),
                    );
                    publish(
                        state,
                        EventKind::FileComplete {
                            filename: file.filename,
                            size: file.data.len(),
                        },
                    );
                }
            }
            FrameEvent::ServerListUpdate(list) => {
                publish(
                    state,
                    EventKind::ServerList {
                        servers: list.servers,
                        sat_servers: list.sat_servers,
                    },
                );
            }
            FrameEvent::Warning(warning) => {
                publish(
                    state,
                    EventKind::Warning {
                        warning: format!("{warning:?}"),
                    },
                );
            }
            _ => {}
        },
        Err(err) => {
            log_error(&format!("client error: {err}"));
            publish(
                state,
                EventKind::Error {
                    message: err.to_string(),
                },
            );
        }
        Ok(_) => {}
    }
}

async fn run_stats_loop(
    state: Arc<AppState>,
    stats_interval_secs: u64,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    if stats_interval_secs == 0 {
        let _ = shutdown_rx.changed().await;
        return;
    }

    let mut interval = tokio::time::interval(Duration::from_secs(stats_interval_secs.max(1)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                break;
            }
            _ = interval.tick() => {
                if state.quiet {
                    continue;
                }

                let telemetry = state
                    .telemetry
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone();
                let endpoint = state
                    .upstream_endpoint
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone();
                let clients = state.connected_clients.load(Ordering::Relaxed);
                let files = state
                    .retained_files
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .len();

                let uptime = state.started_at.elapsed().as_secs();
                eprintln!(
                    "{} uptime={}s bytes_in={} frames={} files={} clients={} upstream={}",
                    label_stats(),
                    uptime,
                    telemetry.bytes_in_total,
                    telemetry.frame_events_total,
                    files,
                    clients,
                    endpoint.unwrap_or_else(|| "disconnected".to_string())
                );
            }
        }
    }
}

async fn events_handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let current = state.connected_clients.load(Ordering::Relaxed);
    if current >= state.max_clients {
        log_info(
            state.quiet,
            &format!(
                "{} rejecting client; limit reached peer={peer}",
                label_warn()
            ),
        );
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    state.connected_clients.fetch_add(1, Ordering::Relaxed);
    log_info(
        state.quiet,
        &format!("{} sse client connected peer={peer}", label_info()),
    );

    let rx = state.event_tx.subscribe();
    let last_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let filter = query.filter;

    let stream = futures::stream::unfold(
        StreamState {
            state: Arc::clone(&state),
            rx: Some(rx),
            last_id,
            filter,
            peer,
            _guard: Some(ClientGuard {
                state: Arc::clone(&state),
                peer,
            }),
        },
        move |mut st| async move {
            let rx = st.rx.as_mut()?;
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if event.id <= st.last_id {
                            continue;
                        }
                        if !event_matches_filter(st.filter.as_deref(), &event.kind) {
                            continue;
                        }

                        st.last_id = event.id;
                        let payload = match serde_json::to_string(&event.kind.to_json()) {
                            Ok(payload) => payload,
                            Err(_) => "{}".to_string(),
                        };
                        let sse = Event::default()
                            .id(event.id.to_string())
                            .event(event.kind.event_name())
                            .data(payload);
                        return Some((Ok(sse), st));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(dropped)) => {
                        log_info(
                            st.state.quiet,
                            &format!(
                                "{} sse client lagged peer={} dropped={}",
                                label_warn(),
                                st.peer,
                                dropped
                            ),
                        );
                        let warning = Event::default().event("warning").data(
                            serde_json::json!({
                                "message": "client lagged; events dropped",
                                "dropped": dropped,
                                "peer": st.peer,
                            })
                            .to_string(),
                        );
                        return Some((Ok(warning), st));
                    }
                }
            }
        },
    );

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

struct StreamState {
    state: Arc<AppState>,
    rx: Option<broadcast::Receiver<BroadcastEvent>>,
    last_id: u64,
    filter: Option<String>,
    peer: SocketAddr,
    _guard: Option<ClientGuard>,
}

struct ClientGuard {
    state: Arc<AppState>,
    peer: SocketAddr,
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.state.connected_clients.fetch_sub(1, Ordering::Relaxed);
        log_info(
            self.state.quiet,
            &format!(
                "{} sse client disconnected peer={}",
                label_info(),
                self.peer
            ),
        );
    }
}

async fn files_handler(State(state): State<Arc<AppState>>) -> Json<FilesResponse> {
    let files = state
        .retained_files
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .list();
    Json(FilesResponse { files })
}

async fn file_download_handler(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<Response, StatusCode> {
    let normalized = sanitize_requested_filename(&filename).ok_or(StatusCode::BAD_REQUEST)?;

    let file = state
        .retained_files
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&normalized)
        .ok_or(StatusCode::NOT_FOUND)?;

    let content_type = content_type_for_filename(&normalized);
    let disposition = format!("attachment; filename=\"{}\"", file.filename);

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(CONTENT_DISPOSITION, value);
    }
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));

    Ok((headers, file.data).into_response())
}

async fn health_handler(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let connected_clients = state.connected_clients.load(Ordering::Relaxed);
    let retained_files = state
        .retained_files
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .len();
    let upstream_endpoint = state
        .upstream_endpoint
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();

    Json(HealthResponse {
        status: "ok",
        connected_clients,
        retained_files,
        uptime_secs: state.started_at.elapsed().as_secs(),
        upstream_endpoint,
    })
}

async fn metrics_handler(State(state): State<Arc<AppState>>) -> Json<ClientTelemetrySnapshot> {
    let snapshot = state
        .telemetry
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    Json(snapshot)
}

fn content_type_for_filename(filename: &str) -> &'static str {
    let upper = filename.to_ascii_uppercase();
    if upper.ends_with(".TXT") || upper.ends_with(".WMO") || upper.ends_with(".XML") {
        "text/plain; charset=utf-8"
    } else if upper.ends_with(".JSON") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

fn sanitize_requested_filename(raw: &str) -> Option<String> {
    let trimmed = raw.trim_start_matches('/').trim();
    if trimmed.is_empty() || trimmed.contains('\0') || trimmed.contains("..") {
        return None;
    }
    if trimmed.starts_with('/') || trimmed.starts_with('\\') {
        return None;
    }
    Some(trimmed.to_string())
}

fn parse_servers_or_default(raw_servers: &[String]) -> anyhow::Result<Vec<(String, u16)>> {
    if raw_servers.is_empty() {
        return Ok(vec![
            ("emwin.weathermessage.com".to_string(), 2211),
            ("master.weathermessage.com".to_string(), 2211),
            ("emwin.interweather.net".to_string(), 1000),
            ("wxmesg.upstateweather.com".to_string(), 2211),
        ]);
    }

    raw_servers
        .iter()
        .map(|entry| {
            parse_server(entry).ok_or_else(|| {
                anyhow::anyhow!("invalid --server entry: {entry} (expected host:port)")
            })
        })
        .collect()
}

fn publish(state: &Arc<AppState>, kind: EventKind) {
    let id = state.next_event_id.fetch_add(1, Ordering::Relaxed);
    let _ = state.event_tx.send(BroadcastEvent { id, kind });
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p = pattern.to_ascii_lowercase();
    let t = text.to_ascii_lowercase();

    let p_bytes = p.as_bytes();
    let t_bytes = t.as_bytes();
    let mut pi = 0usize;
    let mut ti = 0usize;
    let mut star_idx = None;
    let mut match_idx = 0usize;

    while ti < t_bytes.len() {
        if pi < p_bytes.len() && (p_bytes[pi] == t_bytes[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p_bytes.len() && p_bytes[pi] == b'*' {
            star_idx = Some(pi);
            match_idx = ti;
            pi += 1;
        } else if let Some(star_pos) = star_idx {
            pi = star_pos + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }

    while pi < p_bytes.len() && p_bytes[pi] == b'*' {
        pi += 1;
    }

    pi == p_bytes.len()
}

fn event_matches_filter(filter: Option<&str>, event: &EventKind) -> bool {
    match filter {
        Some(pattern) => match event.filename() {
            Some(filename) => wildcard_match(pattern, filename),
            None => true,
        },
        None => true,
    }
}

fn log_info(quiet: bool, msg: &str) {
    if !quiet {
        eprintln!("{msg}");
    }
}

fn log_error(msg: &str) {
    eprintln!("{} {msg}", label_error());
}

fn build_cors_layer(cors_origin: Option<String>) -> anyhow::Result<CorsLayer> {
    if let Some(origin) = cors_origin {
        if origin == "*" {
            return Ok(CorsLayer::new().allow_origin(Any).allow_methods(Any));
        }

        let header_value = HeaderValue::from_str(&origin)
            .map_err(|err| anyhow::anyhow!("invalid --cors-origin value {origin}: {err}"))?;
        return Ok(CorsLayer::new()
            .allow_origin(header_value)
            .allow_methods(Any));
    }

    Ok(CorsLayer::new().allow_methods(Any))
}

#[cfg(test)]
mod tests {
    use super::{
        AppState, EventKind, EventsQuery, RetainedFiles, build_router, event_matches_filter,
        events_handler, sanitize_requested_filename, wildcard_match,
    };
    use axum::body::{Body, to_bytes};
    use axum::extract::{ConnectInfo, Query, State};
    use axum::http::{HeaderMap, Request, StatusCode};
    use byteblaster_core::ClientTelemetrySnapshot;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::time::{Duration, Instant, SystemTime};
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    fn test_state(max_clients: usize) -> Arc<AppState> {
        Arc::new(AppState {
            event_tx: broadcast::channel(32).0,
            retained_files: std::sync::Mutex::new(RetainedFiles::new(32, Duration::from_secs(60))),
            telemetry: std::sync::Mutex::new(ClientTelemetrySnapshot::default()),
            connected_clients: AtomicUsize::new(0),
            max_clients,
            next_event_id: AtomicU64::new(1),
            started_at: Instant::now(),
            upstream_endpoint: std::sync::Mutex::new(None),
            quiet: true,
        })
    }

    #[test]
    fn wildcard_patterns_match_case_insensitive() {
        assert!(wildcard_match("*.TXT", "warn123.txt"));
        assert!(wildcard_match("WARN*", "warn123.txt"));
        assert!(wildcard_match("*orecast*", "FORecast_report.txt"));
        assert!(!wildcard_match("*.ZIP", "warn123.txt"));
    }

    #[test]
    fn sanitize_filename_rejects_bad_paths() {
        assert_eq!(
            sanitize_requested_filename("file.txt"),
            Some("file.txt".to_string())
        );
        assert_eq!(
            sanitize_requested_filename("/nested/file.txt"),
            Some("nested/file.txt".to_string())
        );
        assert!(sanitize_requested_filename("../file.txt").is_none());
        assert!(sanitize_requested_filename(" ").is_none());
    }

    #[test]
    fn retained_files_evict_by_capacity_and_ttl() {
        let mut files = RetainedFiles::new(2, Duration::from_millis(50));
        files.insert("a.txt".to_string(), vec![1], SystemTime::now());
        files.insert("b.txt".to_string(), vec![2], SystemTime::now());
        files.insert("c.txt".to_string(), vec![3], SystemTime::now());

        assert!(files.get("a.txt").is_none());
        assert!(files.get("b.txt").is_some());
        assert!(files.get("c.txt").is_some());

        let old = SystemTime::now() - Duration::from_secs(1);
        files.insert("old.txt".to_string(), vec![9], old);
        assert!(files.get("old.txt").is_none());
    }

    #[tokio::test]
    async fn events_handler_rejects_when_client_limit_reached() {
        let state = test_state(1);
        state.connected_clients.store(1, Ordering::Relaxed);

        let result = events_handler(
            State(state),
            ConnectInfo("127.0.0.1:4000".parse().expect("valid socket addr")),
            HeaderMap::new(),
            Query(EventsQuery { filter: None }),
        )
        .await;

        assert_eq!(result.err(), Some(StatusCode::TOO_MANY_REQUESTS));
    }

    #[tokio::test]
    async fn files_download_accepts_url_encoded_nested_filename() {
        let state = test_state(10);
        {
            let mut guard = state
                .retained_files
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.insert(
                "nested/my file.txt".to_string(),
                b"hello world".to_vec(),
                SystemTime::now(),
            );
        }

        let app = build_router(Arc::clone(&state), None).expect("router should build");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/files/nested%2Fmy%20file.txt")
                    .method("GET")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        assert_eq!(&body[..], b"hello world");
    }

    #[test]
    fn events_filter_only_allows_matching_filenames() {
        let txt = EventKind::FileComplete {
            filename: "report.txt".to_string(),
            size: 2,
        };
        let zip = EventKind::FileComplete {
            filename: "report.zip".to_string(),
            size: 1,
        };
        let telemetry = EventKind::Telemetry(ClientTelemetrySnapshot::default());

        assert!(event_matches_filter(Some("*.txt"), &txt));
        assert!(!event_matches_filter(Some("*.txt"), &zip));
        assert!(event_matches_filter(Some("*.txt"), &telemetry));
    }
}
