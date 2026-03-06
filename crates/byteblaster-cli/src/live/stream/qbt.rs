use crate::live::file_pipeline::persist_completed_file;
use crate::live::shared::parse_servers_or_default;
use crate::live::stream::common::{
    LiveStats, log_completed_file, log_ingest_warning, log_product_event,
};
use byteblaster_core::ingest::{IngestEvent, IngestTelemetry, QbtIngestStream};
use byteblaster_core::qbt_receiver::{QbtDecodeConfig, QbtReceiver, QbtReceiverConfig};
use futures::StreamExt;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn};

pub(super) async fn run_qbt_live_mode(
    output_dir: Option<&Path>,
    live: crate::LiveOptions,
    text_preview_chars: usize,
) -> crate::error::CliResult<()> {
    if live.password.is_some() {
        return Err(crate::error::CliError::invalid_argument(
            "--password is not supported with --receiver qbt",
        ));
    }
    let username = live
        .username
        .ok_or_else(|| crate::error::CliError::invalid_argument("live mode requires --username"))?;

    let pin_servers = !live.servers.is_empty();
    let servers = parse_servers_or_default(&live.servers)?;
    let config = QbtReceiverConfig {
        email: username,
        servers,
        server_list_path: live.server_list_path.map(PathBuf::from),
        follow_server_list_updates: !pin_servers,
        reconnect_delay_secs: 5,
        connection_timeout_secs: 5,
        watchdog_timeout_secs: 49,
        max_exceptions: 10,
        decode: QbtDecodeConfig::default(),
    };

    let receiver = QbtReceiver::builder(config).build()?;
    let mut ingest = QbtIngestStream::new(receiver);
    ingest.start()?;
    let mut events = ingest.events();

    let mut written_files = Vec::new();
    let mut seen = 0usize;
    let mut live_stats = LiveStats::default();
    let mut last_auth_logons: Option<u64> = None;
    let idle = Duration::from_secs(live.idle_timeout_secs.max(1));

    while seen < live.max_events {
        let next = tokio::time::timeout(idle, events.next()).await;
        let Some(item) = next.ok().flatten() else {
            break;
        };

        match item {
            Ok(IngestEvent::Product(product)) => {
                seen += 1;
                live_stats.products_total = live_stats.products_total.saturating_add(1);
                log_product_event(&product, text_preview_chars);
                if let Some(output_dir) = output_dir {
                    let completed = persist_completed_file(
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
                live_stats.connections_total = live_stats.connections_total.saturating_add(1);
                info!(
                    endpoint = %endpoint,
                    connections = live_stats.connections_total,
                    "connected"
                );
            }
            Ok(IngestEvent::Disconnected) => {
                live_stats.disconnects_total = live_stats.disconnects_total.saturating_add(1);
                warn!(
                    disconnects = live_stats.disconnects_total,
                    "disconnected; switching server"
                );
            }
            Ok(IngestEvent::Telemetry(IngestTelemetry::Qbt(snapshot))) => {
                seen += 1;
                let auth_delta = last_auth_logons
                    .map(|prev| snapshot.auth_logon_sent_total.saturating_sub(prev))
                    .unwrap_or(0);
                if auth_delta > 0 {
                    info!(
                        auth_logon_delta = auth_delta,
                        auth_logon_total = snapshot.auth_logon_sent_total,
                        "auth logon sent"
                    );
                }
                last_auth_logons = Some(snapshot.auth_logon_sent_total);

                info!(
                    bytes_in_total = snapshot.bytes_in_total,
                    frame_events_total = snapshot.frame_events_total,
                    products_total = live_stats.products_total,
                    event_queue_drop_total = snapshot.event_queue_drop_total,
                    auth_logon_sent_total = snapshot.auth_logon_sent_total,
                    watchdog_timeouts_total = snapshot.watchdog_timeouts_total,
                    watchdog_exception_events_total = snapshot.watchdog_exception_events_total,
                    "telemetry"
                );
            }
            Ok(IngestEvent::Warning(warning)) => {
                seen += 1;
                log_ingest_warning(&warning);
            }
            Ok(_) => {}
            Err(err) => {
                warn!(error = %err, "stream live warning");
            }
        }
    }

    drop(events);
    ingest.stop().await?;

    info!(
        events = seen,
        files = written_files.len(),
        products = live_stats.products_total,
        connections = live_stats.connections_total,
        disconnects = live_stats.disconnects_total,
        receiver = "qbt",
        status = "ok",
        "stream live complete"
    );

    Ok(())
}
