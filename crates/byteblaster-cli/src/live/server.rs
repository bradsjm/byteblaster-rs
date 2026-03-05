//! Server command for running an HTTP server with SSE endpoints.
//!
//! This module provides an HTTP server that:
//! - Streams events via Server-Sent Events (SSE)
//! - Serves completed files for download
//! - Provides health and metrics endpoints
//! - Supports CORS for browser clients

use crate::cmd::event_output::{frame_event_filename, frame_event_name, frame_event_to_json};
use crate::live::server_support::{RetainedFileMeta, RetainedFiles, file_download_url};
use crate::live::shared::parse_servers_or_default;
use crate::product_meta::detect_product_meta;
use axum::http::HeaderValue;
use byteblaster_core::{ClientConfig, ClientTelemetrySnapshot, DecodeConfig, FrameEvent};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};

mod server_http;
mod server_ingest;

#[cfg(test)]
use server_http::{event_matches_filter, files_handler};

/// Capacity of the broadcast channel for events.
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
    Frame(FrameEvent),
    FileComplete {
        filename: String,
        size: usize,
        timestamp_utc: u64,
    },
    Telemetry(ClientTelemetrySnapshot),
    Error {
        message: String,
    },
}

impl EventKind {
    fn event_name(&self) -> &'static str {
        match self {
            Self::Connected { .. } => "connected",
            Self::Disconnected => "disconnected",
            Self::Frame(frame) => frame_event_name(frame),
            Self::FileComplete { .. } => "file_complete",
            Self::Telemetry(_) => "telemetry",
            Self::Error { .. } => "error",
        }
    }

    fn filename(&self) -> Option<&str> {
        match self {
            Self::Frame(frame) => frame_event_filename(frame),
            Self::FileComplete { filename, .. } => Some(filename.as_str()),
            _ => None,
        }
    }

    fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Connected { endpoint } => serde_json::json!({ "endpoint": endpoint }),
            Self::Disconnected => serde_json::json!({}),
            Self::Frame(frame) => frame_event_to_json(frame, 0),
            Self::FileComplete {
                filename,
                size,
                timestamp_utc,
            } => {
                let mut payload = serde_json::json!({
                    "filename": filename,
                    "size": size,
                    "timestamp_utc": timestamp_utc,
                    "download_url": file_download_url(filename),
                });
                if let Some(product) = detect_product_meta(filename)
                    && let Ok(product_json) = serde_json::to_value(product)
                {
                    payload["product"] = product_json;
                }
                payload
            }
            Self::Telemetry(snapshot) => serde_json::json!(snapshot),
            Self::Error { message } => serde_json::json!({ "message": message }),
        }
    }
}

#[derive(Debug)]
struct AppState {
    event_tx: broadcast::Sender<BroadcastEvent>,
    shutdown_rx: watch::Receiver<bool>,
    retained_files: Mutex<RetainedFiles>,
    telemetry: Mutex<ClientTelemetrySnapshot>,
    connected_clients: AtomicUsize,
    max_clients: usize,
    next_event_id: AtomicU64,
    data_blocks_total: AtomicU64,
    current_servers: AtomicUsize,
    current_sat_servers: AtomicUsize,
    started_at: Instant,
    upstream_endpoint: Mutex<Option<String>>,
    quiet: bool,
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    filter: Option<String>,
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

#[derive(Debug, Serialize)]
struct EndpointDoc {
    method: &'static str,
    path: &'static str,
    description: &'static str,
}

#[derive(Debug, Serialize)]
struct RootResponse {
    service: &'static str,
    endpoints: Vec<EndpointDoc>,
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
    let pin_servers = !options.raw_servers.is_empty();
    let servers = parse_servers_or_default(&options.raw_servers)?;
    let bind_addr = SocketAddr::from_str(&options.bind)
        .map_err(|err| anyhow::anyhow!("invalid --bind value {}: {err}", options.bind))?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let state = Arc::new(AppState {
        event_tx: broadcast::channel(EVENT_CHANNEL_CAPACITY).0,
        shutdown_rx: shutdown_rx.clone(),
        retained_files: Mutex::new(RetainedFiles::new(
            options.max_retained_files.max(1),
            Duration::from_secs(options.file_retention_secs.max(1)),
        )),
        telemetry: Mutex::new(ClientTelemetrySnapshot::default()),
        connected_clients: AtomicUsize::new(0),
        max_clients: options.max_clients.max(1),
        next_event_id: AtomicU64::new(1),
        data_blocks_total: AtomicU64::new(0),
        current_servers: AtomicUsize::new(0),
        current_sat_servers: AtomicUsize::new(0),
        started_at: Instant::now(),
        upstream_endpoint: Mutex::new(None),
        quiet: options.quiet,
    });

