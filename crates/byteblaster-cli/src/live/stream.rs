//! Stream command for real-time event streaming.
//!
//! This module provides functionality to stream events from capture files
//! or live ByteBlaster servers, with optional file assembly and output.

use crate::ReceiverKind;
use crate::cmd::event_output::text_preview;
use crate::live::file_pipeline::persist_completed_file;
use crate::live::shared::{parse_servers_or_default, unix_seconds};
use crate::product_meta::detect_product_meta;
use byteblaster_core::ingest::{
    IngestEvent, IngestTelemetry, IngestWarning, ProductOrigin, QbtIngestStream, ReceivedProduct,
    WxWireIngestStream,
};
use byteblaster_core::qbt_receiver::{
    QbtDecodeConfig, QbtFileAssembler, QbtFrameDecoder, QbtFrameEvent, QbtProtocolDecoder,
    QbtReceiver, QbtReceiverConfig, QbtSegmentAssembler,
};
use byteblaster_core::wxwire_receiver::{WxWireReceiver, WxWireReceiverConfig};
use futures::StreamExt;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn};

const CAPTURE_READ_BUFFER_BYTES: usize = 64 * 1024;

/// Statistics tracked during live streaming.
#[derive(Debug, Default)]
struct LiveStats {
    /// Total successful connections.
    connections_total: u64,
    /// Total disconnections.
    disconnects_total: u64,
    /// Total products received.
    products_total: u64,
}

pub async fn run(
    input: Option<String>,
    output_dir: Option<String>,
    live: crate::LiveOptions,
    text_preview_chars: usize,
) -> anyhow::Result<()> {
    if let Some(input_path) = input {
        return run_capture_mode(&input_path, output_dir.as_deref(), text_preview_chars);
    }

    run_live_mode(output_dir.as_deref(), live, text_preview_chars).await
}

fn run_capture_mode(
    input_path: &str,
    output_dir: Option<&str>,
    text_preview_chars: usize,
) -> anyhow::Result<()> {
    let mut reader = std::fs::File::open(input_path)?;
    let mut buf = vec![0u8; CAPTURE_READ_BUFFER_BYTES];
    let mut decoder = QbtProtocolDecoder::default();
    let output_dir_path = output_dir.map(PathBuf::from);
    if let Some(path) = &output_dir_path {
        std::fs::create_dir_all(path)?;
    }
    let mut assembler = output_dir_path.as_ref().map(|_| QbtFileAssembler::new(100));
    let mut written_files = Vec::new();
    let mut events_total = 0usize;

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }

        let events = decoder.feed(&buf[..n])?;
        for event in events {
            events_total += 1;
            log_frame_event(&event, text_preview_chars);
            if let Some(assembler) = assembler.as_mut()
                && let QbtFrameEvent::DataBlock(segment) = event
                && let Some(file) = assembler.push(segment)?
            {
                let completed = persist_completed_file(
                    output_dir_path
                        .as_deref()
                        .expect("output dir configured when assembler enabled"),
                    &file.filename,
                    &file.data,
                    file.timestamp_utc,
                )?;
                log_completed_file(&completed);
                written_files.push(completed.path);
            }
        }
    }

    info!(
        events = events_total,
        files = written_files.len(),
        status = "ok",
        "stream capture complete"
    );

    Ok(())
}

