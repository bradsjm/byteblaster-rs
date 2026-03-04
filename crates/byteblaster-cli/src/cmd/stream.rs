//! Stream command for real-time event streaming.
//!
//! This module provides functionality to stream events from capture files
//! or live ByteBlaster servers, with optional file assembly and output.

use crate::cmd::event_output::{frame_event_to_json, frame_event_to_text};
use crate::output::{
    OutputFormat, emit_json_line, emit_text_line, label_info, label_ok, label_stats, label_warn,
};
use byteblaster_core::{
    ByteBlasterClient, Client, ClientConfig, ClientEvent, DecodeConfig, FileAssembler,
    FrameDecoder, FrameEvent, ProtocolDecoder, SegmentAssembler, parse_server,
};
use futures::StreamExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    format: OutputFormat,
    input: Option<String>,
    output_dir: Option<String>,
    live: crate::LiveOptions,
    text_preview_chars: usize,
) -> anyhow::Result<()> {
    if let Some(input_path) = input {
        return run_capture_mode(
            format,
            &input_path,
            output_dir.as_deref(),
            text_preview_chars,
        );
    }

    run_live_mode(format, output_dir.as_deref(), live, text_preview_chars).await
}

fn run_capture_mode(
    format: OutputFormat,
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
    let mut file_events = Vec::new();

    match format {
        OutputFormat::Text => {
            for event in &events {
                emit_text_line(&frame_event_to_text(event, text_preview_chars));
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
                    emit_text_line(&format!(
                        "{} wrote path={path} timestamp_utc={timestamp_utc}",
                        label_ok()
                    ));
                    written_files.push(path.clone());
                    file_events.push(serde_json::json!({
                        "filename": file.filename,
                        "path": path,
                        "timestamp_utc": timestamp_utc,
                    }));
                }
            }
            emit_text_line(&format!(
                "{} stream capture complete events={} files={}",
                label_ok(),
                events.len(),
                written_files.len()
            ));
        }
        OutputFormat::Json => {
            for event in &events {
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
                    written_files.push(path.clone());
                    file_events.push(serde_json::json!({
                        "filename": file.filename,
                        "path": path,
                        "timestamp_utc": timestamp_utc,
                    }));
                }
            }

            emit_json_line(&serde_json::json!({
                "command":"stream",
                "status":"ok",
                "event_count": events.len(),
                "events": events
                    .iter()
                    .map(|event| frame_event_to_json(event, text_preview_chars))
                    .collect::<Vec<_>>(),
                "written_files": written_files,
                "file_events": file_events,
            }))?
        }
    }
    Ok(())
}

