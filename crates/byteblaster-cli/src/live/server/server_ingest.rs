use super::{AppState, EventKind};
use anyhow::{Context, Result};
use byteblaster_core::qbt_receiver::{
    QbtFileAssembler, QbtFrameEvent, QbtReceiver, QbtReceiverClient, QbtReceiverConfig,
    QbtReceiverEvent, QbtSegmentAssembler,
};
use byteblaster_core::wxwire_receiver::client::{
    WxWireReceiver, WxWireReceiverClient, WxWireReceiverEvent,
};
use byteblaster_core::wxwire_receiver::config::WxWireReceiverConfig;
use byteblaster_core::wxwire_receiver::model::WxWireReceiverFrameEvent;
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
) -> Result<()> {
    let mut assembler = QbtFileAssembler::new(100);
    let mut client = QbtReceiver::builder(config)
        .build()
        .context("failed to build upstream client")?;
    client.start().context("failed to start upstream client")?;

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
                handle_qbt_client_event(item, &state, &mut assembler);
            }
        }
    }

    drop(events);
    client
        .stop()
        .await
        .context("failed to stop upstream client")?;

    Ok(())
}

pub(super) async fn run_wxwire_ingest_loop(
    config: WxWireReceiverConfig,
    state: Arc<AppState>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let mut client = WxWireReceiver::builder(config)
        .build()
        .context("failed to build upstream client")?;
    client.start().context("failed to start upstream client")?;

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
                handle_wxwire_client_event(item, &state);
            }
        }
    }

    drop(events);
    client
        .stop()
        .await
        .context("failed to stop upstream client")?;

    Ok(())
}

fn handle_qbt_client_event(
    item: Result<QbtReceiverEvent, byteblaster_core::qbt_receiver::QbtReceiverError>,
    state: &Arc<AppState>,
    assembler: &mut QbtFileAssembler,
) {
    match item {
        Ok(QbtReceiverEvent::Connected(endpoint)) => {
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
        Ok(QbtReceiverEvent::Disconnected) => {
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
        Ok(QbtReceiverEvent::Telemetry(snapshot)) => {
            {
                let mut guard = state
                    .telemetry
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *guard = serde_json::to_value(&snapshot).unwrap_or_else(|_| serde_json::json!({}));
            }
            super::publish(
                state,
                EventKind::Telemetry(
                    serde_json::to_value(&snapshot).unwrap_or_else(|_| serde_json::json!({})),
                ),
            );
        }
        Ok(QbtReceiverEvent::Frame(frame)) => match frame {
            QbtFrameEvent::DataBlock(segment) => {
                state.data_blocks_total.fetch_add(1, Ordering::Relaxed);
                super::publish(
                    state,
                    EventKind::QbtFrame(QbtFrameEvent::DataBlock(segment.clone())),
                );

                match assembler.push(segment) {
                    Ok(Some(file)) => {
                        let completed_at = SystemTime::now();
                        let timestamp_utc = file
                            .timestamp_utc
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        let retained_meta = {
                            let mut guard = state
                                .retained_files
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            guard.insert(
                                file.filename.clone(),
                                file.data.to_vec(),
                                timestamp_utc,
                                completed_at,
                            )
                        };
                        super::publish(
                            state,
                            EventKind::FileComplete {
                                filename: retained_meta.filename,
                                size: retained_meta.size,
                                timestamp_utc: retained_meta.timestamp_utc,
                                product: retained_meta.product,
                                text_product_header: retained_meta.text_product_header,
                                text_product_enrichment: retained_meta.text_product_enrichment,
                                text_product_warning: retained_meta.text_product_warning,
                            },
                        );
                        super::log_info(
                            state.quiet,
                            &format!(
                                "file complete name={} bytes={} timestamp_utc={}",
                                file.filename,
                                file.data.len(),
                                timestamp_utc
                            ),
                        );
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let message = format!("assembler error: {error}");
                        super::log_error(&message);
                        super::publish(state, EventKind::Error { message });
                    }
                }
            }
            QbtFrameEvent::ServerListUpdate(list) => {
                state
                    .current_servers
                    .store(list.servers.len(), Ordering::Relaxed);
                state
                    .current_sat_servers
                    .store(list.sat_servers.len(), Ordering::Relaxed);
                super::log_info(
                    state.quiet,
                    &format!(
                        "server list received servers={} sat_servers={}",
                        list.servers.len(),
                        list.sat_servers.len()
                    ),
                );
                super::publish(
                    state,
                    EventKind::QbtFrame(QbtFrameEvent::ServerListUpdate(list)),
                );
            }
            QbtFrameEvent::Warning(warning) => {
                super::publish(state, EventKind::QbtFrame(QbtFrameEvent::Warning(warning)));
            }
            _ => {}
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

fn handle_wxwire_client_event(
    item: Result<
        WxWireReceiverEvent,
        byteblaster_core::wxwire_receiver::error::WxWireReceiverError,
    >,
    state: &Arc<AppState>,
) {
    match item {
        Ok(WxWireReceiverEvent::Connected(endpoint)) => {
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
        Ok(WxWireReceiverEvent::Disconnected) => {
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
        Ok(WxWireReceiverEvent::Telemetry(snapshot)) => {
            {
                let mut guard = state
                    .telemetry
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *guard = serde_json::to_value(&snapshot).unwrap_or_else(|_| serde_json::json!({}));
            }
            super::publish(
                state,
                EventKind::Telemetry(
                    serde_json::to_value(&snapshot).unwrap_or_else(|_| serde_json::json!({})),
                ),
            );
        }
        Ok(WxWireReceiverEvent::Frame(frame)) => {
            super::publish(state, EventKind::WxWireFrame(frame.clone()));
            if let WxWireReceiverFrameEvent::File(file) = frame {
                let completed_at = SystemTime::now();
                let timestamp_utc = file
                    .issue_utc
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let retained_meta = {
                    let mut guard = state
                        .retained_files
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    guard.insert(
                        file.filename.clone(),
                        file.data.to_vec(),
                        timestamp_utc,
                        completed_at,
                    )
                };
                super::publish(
                    state,
                    EventKind::FileComplete {
                        filename: retained_meta.filename,
                        size: retained_meta.size,
                        timestamp_utc: retained_meta.timestamp_utc,
                        product: retained_meta.product,
                        text_product_header: retained_meta.text_product_header,
                        text_product_enrichment: retained_meta.text_product_enrichment,
                        text_product_warning: retained_meta.text_product_warning,
                    },
                );
                super::log_info(
                    state.quiet,
                    &format!(
                        "file complete name={} bytes={} timestamp_utc={}",
                        file.filename,
                        file.data.len(),
                        timestamp_utc
                    ),
                );
            }
        }
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
) -> Result<()> {
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
