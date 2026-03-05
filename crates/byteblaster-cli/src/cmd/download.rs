//! Download command for file retrieval and assembly.
//!
//! This module provides functionality to download and assemble files from
//! capture files or live ByteBlaster servers.

use crate::live::file_pipeline::persist_completed_file;
use crate::live::shared::parse_servers_or_default;
use byteblaster_core::{
    ByteBlasterClient, Client, ClientConfig, ClientEvent, DecodeConfig, FileAssembler,
    FrameDecoder, FrameEvent, ProtocolDecoder, SegmentAssembler,
};
use futures::StreamExt;
use std::path::PathBuf;
use std::time::Duration;
use tracing::warn;

/// Runs the download command.
///
/// Downloads and assembles files from a capture file or live server
/// into the specified output directory.
///
/// # Arguments
///
/// * `output_dir` - Directory to write completed files
/// * `input` - Optional path to capture file (live mode if None)
/// * `live` - Live mode connection options
/// * `_text_preview_chars` - Unused (for API compatibility)
///
/// # Returns
///
/// Ok on success, or an error if the operation fails
pub async fn run(
    output_dir: String,
    input: Option<String>,
    live: crate::LiveOptions,
    _text_preview_chars: usize,
) -> anyhow::Result<()> {
    if let Some(input_path) = input {
        return run_capture_mode(&output_dir, &input_path);
    }

    run_live_mode(&output_dir, live).await
}

fn run_capture_mode(output_dir: &str, input_path: &str) -> anyhow::Result<()> {
    let bytes = std::fs::read(input_path)?;

    let mut decoder = ProtocolDecoder::default();
    let events = decoder.feed(&bytes)?;

    std::fs::create_dir_all(output_dir)?;
    let output_dir_path = PathBuf::from(output_dir);
    let mut assembler = FileAssembler::new(100);
    let mut written_files: Vec<String> = Vec::new();
    let mut file_events = Vec::new();

    for event in events {
        if let FrameEvent::DataBlock(segment) = event
            && let Some(file) = assembler.push(segment)?
        {
            let completed = persist_completed_file(
                output_dir_path.as_path(),
                &file.filename,
                &file.data,
                file.timestamp_utc,
            )?;
            written_files.push(completed.path);
            file_events.push(completed.event);
        }
    }

    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "command":"download",
            "status":"ok",
            "output_dir":output_dir,
            "written_files": written_files,
            "file_events": file_events,
        }))?
    );
    Ok(())
}

async fn run_live_mode(output_dir: &str, live: crate::LiveOptions) -> anyhow::Result<()> {
    let email = live
        .email
        .ok_or_else(|| anyhow::anyhow!("live mode requires --email"))?;

    let servers = parse_servers_or_default(&live.servers)?;
    let config = ClientConfig {
        email,
        servers,
        server_list_path: live.server_list_path.map(PathBuf::from),
        follow_server_list_updates: true,
        reconnect_delay_secs: 5,
        connection_timeout_secs: 5,
        watchdog_timeout_secs: 20,
        max_exceptions: 10,
        decode: DecodeConfig::default(),
    };

    std::fs::create_dir_all(output_dir)?;
    let output_dir_path = PathBuf::from(output_dir);

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
                    let completed = persist_completed_file(
                        output_dir_path.as_path(),
                        &file.filename,
                        &file.data,
                        file.timestamp_utc,
                    )?;
                    written_files.push(completed.path);
                    file_events.push(completed.event);
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
                warn!(error = %err, "download live warning");
            }
        }
    }

    drop(events);
    client.stop().await?;

    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "command":"download",
            "status":"ok",
            "mode":"live",
            "output_dir":output_dir,
            "written_files": written_files,
            "file_events": file_events,
        }))?
    );

    Ok(())
}