async fn run_live_mode(
    format: OutputFormat,
    output_dir: Option<&str>,
    live: crate::LiveOptions,
    text_preview_chars: usize,
) -> anyhow::Result<()> {
    let email = live
        .email
        .ok_or_else(|| anyhow::anyhow!("live mode requires --email"))?;

    let servers = parse_servers_or_default(&live.servers)?;
    let config = ClientConfig {
        email,
        servers,
        server_list_path: live.server_list_path.map(PathBuf::from),
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
    let mut payload = Vec::new();
    let mut connection_events = Vec::new();
    let mut written_files = Vec::new();
    let mut file_events = Vec::new();
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
                if matches!(format, OutputFormat::Text) {
                    emit_text_line(&frame_event_to_text(&frame, text_preview_chars));
                }
                if matches!(frame, FrameEvent::DataBlock(_)) {
                    live_stats.data_blocks_total = live_stats.data_blocks_total.saturating_add(1);
                }
                if let FrameEvent::ServerListUpdate(list) = &frame {
                    live_stats.server_list_updates_total =
                        live_stats.server_list_updates_total.saturating_add(1);
                    live_stats.current_servers = list.servers.len();
                    live_stats.current_sat_servers = list.sat_servers.len();
                    connection_events.push(serde_json::json!({
                        "type": "server_list_update",
                        "servers": list.servers,
                        "sat_servers": list.sat_servers,
                    }));
                    if matches!(format, OutputFormat::Text) {
                        emit_text_line(&format!(
                            "{} server list received updates={} servers={} sat_servers={}",
                            label_info(),
                            live_stats.server_list_updates_total,
                            list.servers.len(),
                            list.sat_servers.len()
                        ));
                    }
                }
                if matches!(format, OutputFormat::Json) {
                    payload.push(frame_event_to_json(&frame, text_preview_chars));
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
                    if matches!(format, OutputFormat::Text) {
                        emit_text_line(&format!(
                            "{} wrote path={path} timestamp_utc={timestamp_utc}",
                            label_ok()
                        ));
                    }
                    written_files.push(path.clone());
                    file_events.push(serde_json::json!({
                        "filename": file.filename,
                        "path": path,
                        "timestamp_utc": timestamp_utc,
                    }));
                }
            }
            Ok(ClientEvent::Connected(endpoint)) => {
                live_stats.connections_total = live_stats.connections_total.saturating_add(1);
                if matches!(format, OutputFormat::Text) {
                    emit_text_line(&format!(
                        "{} connected endpoint={endpoint} connections={}",
                        label_ok(),
                        live_stats.connections_total
                    ));
                }
                let event = serde_json::json!({
                    "type":"connected",
                    "endpoint": endpoint,
                });
                if matches!(format, OutputFormat::Json) {
                    connection_events.push(event);
                }
            }
            Ok(ClientEvent::Disconnected) => {
                live_stats.disconnects_total = live_stats.disconnects_total.saturating_add(1);
                if matches!(format, OutputFormat::Text) {
                    emit_text_line(&format!(
                        "{} disconnected; switching server disconnects={}",
                        label_warn(),
                        live_stats.disconnects_total
                    ));
                }
                let event = serde_json::json!({
                    "type":"disconnected",
                });
                if matches!(format, OutputFormat::Json) {
                    connection_events.push(event);
                }
            }
            Ok(ClientEvent::Telemetry(snapshot)) => {
                seen += 1;
                let auth_delta = last_auth_logons
                    .map(|prev| snapshot.auth_logon_sent_total.saturating_sub(prev))
                    .unwrap_or(0);
                if auth_delta > 0 {
                    if matches!(format, OutputFormat::Text) {
                        emit_text_line(&format!(
                            "{} auth logon sent delta={} total={}",
                            label_info(),
                            auth_delta,
                            snapshot.auth_logon_sent_total
                        ));
                    }
                    if matches!(format, OutputFormat::Json) {
                        connection_events.push(serde_json::json!({
                            "type": "auth_logon_sent",
                            "delta": auth_delta,
                            "total": snapshot.auth_logon_sent_total,
                        }));
                    }
                }
                last_auth_logons = Some(snapshot.auth_logon_sent_total);

                if matches!(format, OutputFormat::Text) {
                    emit_text_line(&format!(
                        "{} bytes_in={} frames={} data_blocks={} drops={} server_lists={} servers={} sat_servers={} auth_logons={} watchdog_timeouts={} watchdog_exceptions={}",
                        label_stats(),
                        snapshot.bytes_in_total,
                        snapshot.frame_events_total,
                        live_stats.data_blocks_total,
                        snapshot.event_queue_drop_total,
                        snapshot.server_list_updates_total,
                        live_stats.current_servers,
                        live_stats.current_sat_servers,
                        snapshot.auth_logon_sent_total,
                        snapshot.watchdog_timeouts_total,
                        snapshot.watchdog_exception_events_total
                    ));
                }
                let event = serde_json::json!({
                    "type":"telemetry",
                    "snapshot": snapshot,
                    "stream_stats": {
                        "connections_total": live_stats.connections_total,
                        "disconnects_total": live_stats.disconnects_total,
                        "data_blocks_total": live_stats.data_blocks_total,
                        "server_list_updates_total": live_stats.server_list_updates_total,
                        "current_servers": live_stats.current_servers,
                        "current_sat_servers": live_stats.current_sat_servers,
                    }
                });
                if matches!(format, OutputFormat::Json) {
                    connection_events.push(event);
                }
            }
            Ok(_) => {}
            Err(err) => {
                let event = serde_json::json!({
                    "type":"error",
                    "error": err.to_string(),
                });
                if matches!(format, OutputFormat::Json) {
                    connection_events.push(event);
                }
            }
        }
    }

    drop(events);
    client.stop().await?;

    match format {
        OutputFormat::Text => {
            emit_text_line(&format!(
                "{} stream live complete events={} files={} server_lists={} servers={} sat_servers={} connections={} disconnects={}",
                label_ok(),
                seen,
                written_files.len(),
                live_stats.server_list_updates_total,
                live_stats.current_servers,
                live_stats.current_sat_servers,
                live_stats.connections_total,
                live_stats.disconnects_total
            ));
        }
        OutputFormat::Json => emit_json_line(&serde_json::json!({
            "command":"stream",
            "status":"ok",
            "mode":"live",
            "event_count": payload.len(),
            "events": payload,
            "connection_events": connection_events,
            "written_files": written_files,
            "file_events": file_events,
            "stream_stats": {
                "connections_total": live_stats.connections_total,
                "disconnects_total": live_stats.disconnects_total,
                "data_blocks_total": live_stats.data_blocks_total,
                "server_list_updates_total": live_stats.server_list_updates_total,
                "current_servers": live_stats.current_servers,
                "current_sat_servers": live_stats.current_sat_servers,
            }
        }))?,
    }

    Ok(())
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn write_completed_file(output_dir: &Path, filename: &str, data: &[u8]) -> anyhow::Result<String> {
    let target = output_dir.join(filename);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, data)?;
    Ok(target.to_string_lossy().to_string())
}

fn parse_servers_or_default(raw_servers: &[String]) -> anyhow::Result<Vec<(String, u16)>> {
    if raw_servers.is_empty() {
        return Ok(vec![
            ("emwin.weathermessage.com".to_string(), 2211),
            ("master.weathermessage.com".to_string(), 2211),
            ("emwin.interweather.net".to_string(), 1000),
            ("wxmesg.upstateweather.com".to_string(), 2211),
        ]);
    }

    raw_servers
        .iter()
        .map(|entry| {
            parse_server(entry).ok_or_else(|| {
                anyhow::anyhow!("invalid --server entry: {entry} (expected host:port)")
            })
        })
        .collect()
}
