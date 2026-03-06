use super::types::{
    AppState, BroadcastEvent, ClientGuard, EndpointDoc, EventKind, EventsQuery, FilesResponse,
    HealthResponse, RootResponse,
};
use crate::live::server_support::{
    build_file_download_response, filename_request_or_400, wildcard_match,
};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, Redirect, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures::Stream;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

pub(super) fn build_router(state: Arc<AppState>, cors: tower_http::cors::CorsLayer) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/dashboard", get(dashboard_handler))
        .route("/dashboard/", get(dashboard_trailing_slash_handler))
        .route("/events", get(events_handler))
        .route("/files", get(files_handler))
        .route("/files/*filename", get(file_download_handler))
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .layer(cors)
        .with_state(state)
}

pub(super) async fn root_handler() -> Json<RootResponse> {
    Json(RootResponse {
        service: "byteblaster-cli server",
        endpoints: vec![
            EndpointDoc {
                method: "GET",
                path: "/",
                description: "API index with endpoint descriptions",
            },
            EndpointDoc {
                method: "GET",
                path: "/events?filter=*.TXT",
                description: "SSE stream of frame and server events; optional wildcard filename filter",
            },
            EndpointDoc {
                method: "GET",
                path: "/dashboard",
                description: "HTML admin dashboard UI (read-only)",
            },
            EndpointDoc {
                method: "GET",
                path: "/files",
                description: "List retained completed files",
            },
            EndpointDoc {
                method: "GET",
                path: "/files/*filename",
                description: "Download retained file by URL-encoded filename path",
            },
            EndpointDoc {
                method: "GET",
                path: "/health",
                description: "Server health summary",
            },
            EndpointDoc {
                method: "GET",
                path: "/metrics",
                description: "JSON telemetry snapshot",
            },
        ],
    })
}

pub(super) async fn dashboard_handler() -> Html<&'static str> {
    Html(super::DASHBOARD_HTML)
}

pub(super) async fn dashboard_trailing_slash_handler() -> Redirect {
    Redirect::permanent("/dashboard")
}

pub(super) async fn events_handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    if state
        .connected_clients
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current < state.max_clients).then_some(current + 1)
        })
        .is_err()
    {
        super::log_info(
            state.quiet,
            &format!("rejecting client; limit reached peer={peer}"),
        );
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    super::log_info(state.quiet, &format!("sse client connected peer={peer}"));

    let rx = state.event_tx.subscribe();
    let shutdown_rx = state.shutdown_rx.clone();
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
            shutdown_rx,
            peer,
            _guard: Some(ClientGuard {
                state: Arc::clone(&state),
                peer,
            }),
        },
        move |mut st| async move {
            let rx = st.rx.as_mut()?;
            loop {
                tokio::select! {
                    _ = st.shutdown_rx.changed() => return None,
                    received = rx.recv() => match received {
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
                        super::log_info(
                            st.state.quiet,
                            &format!("sse client lagged peer={} dropped={}", st.peer, dropped),
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
            }
        },
    );

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

pub(super) async fn files_handler(State(state): State<Arc<AppState>>) -> Json<FilesResponse> {
    let files = state
        .retained_files
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .list();
    Json(FilesResponse { files })
}

pub(super) async fn file_download_handler(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<Response, StatusCode> {
    let normalized = filename_request_or_400(&filename)?;

    let file = state
        .retained_files
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&normalized)
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(build_file_download_response(file))
}

pub(super) async fn health_handler(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
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

pub(super) async fn metrics_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let snapshot = state
        .telemetry
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    Json(snapshot)
}

pub(super) fn event_matches_filter(filter: Option<&str>, event: &EventKind) -> bool {
    match filter {
        Some(pattern) => match event.filename() {
            Some(filename) => wildcard_match(pattern, filename),
            None => true,
        },
        None => true,
    }
}

pub(super) struct StreamState {
    pub(super) state: Arc<AppState>,
    pub(super) rx: Option<tokio::sync::broadcast::Receiver<BroadcastEvent>>,
    pub(super) last_id: u64,
    pub(super) filter: Option<String>,
    pub(super) shutdown_rx: tokio::sync::watch::Receiver<bool>,
    pub(super) peer: SocketAddr,
    pub(super) _guard: Option<ClientGuard>,
}