    let cors = build_cors_layer(options.cors_origin)?;
    let app = server_http::build_router(Arc::clone(&state), cors);

    let listener = TcpListener::bind(bind_addr).await?;
    log_info(options.quiet, &format!("server listening addr={bind_addr}"));

    let config = ClientConfig {
        email: options.email,
        servers,
        server_list_path: options.server_list_path.map(PathBuf::from),
        follow_server_list_updates: !pin_servers,
        reconnect_delay_secs: 5,
        connection_timeout_secs: 5,
        watchdog_timeout_secs: 20,
        max_exceptions: 10,
        decode: DecodeConfig::default(),
    };

    let ingest_task = tokio::spawn(server_ingest::run_ingest_loop(
        config,
        Arc::clone(&state),
        shutdown_rx.clone(),
    ));
    let stats_task = tokio::spawn(server_ingest::run_stats_loop(
        Arc::clone(&state),
        options.stats_interval_secs,
        shutdown_rx.clone(),
    ));
    let ctrlc_task = tokio::spawn({
        let shutdown_tx = shutdown_tx.clone();
        async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = shutdown_tx.send(true);
        }
    });
    let mut http_shutdown_rx = shutdown_rx.clone();

    let serve = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        let _ = http_shutdown_rx.changed().await;
    });

    let serve_result = serve.await;
    ctrlc_task.abort();
    let _ = shutdown_tx.send(true);

    let ingest_result = ingest_task.await;
    let stats_result = stats_task.await;

    if let Err(err) = serve_result {
        return Err(anyhow::anyhow!("http server failed: {err}"));
    }
    match ingest_result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            return Err(anyhow::anyhow!("ingest task failed: {err}"));
        }
        Err(err) => {
            return Err(anyhow::anyhow!("ingest task join failed: {err}"));
        }
    }
    match stats_result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            return Err(anyhow::anyhow!("stats task failed: {err}"));
        }
        Err(err) => {
            return Err(anyhow::anyhow!("stats task join failed: {err}"));
        }
    }

    Ok(())
}

const DASHBOARD_HTML: &str = include_str!("../cmd/dashboard.html");

#[cfg(test)]
fn build_router(state: Arc<AppState>, cors_origin: Option<String>) -> anyhow::Result<axum::Router> {
    let cors = build_cors_layer(cors_origin)?;
    Ok(server_http::build_router(state, cors))
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
            &format!("sse client disconnected peer={}", self.peer),
        );
    }
}

fn publish(state: &Arc<AppState>, kind: EventKind) {
    let id = state.next_event_id.fetch_add(1, Ordering::Relaxed);
    let _ = state.event_tx.send(BroadcastEvent { id, kind });
}

#[cfg(test)]
fn sanitize_requested_filename(raw: &str) -> Option<String> {
    crate::live::server_support::sanitize_requested_filename(raw)
}

fn log_info(quiet: bool, msg: &str) {
    if !quiet {
        info!("{msg}");
    }
}

