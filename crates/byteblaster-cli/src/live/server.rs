//! Server command for running an HTTP server with SSE endpoints.
//!
//! This module provides an HTTP server that:
//! - Streams events via Server-Sent Events (SSE)
//! - Serves completed files for download
//! - Provides health and metrics endpoints
//! - Supports CORS for browser clients

use crate::ReceiverKind;
use crate::live::config::{LiveConfigRequest, LiveReceiverConfig, build_live_receiver_config};
use crate::live::server_support::RetainedFiles;
use axum::http::HeaderValue;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};

mod server_http;
mod server_ingest;
mod types;

pub use types::ServerOptions;
use types::{AppState, BroadcastEvent, EventKind, TelemetryPayload};

#[cfg(test)]
use server_http::{event_matches_filter, files_handler};

/// Capacity of the broadcast channel for events.
const EVENT_CHANNEL_CAPACITY: usize = 4096;

pub async fn run(options: ServerOptions) -> crate::error::CliResult<()> {
    let ServerOptions {
        receiver,
        username,
        password,
        raw_servers,
        server_list_path,
        bind,
        cors_origin,
        max_clients,
        stats_interval_secs,
        file_retention_secs,
        max_retained_files,
        quiet,
    } = options;

    let bind_addr = SocketAddr::from_str(&bind).map_err(|err| {
        crate::error::CliError::invalid_argument(format!("invalid --bind value {bind}: {err}"))
    })?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let state = Arc::new(AppState {
        event_tx: broadcast::channel(EVENT_CHANNEL_CAPACITY).0,
        shutdown_rx: shutdown_rx.clone(),
        retained_files: Mutex::new(RetainedFiles::new(
            max_retained_files.max(1),
            Duration::from_secs(file_retention_secs.max(1)),
        )),
        telemetry: Mutex::new(TelemetryPayload::Unavailable),
        connected_clients: AtomicUsize::new(0),
        max_clients: max_clients.max(1),
        next_event_id: AtomicU64::new(1),
        data_blocks_total: AtomicU64::new(0),
        current_servers: AtomicUsize::new(0),
        current_sat_servers: AtomicUsize::new(0),
        started_at: Instant::now(),
        upstream_endpoint: Mutex::new(None),
        quiet,
    });

    let cors = build_cors_layer(cors_origin)?;
    let app = server_http::build_router(Arc::clone(&state), cors);

    let listener = TcpListener::bind(bind_addr).await?;
    log_info(quiet, &format!("server listening addr={bind_addr}"));

    let ingest_task = match receiver {
        ReceiverKind::Qbt => {
            let LiveReceiverConfig::Qbt(config) = build_live_receiver_config(LiveConfigRequest {
                receiver: ReceiverKind::Qbt,
                username: Some(username),
                password,
                raw_servers,
                server_list_path,
                idle_timeout_secs: 90,
                qbt_watchdog_timeout_secs: 20,
                username_context: "server mode",
                password_context: "server mode",
            })?
            else {
                unreachable!("qbt server mode must build qbt config");
            };
            tokio::spawn(server_ingest::run_qbt_ingest_loop(
                config,
                Arc::clone(&state),
                shutdown_rx.clone(),
            ))
        }
        ReceiverKind::Wxwire => {
            let LiveReceiverConfig::WxWire(config) =
                build_live_receiver_config(LiveConfigRequest {
                    receiver: ReceiverKind::Wxwire,
                    username: Some(username),
                    password,
                    raw_servers,
                    server_list_path,
                    idle_timeout_secs: 90,
                    qbt_watchdog_timeout_secs: 0,
                    username_context: "wxwire server mode",
                    password_context: "wxwire server mode",
                })?
            else {
                unreachable!("wxwire server mode must build wxwire config");
            };
            tokio::spawn(server_ingest::run_wxwire_ingest_loop(
                config,
                Arc::clone(&state),
                shutdown_rx.clone(),
            ))
        }
    };
    let stats_task = tokio::spawn(server_ingest::run_stats_loop(
        Arc::clone(&state),
        stats_interval_secs,
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
        return Err(crate::error::CliError::runtime(format!(
            "http server failed: {err}"
        )));
    }
    match ingest_result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            return Err(crate::error::CliError::runtime(format!(
                "ingest task failed: {err}"
            )));
        }
        Err(err) => {
            return Err(crate::error::CliError::runtime(format!(
                "ingest task join failed: {err}"
            )));
        }
    }
    match stats_result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            return Err(crate::error::CliError::runtime(format!(
                "stats task failed: {err}"
            )));
        }
        Err(err) => {
            return Err(crate::error::CliError::runtime(format!(
                "stats task join failed: {err}"
            )));
        }
    }

    Ok(())
}

