//! Stream command for real-time event streaming.
//!
//! This module provides functionality to stream events from capture files
//! or live ByteBlaster servers, with optional file assembly and output.

use crate::cmd::event_output::text_preview;
use crate::live::shared::{parse_servers_or_default, unix_seconds, write_completed_file};
use crate::product_meta::detect_product_meta;
use byteblaster_core::{
    ByteBlasterClient, Client, ClientConfig, ClientEvent, DecodeConfig, FileAssembler,
    FrameDecoder, FrameEvent, ProtocolDecoder, SegmentAssembler,
};
use futures::StreamExt;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn};

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
    let bytes = std::fs::read(input_path)?;

    let mut decoder = ProtocolDecoder::default();
    let events = decoder.feed(&bytes)?;
    let output_dir_path = output_dir.map(PathBuf::from);
    if let Some(path) = &output_dir_path {
        std::fs::create_dir_all(path)?;
    }
    let mut assembler = output_dir_path.as_ref().map(|_| FileAssembler::new(100));
    let mut written_files = Vec::new();

    for event in &events {
        log_frame_event(event, text_preview_chars);
        if let Some(assembler) = assembler.as_mut()
            && let FrameEvent::DataBlock(segment) = event
            && let Some(file) = assembler.push(segment.clone())?
        {
            let path = write_completed_file(
                output_dir_path
                    .as_deref()
                    .expect("output dir configured when assembler enabled"),
                &file.filename,
                &file.data,
            )?;
            let timestamp_utc = unix_seconds(file.timestamp_utc);
            info!(
                path = %path,
                filename = %file.filename,
                timestamp_utc,
                "wrote file"
            );
            written_files.push(path);
        }
    }

    info!(
        events = events.len(),
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
    let email = live
        .email
        .ok_or_else(|| anyhow::anyhow!("live mode requires --email"))?;

    let pin_servers = !live.servers.is_empty();
    let servers = parse_servers_or_default(&live.servers)?;
    let config = ClientConfig {
        email,
        servers,
        server_list_path: live.server_list_path.map(PathBuf::from),
        follow_server_list_updates: !pin_servers,
        reconnect_delay_secs: 5,
        connection_timeout_secs: 5,
        watchdog_timeout_secs: 49,
        max_exceptions: 10,
        decode: DecodeConfig::default(),
    };

    let mut client = Client::builder(config).build()?;
    client.start()?;
    let mut events = client.events();
    let output_dir_path = output_dir.map(PathBuf::from);
    if let Some(path) = &output_dir_path {
        std::fs::create_dir_all(path)?;
    }
    let mut assembler = output_dir_path.as_ref().map(|_| FileAssembler::new(100));

    let mut seen = 0usize;
    let mut written_files = Vec::new();
    let mut live_stats = LiveStats::default();
    let mut last_auth_logons: Option<u64> = None;
    let idle = Duration::from_secs(live.idle_timeout_secs.max(1));

    while seen < live.max_events {
        let next = tokio::time::timeout(idle, events.next()).await;
        let Some(item) = next.ok().flatten() else {
            break;
        };

        match item {
            Ok(ClientEvent::Frame(frame)) => {
                seen += 1;
                log_frame_event(&frame, text_preview_chars);
                if matches!(frame, FrameEvent::DataBlock(_)) {
                    live_stats.data_blocks_total = live_stats.data_blocks_total.saturating_add(1);
                }
                if let FrameEvent::ServerListUpdate(list) = &frame {
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
                    && let FrameEvent::DataBlock(segment) = &frame
                    && let Some(file) = assembler.push(segment.clone())?
                {
                    let path = write_completed_file(
                        output_dir_path
                            .as_deref()
                            .expect("output dir configured when assembler enabled"),
                        &file.filename,
                        &file.data,
                    )?;
                    let timestamp_utc = unix_seconds(file.timestamp_utc);
                    info!(
                        path = %path,
                        filename = %file.filename,
                        timestamp_utc,
                        "wrote file"
                    );
                    written_files.push(path);
                }
            }
            Ok(ClientEvent::Connected(endpoint)) => {
                live_stats.connections_total = live_stats.connections_total.saturating_add(1);
                info!(
                    endpoint = %endpoint,
                    connections = live_stats.connections_total,
                    "connected"
                );
            }
            Ok(ClientEvent::Disconnected) => {
                live_stats.disconnects_total = live_stats.disconnects_total.saturating_add(1);
                warn!(
                    disconnects = live_stats.disconnects_total,
                    "disconnected; switching server"
                );
            }
            Ok(ClientEvent::Telemetry(snapshot)) => {
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
                    watchdog_exception_events_total = snapshot.watchdog_exception_events_total,
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
        status = "ok",
        "stream live complete"
    );

    Ok(())
}

fn log_frame_event(frame: &FrameEvent, text_preview_chars: usize) {
    match frame {
        FrameEvent::DataBlock(segment) => {
            let product = detect_product_meta(&segment.filename);
            let preview = text_preview(&segment.filename, &segment.content, text_preview_chars);
            info!(
                event = "data_block",
                filename = %segment.filename,
                block_number = segment.block_number,
                total_blocks = segment.total_blocks,
                bytes = segment.content.len(),
                timestamp_utc = unix_seconds(segment.timestamp_utc),
                product_title = ?product.as_ref().map(|meta| meta.title.as_str()),
                preview = preview.as_deref(),
                "frame event"
            );
        }
        FrameEvent::ServerListUpdate(list) => {
            info!(
                event = "server_list",
                servers = list.servers.len(),
                sat_servers = list.sat_servers.len(),
                "frame event"
            );
        }
        FrameEvent::Warning(warning) => {
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
