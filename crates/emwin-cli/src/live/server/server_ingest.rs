use super::types::{AppState, EventKind, FileCompleteEventPayload, TelemetryPayload};
use emwin_protocol::ingest::{
    IngestConfig, IngestError, IngestEvent, IngestReceiver, IngestTelemetry, IngestWarning,
    ProductOrigin,
};
use emwin_protocol::qbt_receiver::{QbtFrameEvent, QbtProtocolWarning, QbtReceiverConfig};
use emwin_protocol::wxwire_receiver::{WxWireReceiverConfig, WxWireReceiverFrameEvent};
use futures::StreamExt;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};
use tokio::sync::watch;
use tracing::info;

pub(super) async fn run_qbt_ingest_loop(
    config: QbtReceiverConfig,
    state: Arc<AppState>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> crate::error::CliResult<()> {
    let mut ingest = IngestReceiver::build(IngestConfig::Qbt(config))?;
    ingest.start()?;

    let mut events = ingest.events()?;
    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                break;
            }
            item = events.next() => {
                let Some(item) = item else {
                    break;
                };
                handle_ingest_event(item, &state);
            }
        }
    }

    drop(events);
    ingest.stop().await?;

    Ok(())
}

pub(super) async fn run_wxwire_ingest_loop(
    config: WxWireReceiverConfig,
    state: Arc<AppState>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> crate::error::CliResult<()> {
    let mut ingest = IngestReceiver::build(IngestConfig::WxWire(config))?;
    ingest.start()?;

    let mut events = ingest.events()?;
    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                break;
            }
            item = events.next() => {
                let Some(item) = item else {
                    break;
                };
                handle_ingest_event(item, &state);
            }
        }
    }

    drop(events);
    ingest.stop().await?;

    Ok(())
}

fn handle_ingest_event(item: Result<IngestEvent, IngestError>, state: &Arc<AppState>) {
    match item {
        Ok(IngestEvent::Connected { endpoint }) => {
            {
                let mut guard = state
                    .upstream_endpoint
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *guard = Some(endpoint.clone());
            }
            super::log_info(
                state.quiet,
                &format!("upstream connected endpoint={endpoint}"),
            );
            super::publish(
                state,
                EventKind::Connected {
                    endpoint: endpoint.clone(),
                },
            );
        }
        Ok(IngestEvent::Disconnected) => {
            {
                let mut guard = state
                    .upstream_endpoint
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *guard = None;
            }
            super::log_info(state.quiet, "upstream disconnected");
            super::publish(state, EventKind::Disconnected);
        }
        Ok(IngestEvent::Telemetry(snapshot)) => {
            let telemetry_value = match snapshot {
                IngestTelemetry::Qbt(value) => TelemetryPayload::Qbt(value),
                IngestTelemetry::WxWire(value) => TelemetryPayload::WxWire(value),
                _ => TelemetryPayload::Unavailable,
            };
            {
                let mut guard = state
                    .telemetry
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *guard = telemetry_value.clone();
            }
            super::publish(state, EventKind::Telemetry(telemetry_value));
        }
        Ok(IngestEvent::Product(product)) => {
            if matches!(product.origin, ProductOrigin::Qbt) {
                state.data_blocks_total.fetch_add(1, Ordering::Relaxed);
            }

            let completed_at = SystemTime::now();
            let timestamp_utc = product
                .source_timestamp_utc
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            let retained_meta = {
                let mut guard = state
                    .retained_files
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.insert(
                    product.filename.clone(),
                    product.data.to_vec(),
                    timestamp_utc,
                    completed_at,
                )
            };
            super::publish(
                state,
                EventKind::FileComplete(Box::new(FileCompleteEventPayload::from_metadata(
                    retained_meta,
                ))),
            );
            super::log_info(
                state.quiet,
                &format!(
                    "file complete name={} bytes={} timestamp_utc={}",
                    product.filename,
                    product.data.len(),
                    timestamp_utc
                ),
            );
        }
        Ok(IngestEvent::Warning(warning)) => match warning {
            IngestWarning::Qbt(value) => {
                if let QbtProtocolWarning::BackpressureDrop { .. } = value {
                    super::log_info(state.quiet, "qbt ingest backpressure warning");
                }
                super::publish(state, EventKind::QbtFrame(QbtFrameEvent::Warning(value)));
            }
            IngestWarning::WxWire(value) => {
                super::publish(
                    state,
                    EventKind::WxWireFrame(WxWireReceiverFrameEvent::Warning(value)),
                );
            }
            _ => {
                super::publish(
                    state,
                    EventKind::Error {
                        message: format!("ingest warning: {warning:?}"),
                    },
                );
            }
        },
        Err(err) => {
            super::log_error(&format!("client error: {err}"));
            super::publish(
                state,
                EventKind::Error {
                    message: err.to_string(),
                },
            );
        }
        Ok(_) => {}
    }
}

pub(super) async fn run_stats_loop(
    state: Arc<AppState>,
    stats_interval_secs: u64,
    mut shutdown_rx: watch::Receiver<bool>,
) -> crate::error::CliResult<()> {
    if stats_interval_secs == 0 {
        let _ = shutdown_rx.changed().await;
        return Ok(());
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
                let data_blocks = state.data_blocks_total.load(Ordering::Relaxed);
                let servers = state.current_servers.load(Ordering::Relaxed);
                let sat_servers = state.current_sat_servers.load(Ordering::Relaxed);

                let uptime_secs = state.started_at.elapsed().as_secs();
                let upstream = endpoint.unwrap_or_else(|| "disconnected".to_string());
                info!(
                    uptime_secs,
                    data_blocks_total = data_blocks,
                    servers,
                    sat_servers,
                    retained_files = files,
                    connected_clients = clients,
                    upstream,
                    "server stats snapshot"
                );
            }
        }
    }

    Ok(())
}
