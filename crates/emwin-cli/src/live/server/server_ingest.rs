//! Feed ingest events into the live server state.
//!
//! This module translates runtime events into retained files, broadcast notifications, and
//! telemetry snapshots without leaking transport-specific details into the HTTP layer.

use super::types::{AppState, CompletedFileEventPayload, EventKind, TelemetryPayload};
use crate::live::archive_postprocess::post_process_archive;
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

/// Runs the QBT ingest loop until shutdown or stream termination.
pub(super) async fn run_qbt_ingest_loop(
    config: QbtReceiverConfig,
    state: Arc<AppState>,
    post_process_archives: bool,
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
                handle_ingest_event(item, &state, post_process_archives);
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
    post_process_archives: bool,
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
                handle_ingest_event(item, &state, post_process_archives);
            }
        }
    }

    drop(events);
    ingest.stop().await?;

    Ok(())
}

fn handle_ingest_event(
    item: Result<IngestEvent, IngestError>,
    state: &Arc<AppState>,
    post_process_archives: bool,
) {
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

            let delivered =
                match post_process_archive(post_process_archives, &product.filename, &product.data)
                {
                    Ok(delivered) => delivered,
                    Err(err) => {
                        tracing::warn!(
                            archive_filename = %product.filename,
                            error = %err,
                            "Corrupt Zip File Received"
                        );
                        return;
                    }
                };
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
                    delivered.filename.clone(),
                    delivered.data.to_vec(),
                    timestamp_utc,
                    completed_at,
                )
            };
            super::publish(
                state,
                EventKind::FileComplete(Box::new(CompletedFileEventPayload::from_metadata(
                    retained_meta,
                ))),
            );
            super::log_info(
                state.quiet,
                &format!(
                    "file complete name={} bytes={} timestamp_utc={}",
                    delivered.filename,
                    delivered.data.len(),
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
                let received_servers = state.received_servers.load(Ordering::Relaxed);
                let received_sat_servers = state.received_sat_servers.load(Ordering::Relaxed);

                let uptime_secs = state.started_at.elapsed().as_secs();
                let upstream = endpoint.unwrap_or_else(|| "disconnected".to_string());
                info!(
                    uptime_secs,
                    data_blocks_total = data_blocks,
                    received_servers,
                    received_sat_servers,
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

#[cfg(test)]
mod tests {
    use super::handle_ingest_event;
    use crate::live::server::types::{AppState, EventKind, TelemetryPayload};
    use crate::live::server_support::RetainedFiles;
    use bytes::Bytes;
    use emwin_protocol::ingest::IngestEvent;
    use emwin_protocol::qbt_receiver::QbtCompletedFile;
    use std::io::Write;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, AtomicUsize};
    use std::time::{Duration, Instant, SystemTime};
    use tokio::sync::{broadcast, watch};

    fn test_state() -> Arc<AppState> {
        let (_, shutdown_rx) = watch::channel(false);
        Arc::new(AppState {
            event_tx: broadcast::channel(16).0,
            shutdown_rx,
            retained_files: Mutex::new(RetainedFiles::new(16, Duration::from_secs(60))),
            telemetry: Mutex::new(TelemetryPayload::Unavailable),
            connected_clients: AtomicUsize::new(0),
            max_clients: 16,
            next_event_id: AtomicU64::new(1),
            data_blocks_total: AtomicU64::new(0),
            received_servers: AtomicUsize::new(0),
            received_sat_servers: AtomicUsize::new(0),
            started_at: Instant::now(),
            upstream_endpoint: Mutex::new(None),
            quiet: true,
        })
    }

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in entries {
            writer
                .start_file(name, options)
                .expect("start file should succeed");
            writer.write_all(body).expect("write body should succeed");
        }
        writer.finish().expect("finish should succeed").into_inner()
    }

    #[test]
    fn product_event_post_processes_archives_before_retention_and_publish() {
        let state = test_state();
        let mut rx = state.event_tx.subscribe();
        let data = archive(&[(
            "nested/TAFPDKGA.TXT",
            b"000 \nFTUS42 KFFC 022320\nTAFPDK\nBody\n",
        )]);

        handle_ingest_event(
            Ok(IngestEvent::Product(
                QbtCompletedFile {
                    filename: "TAFPDKGA.ZIP".to_string(),
                    data: Bytes::from(data),
                    timestamp_utc: SystemTime::UNIX_EPOCH + Duration::from_secs(1),
                }
                .into(),
            )),
            &state,
            true,
        );

        let retained = state
            .retained_files
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get("nested/TAFPDKGA.TXT")
            .expect("retained file should exist");
        assert_eq!(retained.metadata.product.pil.as_deref(), Some("TAF"));
        assert_eq!(retained.metadata.filename, "nested/TAFPDKGA.TXT");
        assert!(
            state
                .retained_files
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get("TAFPDKGA.ZIP")
                .is_none()
        );

        let published = rx.try_recv().expect("file complete event should publish");
        match published.kind {
            EventKind::FileComplete(file) => {
                assert_eq!(file.metadata.filename, "nested/TAFPDKGA.TXT");
                assert_eq!(file.download_url, "/files/nested%2FTAFPDKGA.TXT");
            }
            _ => panic!("expected file_complete event"),
        }
    }

    #[test]
    fn corrupt_archive_is_dropped_before_retention_and_publish() {
        let state = test_state();
        let mut rx = state.event_tx.subscribe();

        handle_ingest_event(
            Ok(IngestEvent::Product(
                QbtCompletedFile {
                    filename: "BROKEN.ZIP".to_string(),
                    data: Bytes::from_static(b"not a zip"),
                    timestamp_utc: SystemTime::UNIX_EPOCH + Duration::from_secs(1),
                }
                .into(),
            )),
            &state,
            true,
        );

        assert_eq!(
            state
                .retained_files
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .len(),
            0
        );
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }
}