const DASHBOARD_HTML: &str = include_str!("../cmd/dashboard.html");

#[cfg(test)]
fn build_router(
    state: Arc<AppState>,
    cors_origin: Option<String>,
) -> crate::error::CliResult<axum::Router> {
    let cors = build_cors_layer(cors_origin)?;
    Ok(server_http::build_router(state, cors))
}

fn publish(state: &Arc<AppState>, kind: EventKind) {
    let id = state
        .next_event_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

fn build_cors_layer(cors_origin: Option<String>) -> crate::error::CliResult<CorsLayer> {
    if let Some(origin) = cors_origin {
        if origin == "*" {
            return Ok(CorsLayer::new().allow_origin(Any).allow_methods(Any));
        }

        let header_value = HeaderValue::from_str(&origin).map_err(|err| {
            crate::error::CliError::invalid_argument(format!(
                "invalid --cors-origin value {origin}: {err}"
            ))
        })?;
        return Ok(CorsLayer::new()
            .allow_origin(header_value)
            .allow_methods(Any));
    }

    Ok(CorsLayer::new().allow_methods(Any))
}

#[cfg(test)]
mod tests {
    use super::{
        TelemetryPayload, build_router, event_matches_filter, files_handler,
        sanitize_requested_filename,
    };
    use crate::live::file_pipeline::CompletedFileMetadata;
    use crate::live::server::types::{AppState, EventKind, EventsQuery, FileCompleteEventPayload};
    use crate::live::server_support::RetainedFiles;
    use crate::live::server_support::wildcard_match;
    use axum::Json;
    use axum::body::{Body, to_bytes};
    use axum::extract::{ConnectInfo, Query, State};
    use axum::http::{HeaderMap, Request, StatusCode};
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
            telemetry: std::sync::Mutex::new(TelemetryPayload::Unavailable),
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

    fn file_complete_event(filename: &str) -> EventKind {
        EventKind::FileComplete(Box::new(FileCompleteEventPayload::from_metadata(
            CompletedFileMetadata {
                filename: filename.to_string(),
                size: 11,
                timestamp_utc: 1,
                product: None,
                text_product_header: None,
                text_product_enrichment: None,
                text_product_warning: None,
            },
        )))
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
            Query(EventsQuery { filter: None }),
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
    async fn files_endpoint_serializes_enriched_metadata() {
        let state = test_state(10);
        {
            let mut guard = state
                .retained_files
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.insert(
                "TAFPDKGA.TXT".to_string(),
                b"000
FTUS42 KFFC 022320
TAFPDK
Body
"
                .to_vec(),
                1,
                SystemTime::now(),
            );
        }

        let Json(response) = files_handler(State(state)).await;
        let file = &response.files[0];
        assert_eq!(file.filename, "TAFPDKGA.TXT");
        assert_eq!(
            file.product.as_ref().and_then(|value| value.pil.as_deref()),
            Some("TAF")
        );
        assert!(
            file.product
                .as_ref()
                .map(|value| !value.title.is_empty())
                .unwrap_or(false)
        );
        assert_eq!(
            file.text_product_header
                .as_ref()
                .map(|value| value.ttaaii.as_str()),
            Some("FTUS42")
        );
        assert_eq!(
            file.text_product_enrichment
                .as_ref()
                .and_then(|value| value.pil_nnn.as_deref()),
            Some("TAF")
        );
        assert!(file.text_product_warning.is_none());
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
        let txt = file_complete_event("report.txt");
        let zip = file_complete_event("report.zip");
        let telemetry = EventKind::Telemetry(TelemetryPayload::Unavailable);

        assert!(event_matches_filter(Some("*.txt"), &txt));
        assert!(!event_matches_filter(Some("*.txt"), &zip));
        assert!(event_matches_filter(Some("*.txt"), &telemetry));
    }

    #[test]
    fn file_complete_event_includes_download_url() {
        let value = file_complete_event("nested/my file.txt").to_json();

        assert_eq!(value["download_url"], "/files/nested%2Fmy%20file.txt");
        assert_eq!(value["timestamp_utc"], 1);
    }
}
