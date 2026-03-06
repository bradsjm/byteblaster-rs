use crate::live::file_pipeline::persist_completed_file;
use crate::live::stream::common::{log_completed_file, log_ingest_warning, log_product_event};
use byteblaster_core::ingest::{IngestEvent, IngestTelemetry, WxWireIngestStream};
use byteblaster_core::wxwire_receiver::{WxWireReceiver, WxWireReceiverConfig};
use futures::StreamExt;
use std::path::Path;
use std::time::Duration;
use tracing::{info, warn};

pub(super) async fn run_wxwire_live_mode(
    output_dir: Option<&Path>,
    live: crate::LiveOptions,
    text_preview_chars: usize,
) -> crate::error::CliResult<()> {
    if !live.servers.is_empty() {
        return Err(crate::error::CliError::invalid_argument(
            "--server is not supported with --receiver wxwire",
        ));
    }
    if live.server_list_path.is_some() {
        return Err(crate::error::CliError::invalid_argument(
            "--server-list-path is not supported with --receiver wxwire",
        ));
    }

    let username = live.username.ok_or_else(|| {
        crate::error::CliError::invalid_argument("wxwire live mode requires --username")
    })?;
    let password = live.password.ok_or_else(|| {
        crate::error::CliError::invalid_argument("wxwire live mode requires --password")
    })?;

    let receiver = WxWireReceiver::builder(WxWireReceiverConfig {
        username,
        password,
        idle_timeout_secs: live.idle_timeout_secs.max(1),
        ..WxWireReceiverConfig::default()
    })
    .build()?;

    let mut ingest = WxWireIngestStream::new(receiver);
    ingest.start()?;
    let mut events = ingest.events();

    let idle = Duration::from_secs(live.idle_timeout_secs.max(1));
    let mut seen = 0usize;
    let mut connections_total = 0u64;
    let mut disconnects_total = 0u64;
    let mut products_total = 0u64;
    let mut written_files = Vec::new();

    while seen < live.max_events {
        let next = tokio::time::timeout(idle, events.next()).await;
        let Some(item) = next.ok().flatten() else {
            break;
        };

        match item {
            Ok(IngestEvent::Product(product)) => {
                seen += 1;
                products_total = products_total.saturating_add(1);
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
                connections_total = connections_total.saturating_add(1);
                info!(
                    endpoint = %endpoint,
                    connections = connections_total,
                    "connected"
                );
            }
            Ok(IngestEvent::Disconnected) => {
                disconnects_total = disconnects_total.saturating_add(1);
                warn!(
                    disconnects = disconnects_total,
                    "disconnected; reconnecting"
                );
            }
            Ok(IngestEvent::Telemetry(IngestTelemetry::WxWire(snapshot))) => {
                seen += 1;
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
            Ok(IngestEvent::Warning(warning)) => {
                seen += 1;
                log_ingest_warning(&warning);
            }
            Ok(_) => {}
            Err(err) => {
                warn!(error = %err, "stream wxwire live warning");
            }
        }
    }

    drop(events);
    ingest.stop().await?;

    info!(
        events = seen,
        files = written_files.len(),
        products = products_total,
        connections = connections_total,
        disconnects = disconnects_total,
        receiver = "wxwire",
        status = "ok",
        "stream live complete"
    );

    Ok(())
}
