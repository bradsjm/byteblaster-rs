use super::{AppState, EventKind};
use anyhow::{Context, Result};
use byteblaster_core::{
    ByteBlasterClient, Client, ClientConfig, ClientEvent, FileAssembler, FrameEvent,
    SegmentAssembler,
};
use futures::StreamExt;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};
use tokio::sync::watch;
use tracing::info;

pub(super) async fn run_ingest_loop(
    config: ClientConfig,
    state: Arc<AppState>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let mut assembler = FileAssembler::new(100);
    let mut client = Client::builder(config)
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
                handle_client_event(item, &state, &mut assembler);
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
        Ok(ClientEvent::Disconnected) => {
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
        Ok(ClientEvent::Telemetry(snapshot)) => {
            {
                let mut guard = state
                    .telemetry
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *guard = snapshot.clone();
            }
            super::publish(state, EventKind::Telemetry(snapshot));
        }
        Ok(ClientEvent::Frame(frame)) => match frame {
            FrameEvent::DataBlock(segment) => {
                state.data_blocks_total.fetch_add(1, Ordering::Relaxed);
                super::publish(
                    state,
                    EventKind::Frame(FrameEvent::DataBlock(segment.clone())),
                );

                match assembler.push(segment) {
                    Ok(Some(file)) => {
                        let completed_at = SystemTime::now();
                        let timestamp_utc = file
                            .timestamp_utc
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        {
                            let mut guard = state
                                .retained_files
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            guard.insert(
                                file.filename.clone(),
                                file.data.to_vec(),
                                timestamp_utc,
                                completed_at,
                            );
                        }
                        super::log_info(
                            state.quiet,
                            &format!(
                                "file complete name={} bytes={} timestamp_utc={}",
                                file.filename,
                                file.data.len(),
                                timestamp_utc
                            ),
                        );
                        super::publish(
                            state,
                            EventKind::FileComplete {
                                filename: file.filename,
                                size: file.data.len(),
                                timestamp_utc,
                            },
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
            FrameEvent::ServerListUpdate(list) => {
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
                super::publish(state, EventKind::Frame(FrameEvent::ServerListUpdate(list)));
            }
            FrameEvent::Warning(warning) => {
                super::publish(state, EventKind::Frame(FrameEvent::Warning(warning)));
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
                let data_blocks = state.data_blocks_total.load(Ordering::Relaxed);
                let servers = state.current_servers.load(Ordering::Relaxed);
                let sat_servers = state.current_sat_servers.load(Ordering::Relaxed);

                let uptime_secs = state.started_at.elapsed().as_secs();
                let upstream = endpoint.unwrap_or_else(|| "disconnected".to_string());
                info!(
                    uptime_secs,
                    bytes_in_total = telemetry.bytes_in_total,
                    frame_events_total = telemetry.frame_events_total,
                    data_blocks_total = data_blocks,
                    event_queue_drop_total = telemetry.event_queue_drop_total,
                    server_list_updates_total = telemetry.server_list_updates_total,
                    servers,
                    sat_servers,
                    auth_logon_sent_total = telemetry.auth_logon_sent_total,
                    watchdog_timeouts_total = telemetry.watchdog_timeouts_total,
                    watchdog_exception_events_total = telemetry.watchdog_exception_events_total,
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
