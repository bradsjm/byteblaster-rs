//! Live stream command implementation.
//!
//! This module implements the `stream` subcommand for the EMWIN CLI.
//! It connects to live servers and streams product events in real-time,
//! with optional file output to a directory.
//!
//! ## Features
//!
//! - Real-time event streaming from QBT or Weather Wire sources
//! - Optional file persistence to a directory
//! - Telemetry logging and connection state tracking
//! - Idle timeout handling for automatic disconnection

mod common;

use crate::live::ingest::{LiveIngest, LiveIngestRequest};
use common::{LiveStats, log_completed_file, log_ingest_warning, log_product_event};
use emwin_protocol::ingest::{IngestEvent, IngestTelemetry};
use futures::StreamExt;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn};

pub async fn run(
    output_dir: Option<String>,
    live: crate::LiveOptions,
    text_preview_chars: usize,
) -> crate::error::CliResult<()> {
    let output_dir_path = output_dir.map(PathBuf::from);
    if let Some(path) = &output_dir_path {
        std::fs::create_dir_all(path)?;
    }

    let mut ingest = LiveIngest::build(LiveIngestRequest {
        live: &live,
        qbt_watchdog_timeout_secs: 49,
        username_context: "live mode",
        password_context: "live mode",
    })?;
    let receiver = ingest.receiver_kind();

    ingest.start()?;
    let mut events = ingest.events()?;
    let mut written_files = Vec::new();
    let mut stats = LiveStats::default();
    let mut seen = 0usize;
    let mut last_auth_logons = None;
    let idle_timeout = Duration::from_secs(live.idle_timeout_secs.max(1));

    while seen < live.max_events {
        let next = tokio::time::timeout(idle_timeout, events.next()).await;
        let Some(item) = next.ok().flatten() else {
            break;
        };

        match item {
            Ok(IngestEvent::Product(product)) => {
                seen += 1;
                stats.products_total = stats.products_total.saturating_add(1);
                log_product_event(&product, text_preview_chars);
                if let Some(output_dir) = output_dir_path.as_deref() {
                    let completed = crate::live::file_pipeline::persist_completed_file(
                        output_dir,
                        &product.filename,
                        &product.data,
                        product.source_timestamp_utc,
                    )?;
                    log_completed_file(&completed);
                    written_files.push(completed.path);
                }
            }
            Ok(IngestEvent::Connected { endpoint }) => {
                stats.connections_total = stats.connections_total.saturating_add(1);
                info!(
                    endpoint = %endpoint,
                    connections = stats.connections_total,
                    "connected"
                );
            }
            Ok(IngestEvent::Disconnected) => {
                stats.disconnects_total = stats.disconnects_total.saturating_add(1);
                let message = match receiver {
                    crate::ReceiverKind::Qbt => "disconnected; switching server",
                    crate::ReceiverKind::Wxwire => "disconnected; reconnecting",
                };
                warn!(disconnects = stats.disconnects_total, "{message}");
            }
            Ok(IngestEvent::Telemetry(telemetry)) => {
                seen += 1;
                log_telemetry(telemetry, stats.products_total, &mut last_auth_logons);
            }
            Ok(IngestEvent::Warning(warning)) => {
                seen += 1;
                log_ingest_warning(&warning);
            }
            Ok(_) => {}
            Err(err) => {
                warn!(error = %err, "stream warning");
            }
        }
    }

    drop(events);
    ingest.stop().await?;

    info!(
        events = seen,
        files = written_files.len(),
        products = stats.products_total,
        connections = stats.connections_total,
        disconnects = stats.disconnects_total,
        receiver = format!("{receiver:?}").to_ascii_lowercase(),
        status = "ok",
        "stream live complete"
    );

    Ok(())
}

fn log_telemetry(
    telemetry: IngestTelemetry,
    products_total: u64,
    last_auth_logons: &mut Option<u64>,
) {
    match telemetry {
        IngestTelemetry::Qbt(snapshot) => {
            let auth_delta = last_auth_logons
                .map(|previous| snapshot.auth_logon_sent_total.saturating_sub(previous))
                .unwrap_or(0);
            if auth_delta > 0 {
                info!(
                    auth_logon_delta = auth_delta,
                    auth_logon_total = snapshot.auth_logon_sent_total,
                    "auth logon sent"
                );
            }
            *last_auth_logons = Some(snapshot.auth_logon_sent_total);

            info!(
                bytes_in_total = snapshot.bytes_in_total,
                frame_events_total = snapshot.frame_events_total,
                products_total,
                event_queue_drop_total = snapshot.event_queue_drop_total,
                auth_logon_sent_total = snapshot.auth_logon_sent_total,
                watchdog_timeouts_total = snapshot.watchdog_timeouts_total,
                watchdog_exception_events_total = snapshot.watchdog_exception_events_total,
                "telemetry"
            );
        }
        IngestTelemetry::WxWire(snapshot) => {
            info!(
                decoded_messages_total = snapshot.decoded_messages_total,
                files_emitted_total = snapshot.files_emitted_total,
                products_total,
                warning_events_total = snapshot.warning_events_total,
                event_queue_drop_total = snapshot.event_queue_drop_total,
                reconnect_attempts_total = snapshot.reconnect_attempts_total,
                "telemetry"
            );
        }
        _ => {}
    }
}
