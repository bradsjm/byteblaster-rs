//! Stream command for real-time event streaming.
//!
//! This module provides functionality to stream events from capture files
//! or live ByteBlaster servers, with optional file assembly and output.

use crate::ReceiverKind;
use crate::cmd::event_output::text_preview;
use crate::live::file_pipeline::persist_completed_file;
use crate::live::shared::{parse_servers_or_default, unix_seconds};
use crate::product_meta::detect_product_meta;
use byteblaster_core::qbt_receiver::{
    QbtDecodeConfig, QbtFileAssembler, QbtFrameDecoder, QbtFrameEvent, QbtProtocolDecoder,
    QbtReceiver, QbtReceiverClient, QbtReceiverConfig, QbtReceiverEvent, QbtSegmentAssembler,
};
use byteblaster_core::wxwire_receiver::client::{
    WxWireReceiver, WxWireReceiverClient, WxWireReceiverEvent,
};
use byteblaster_core::wxwire_receiver::config::WxWireReceiverConfig;
use byteblaster_core::wxwire_receiver::model::{WxWireReceiverFile, WxWireReceiverFrameEvent};
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
    /// Total server list updates received.
    server_list_updates_total: u64,
    /// Current number of primary servers.
    current_servers: usize,
    /// Current number of satellite servers.
    current_sat_servers: usize,
    /// Total data blocks received.
    data_blocks_total: u64,
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

            let mut client = QbtReceiver::builder(config).build()?;
            client.start()?;
            let mut events = client.events();
            let mut assembler = output_dir_path.as_ref().map(|_| QbtFileAssembler::new(100));

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
                    Ok(QbtReceiverEvent::Frame(frame)) => {
                        seen += 1;
                        log_frame_event(&frame, text_preview_chars);
                        if matches!(frame, QbtFrameEvent::DataBlock(_)) {
                            live_stats.data_blocks_total =
                                live_stats.data_blocks_total.saturating_add(1);
                        }
                        if let QbtFrameEvent::ServerListUpdate(list) = &frame {
                            live_stats.server_list_updates_total =
                                live_stats.server_list_updates_total.saturating_add(1);
                            live_stats.current_servers = list.servers.len();
                            live_stats.current_sat_servers = list.sat_servers.len();
                            info!(
                                updates = live_stats.server_list_updates_total,
                                servers = list.servers.len(),
                                sat_servers = list.sat_servers.len(),
                                "server list received"
                            );
                        }
                        if let Some(assembler) = assembler.as_mut()
                            && let QbtFrameEvent::DataBlock(segment) = &frame
                            && let Some(file) = assembler.push(segment.clone())?
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
                    Ok(QbtReceiverEvent::Connected(endpoint)) => {
                        live_stats.connections_total =
                            live_stats.connections_total.saturating_add(1);
                        info!(
                            endpoint = %endpoint,
                            connections = live_stats.connections_total,
                            "connected"
                        );
                    }
                    Ok(QbtReceiverEvent::Disconnected) => {
                        live_stats.disconnects_total =
                            live_stats.disconnects_total.saturating_add(1);
                        warn!(
                            disconnects = live_stats.disconnects_total,
                            "disconnected; switching server"
                        );
                    }
                    Ok(QbtReceiverEvent::Telemetry(snapshot)) => {
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
                            data_blocks_total = live_stats.data_blocks_total,
                            event_queue_drop_total = snapshot.event_queue_drop_total,
                            server_list_updates_total = snapshot.server_list_updates_total,
                            current_servers = live_stats.current_servers,
                            current_sat_servers = live_stats.current_sat_servers,
                            auth_logon_sent_total = snapshot.auth_logon_sent_total,
                            watchdog_timeouts_total = snapshot.watchdog_timeouts_total,
                            watchdog_exception_events_total =
                                snapshot.watchdog_exception_events_total,
                            "telemetry"
                        );
                    }
                    Ok(_) => {}
                    Err(err) => {
                        warn!(error = %err, "stream live warning");
                    }
                }
            }

            drop(events);
            client.stop().await?;

            info!(
                events = seen,
                files = written_files.len(),
                server_list_updates = live_stats.server_list_updates_total,
                current_servers = live_stats.current_servers,
                current_sat_servers = live_stats.current_sat_servers,
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

            let mut client = WxWireReceiver::builder(WxWireReceiverConfig {
                username,
                password,
                idle_timeout_secs: live.idle_timeout_secs.max(1),
                ..WxWireReceiverConfig::default()
            })
            .build()?;
            client.start()?;
            let mut events = client.events();
            let idle = Duration::from_secs(live.idle_timeout_secs.max(1));
            let mut seen = 0usize;
            let mut connections_total = 0u64;
            let mut disconnects_total = 0u64;

            while seen < live.max_events {
                let next = tokio::time::timeout(idle, events.next()).await;
                let Some(item) = next.ok().flatten() else {
                    break;
                };

                match item {
                    Ok(WxWireReceiverEvent::Frame(frame)) => {
                        seen += 1;
                        log_wxwire_frame_event(&frame, text_preview_chars);
                        if let Some(output_dir) = output_dir_path.as_deref()
                            && let WxWireReceiverFrameEvent::File(file) = &frame
                        {
                            let completed = persist_completed_file(
                                output_dir,
                                &file.filename,
                                &file.data,
                                file.issue_utc,
                            )?;
                            log_completed_file(&completed);
                            written_files.push(completed.path);
                        }
                    }
                    Ok(WxWireReceiverEvent::Connected(endpoint)) => {
                        connections_total = connections_total.saturating_add(1);
                        info!(
                            endpoint = %endpoint,
                            connections = connections_total,
                            "connected"
                        );
                    }
                    Ok(WxWireReceiverEvent::Disconnected) => {
                        disconnects_total = disconnects_total.saturating_add(1);
                        warn!(
                            disconnects = disconnects_total,
                            "disconnected; reconnecting"
                        );
                    }
                    Ok(WxWireReceiverEvent::Telemetry(snapshot)) => {
                        seen += 1;
                        info!(
                            decoded_messages_total = snapshot.decoded_messages_total,
                            files_emitted_total = snapshot.files_emitted_total,
                            warning_events_total = snapshot.warning_events_total,
                            event_queue_drop_total = snapshot.event_queue_drop_total,
                            reconnect_attempts_total = snapshot.reconnect_attempts_total,
                            "telemetry"
                        );
                    }
                    Ok(_) => {}
                    Err(err) => {
                        warn!(error = %err, "stream wxwire live warning");
                    }
                }
            }

            drop(events);
            client.stop().await?;

            info!(
                events = seen,
                files = written_files.len(),
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

fn log_wxwire_frame_event(frame: &WxWireReceiverFrameEvent, text_preview_chars: usize) {
    match frame {
        WxWireReceiverFrameEvent::File(file) => {
            log_wxwire_file_event(file, text_preview_chars);
        }
        WxWireReceiverFrameEvent::Warning(warning) => {
            warn!(
                event = "warning",
                warning = ?warning,
                "frame warning"
            );
        }
        _ => {
            info!(
                event = "other",
                frame = ?frame,
                "frame event"
            );
        }
    }
}

fn log_wxwire_file_event(file: &WxWireReceiverFile, text_preview_chars: usize) {
    let product = detect_product_meta(&file.filename);
    let preview = text_preview(&file.filename, &file.data, text_preview_chars);
    info!(
        event = "file",
        filename = %file.filename,
        bytes = file.data.len(),
        issue_utc = unix_seconds(file.issue_utc),
        ttaaii = %file.ttaaii,
        cccc = %file.cccc,
        awipsid = %file.awipsid,
        id = %file.id,
        subject = %file.subject,
        product_title = product
            .as_ref()
            .map(|meta| meta.title.as_str())
            .unwrap_or(""),
        preview = preview.as_deref(),
        "frame event"
    );
}