fn log_error(msg: &str) {
    error!("{msg}");
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
        AppState, EventKind, RetainedFiles, build_router, event_matches_filter, files_handler,
        sanitize_requested_filename,
    };
    use crate::live::server_support::wildcard_match;
    use axum::Json;
    use axum::body::{Body, to_bytes};
    use axum::extract::{ConnectInfo, Query, State};
    use axum::http::{HeaderMap, Request, StatusCode};
    use byteblaster_core::ClientTelemetrySnapshot;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::time::{Duration, Instant, SystemTime};
    use tokio::sync::{broadcast, watch};
    use tower::ServiceExt;

    fn test_state(max_clients: usize) -> Arc<AppState> {
        let (_, shutdown_rx) = watch::channel(false);
        Arc::new(AppState {
            event_tx: broadcast::channel(32).0,
            shutdown_rx,
            retained_files: std::sync::Mutex::new(RetainedFiles::new(32, Duration::from_secs(60))),
            telemetry: std::sync::Mutex::new(ClientTelemetrySnapshot::default()),
            connected_clients: AtomicUsize::new(0),
            max_clients,
            next_event_id: AtomicU64::new(1),
            data_blocks_total: AtomicU64::new(0),
            current_servers: AtomicUsize::new(0),
            current_sat_servers: AtomicUsize::new(0),
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
        files.insert("a.txt".to_string(), vec![1], 1, SystemTime::now());
        files.insert("b.txt".to_string(), vec![2], 2, SystemTime::now());
        files.insert("c.txt".to_string(), vec![3], 3, SystemTime::now());

        assert!(files.get("a.txt").is_none());
        assert!(files.get("b.txt").is_some());
        assert!(files.get("c.txt").is_some());

        let old = SystemTime::now() - Duration::from_secs(1);
        files.insert("old.txt".to_string(), vec![9], 9, old);
        assert!(files.get("old.txt").is_none());
    }

    #[tokio::test]
    async fn events_handler_rejects_when_client_limit_reached() {
        let state = test_state(1);
        state.connected_clients.store(1, Ordering::Relaxed);

        let result = crate::live::server::server_http::events_handler(
            State(state),
            ConnectInfo("127.0.0.1:4000".parse().expect("valid socket addr")),
            HeaderMap::new(),
            Query(super::EventsQuery { filter: None }),
        )
        .await;

        assert!(matches!(result, Err(StatusCode::TOO_MANY_REQUESTS)));
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
                1,
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

    #[tokio::test]
    async fn files_endpoint_includes_product_metadata_when_detectable() {
        let state = test_state(10);
        {
            let mut guard = state
                .retained_files
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.insert(
                "TAFPDKGA.TXT".to_string(),
                b"data".to_vec(),
                1,
                SystemTime::now(),
            );
        }

        let Json(response) = files_handler(State(state)).await;
        let value = serde_json::to_value(response).expect("files response should serialize");
        assert_eq!(value["files"][0]["filename"], "TAFPDKGA.TXT");
        assert_eq!(value["files"][0]["product"]["pil"], "TAF");
        assert_eq!(
            value["files"][0]["product"]["title"],
            "Terminal Aerodrome Forecast"
        );
    }

    #[tokio::test]
    async fn root_endpoint_lists_available_routes() {
        let state = test_state(10);
        let app = build_router(state, None).expect("router should build");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
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
        let body_text = String::from_utf8(body.to_vec()).expect("body should be utf8 json");
        assert!(body_text.contains("\"/events?filter=*.TXT\""));
        assert!(body_text.contains("\"/dashboard\""));
        assert!(body_text.contains("\"/files\""));
        assert!(body_text.contains("\"/health\""));
        assert!(body_text.contains("\"/metrics\""));
    }

    #[tokio::test]
    async fn dashboard_endpoint_serves_html() {
        let state = test_state(10);
        let app = build_router(state, None).expect("router should build");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/dashboard")
                    .method("GET")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        assert!(content_type.starts_with("text/html"));
    }

    #[tokio::test]
    async fn dashboard_trailing_slash_redirects_to_canonical_path() {
        let state = test_state(10);
        let app = build_router(state, None).expect("router should build");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/dashboard/")
                    .method("GET")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(
            response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok()),
            Some("/dashboard")
        );
    }

    #[test]
    fn events_filter_only_allows_matching_filenames() {
        let txt = EventKind::FileComplete {
            filename: "report.txt".to_string(),
            size: 2,
            timestamp_utc: 1,
        };
        let zip = EventKind::FileComplete {
            filename: "report.zip".to_string(),
            size: 1,
            timestamp_utc: 1,
        };
        let telemetry = EventKind::Telemetry(ClientTelemetrySnapshot::default());

        assert!(event_matches_filter(Some("*.txt"), &txt));
        assert!(!event_matches_filter(Some("*.txt"), &zip));
        assert!(event_matches_filter(Some("*.txt"), &telemetry));
    }

    #[test]
    fn file_complete_event_includes_download_url() {
        let value = EventKind::FileComplete {
            filename: "nested/my file.txt".to_string(),
            size: 11,
            timestamp_utc: 1,
        }
        .to_json();

        assert_eq!(value["download_url"], "/files/nested%2Fmy%20file.txt");
        assert_eq!(value["timestamp_utc"], 1);
    }
}