async fn run_live_mode(
    output_dir: Option<&str>,
    live: crate::LiveOptions,
    text_preview_chars: usize,
) -> anyhow::Result<()> {
    let output_dir_path = output_dir.map(PathBuf::from);
    if let Some(path) = &output_dir_path {
        std::fs::create_dir_all(path)?;
    }
    let mut written_files = Vec::new();

    match live.receiver {
        ReceiverKind::Qbt => {
            if live.password.is_some() {
                return Err(anyhow::anyhow!(
                    "--password is not supported with --receiver qbt"
                ));
            }
            let email = live
                .email
                .ok_or_else(|| anyhow::anyhow!("live mode requires --email"))?;

            let pin_servers = !live.servers.is_empty();
            let servers = parse_servers_or_default(&live.servers)?;
            let config = QbtReceiverConfig {
                email,
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
                        if let Some(output_dir) = output_dir_path.as_deref() {
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
                        live_stats.connections_total =
                            live_stats.connections_total.saturating_add(1);
                        info!(
                            endpoint = %endpoint,
                            connections = live_stats.connections_total,
                            "connected"
                        );
                    }
                    Ok(IngestEvent::Disconnected) => {
                        live_stats.disconnects_total =
                            live_stats.disconnects_total.saturating_add(1);
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
                            watchdog_exception_events_total =
                                snapshot.watchdog_exception_events_total,
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
        }
        ReceiverKind::Wxwire => {
            if !live.servers.is_empty() {
                return Err(anyhow::anyhow!(
                    "--server is not supported with --receiver wxwire"
                ));
            }
            if live.server_list_path.is_some() {
                return Err(anyhow::anyhow!(
                    "--server-list-path is not supported with --receiver wxwire"
                ));
            }
            let username = live
                .email
                .ok_or_else(|| anyhow::anyhow!("wxwire live mode requires --email"))?;
            let password = live
                .password
                .ok_or_else(|| anyhow::anyhow!("wxwire live mode requires --password"))?;

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
                        if let Some(output_dir) = output_dir_path.as_deref() {
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
        }
    }

    Ok(())
}

fn log_frame_event(frame: &QbtFrameEvent, text_preview_chars: usize) {
    match frame {
        QbtFrameEvent::DataBlock(segment) => {
            let product = detect_product_meta(&segment.filename);
            let preview = text_preview(&segment.filename, &segment.content, text_preview_chars);
            info!(
                event = "data_block",
                filename = %segment.filename,
                block_number = segment.block_number,
                total_blocks = segment.total_blocks,
                bytes = segment.content.len(),
                timestamp_utc = unix_seconds(segment.timestamp_utc),
                product_title = product
                    .as_ref()
                    .map(|meta| meta.title.as_str())
                    .unwrap_or(""),
                preview = preview.as_deref(),
                "frame event"
            );
        }
        QbtFrameEvent::ServerListUpdate(list) => {
            info!(
                event = "server_list",
                servers = list.servers.len(),
                sat_servers = list.sat_servers.len(),
                "frame event"
            );
        }
        QbtFrameEvent::Warning(warning) => {
            warn!(
                event = "warning",
                warning = ?warning,
                "frame warning"
            );
        }
        other => {
            info!(
                event = "other",
                frame = ?other,
                "frame event"
            );
        }
    }
}

fn log_completed_file(completed: &crate::live::file_pipeline::CompletedFileRecord) {
    let header = completed.event.get("text_product_header");
    let enrichment = completed.event.get("text_product_enrichment");
    let warning = completed.event.get("text_product_warning");
    info!(
        path = %completed.path,
        filename = %completed.filename,
        timestamp_utc = completed.timestamp_utc,
        text_product_afos = header.and_then(|value| value.get("afos")).and_then(|value| value.as_str()),
        text_product_ttaaii = header.and_then(|value| value.get("ttaaii")).and_then(|value| value.as_str()),
        text_product_pil_nnn = enrichment.and_then(|value| value.get("pil_nnn")).and_then(|value| value.as_str()),
        text_product_warning_code = warning.and_then(|value| value.get("code")).and_then(|value| value.as_str()),
        "wrote file"
    );
}

fn log_product_event(product: &ReceivedProduct, text_preview_chars: usize) {
    let meta = detect_product_meta(&product.filename);
    let preview = text_preview(&product.filename, &product.data, text_preview_chars);
    match &product.origin {
        ProductOrigin::Qbt => {
            info!(
                event = "product",
                source = "qbt",
                filename = %product.filename,
                bytes = product.data.len(),
                timestamp_utc = unix_seconds(product.source_timestamp_utc),
                product_title = meta
                    .as_ref()
                    .map(|value| value.title.as_str())
                    .unwrap_or(""),
                preview = preview.as_deref(),
                "ingest event"
            );
        }
        ProductOrigin::WxWire {
            message_id,
            subject,
            delay_stamp_utc,
        } => {
            info!(
                event = "product",
                source = "wxwire",
                filename = %product.filename,
                bytes = product.data.len(),
                timestamp_utc = unix_seconds(product.source_timestamp_utc),
                message_id = %message_id,
                subject = %subject,
                delay_stamp_utc = delay_stamp_utc.map(unix_seconds),
                product_title = meta
                    .as_ref()
                    .map(|value| value.title.as_str())
                    .unwrap_or(""),
                preview = preview.as_deref(),
                "ingest event"
            );
        }
        _ => {
            info!(
                event = "product",
                source = "unknown",
                filename = %product.filename,
                bytes = product.data.len(),
                timestamp_utc = unix_seconds(product.source_timestamp_utc),
                "ingest event"
            );
        }
    }
}

fn log_ingest_warning(warning: &IngestWarning) {
    match warning {
        IngestWarning::Qbt(value) => {
            warn!(event = "warning", source = "qbt", warning = ?value, "ingest warning");
        }
        IngestWarning::WxWire(value) => {
            warn!(event = "warning", source = "wxwire", warning = ?value, "ingest warning");
        }
        _ => {
            warn!(event = "warning", source = "unknown", warning = ?warning, "ingest warning");
        }
    }
}
