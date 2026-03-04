//! Download command for file retrieval and assembly.
//!
//! This module provides functionality to download and assemble files from
//! capture files or live ByteBlaster servers.

use crate::output::{OutputFormat, emit_json_line, emit_text_line, label_ok, label_warn};
use byteblaster_core::{
    ByteBlasterClient, Client, ClientConfig, ClientEvent, DecodeConfig, FileAssembler,
    FrameDecoder, FrameEvent, ProtocolDecoder, SegmentAssembler, parse_server,
};
use futures::StreamExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Runs the download command.
///
/// Downloads and assembles files from a capture file or live server
/// into the specified output directory.
///
/// # Arguments
///
/// * `format` - Output format for status messages
/// * `output_dir` - Directory to write completed files
/// * `input` - Optional path to capture file (live mode if None)
/// * `live` - Live mode connection options
/// * `_text_preview_chars` - Unused (for API compatibility)
///
/// # Returns
///
/// Ok on success, or an error if the operation fails
pub async fn run(
    format: OutputFormat,
    output_dir: String,
    input: Option<String>,
    live: crate::LiveOptions,
    _text_preview_chars: usize,
) -> anyhow::Result<()> {
    if let Some(input_path) = input {
        return run_capture_mode(format, &output_dir, &input_path);
    }

    run_live_mode(format, &output_dir, live).await
}

fn run_capture_mode(
    format: OutputFormat,
    output_dir: &str,
    input_path: &str,
) -> anyhow::Result<()> {
    let bytes = std::fs::read(input_path)?;

    let mut decoder = ProtocolDecoder::default();
    let events = decoder.feed(&bytes)?;

    std::fs::create_dir_all(output_dir)?;
    let mut assembler = FileAssembler::new(100);
    let mut written_files: Vec<String> = Vec::new();
    let mut file_events = Vec::new();

    for event in events {
        if let FrameEvent::DataBlock(segment) = event
            && let Some(file) = assembler.push(segment)?
        {
            let target = Path::new(output_dir).join(&file.filename);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, &file.data)?;
            let path = target.to_string_lossy().to_string();
            let timestamp_utc = unix_seconds(file.timestamp_utc);
            written_files.push(path.clone());
            file_events.push(serde_json::json!({
                "filename": file.filename,
                "path": path,
                "timestamp_utc": timestamp_utc,
            }));
        }
    }

    match format {
        OutputFormat::Text => {
            for file_event in &file_events {
                let path = file_event["path"].as_str().unwrap_or("");
                let timestamp_utc = file_event["timestamp_utc"].as_u64().unwrap_or(0);
                emit_text_line(&format!(
                    "{} wrote path={path} timestamp_utc={timestamp_utc}",
                    label_ok()
                ));
            }
            emit_text_line(&format!(
                "{} download capture complete files={}",
                label_ok(),
                written_files.len()
            ));
        }
        OutputFormat::Json => emit_json_line(&serde_json::json!({
            "command":"download",
            "status":"ok",
            "output_dir":output_dir,
            "written_files": written_files,
            "file_events": file_events,
        }))?,
    }
    Ok(())
}

async fn run_live_mode(
    format: OutputFormat,
    output_dir: &str,
    live: crate::LiveOptions,
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
        watchdog_timeout_secs: 20,
        max_exceptions: 10,
        decode: DecodeConfig::default(),
    };

    std::fs::create_dir_all(output_dir)?;

    let mut client = Client::builder(config).build()?;
    client.start()?;
    let mut events = client.events();
    let mut assembler = FileAssembler::new(100);
    let mut written_files = Vec::new();
    let mut file_events = Vec::new();
    let mut seen = 0usize;
    let idle = Duration::from_secs(live.idle_timeout_secs.max(1));

    while seen < live.max_events {
        let next = tokio::time::timeout(idle, events.next()).await;
        let Some(item) = next.ok().flatten() else {
            break;
        };

        match item {
            Ok(ClientEvent::Frame(FrameEvent::DataBlock(segment))) => {
                seen += 1;
                if let Some(file) = assembler.push(segment)? {
                    let target = Path::new(output_dir).join(&file.filename);
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&target, &file.data)?;
                    let path = target.to_string_lossy().to_string();
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
            Ok(ClientEvent::Frame(_)) => {
                seen += 1;
            }
            Ok(ClientEvent::Telemetry(_)) => {
                seen += 1;
            }
            Ok(ClientEvent::Connected(_)) | Ok(ClientEvent::Disconnected) => {}
            Ok(_) => {}
            Err(err) => {
                eprintln!("{} download live warning: {err}", label_warn());
            }
        }
    }

    drop(events);
    client.stop().await?;

    match format {
        OutputFormat::Text => {
            emit_text_line(&format!(
                "{} download live complete files={}",
                label_ok(),
                written_files.len()
            ));
        }
        OutputFormat::Json => emit_json_line(&serde_json::json!({
            "command":"download",
            "status":"ok",
            "mode":"live",
            "output_dir":output_dir,
            "written_files": written_files,
            "file_events": file_events,
        }))?,
    }

    Ok(())
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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
