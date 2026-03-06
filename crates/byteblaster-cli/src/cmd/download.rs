//! Download command for file retrieval and assembly.
//!
//! This module provides functionality to download and assemble files from
//! capture files or live ByteBlaster servers.

use crate::ReceiverKind;
use crate::cmd::capture::decode_capture_events;
use crate::live::config::{LiveConfigRequest, LiveReceiverConfig, build_live_receiver_config};
use crate::live::file_pipeline::persist_completed_file;
use byteblaster_core::ingest::{IngestEvent, QbtIngestStream, WxWireIngestStream};
use byteblaster_core::qbt_receiver::{
    QbtFileAssembler, QbtFrameEvent, QbtReceiver, QbtSegmentAssembler,
};
use byteblaster_core::wxwire_receiver::WxWireReceiver;
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
) -> crate::error::CliResult<()> {
    if let Some(input_path) = input {
        return run_capture_mode(&output_dir, &input_path);
    }

    run_live_mode(&output_dir, live).await
}

fn run_capture_mode(output_dir: &str, input_path: &str) -> crate::error::CliResult<()> {
    std::fs::create_dir_all(output_dir)?;
    let output_dir_path = PathBuf::from(output_dir);
    let mut assembler = QbtFileAssembler::new(100);
    let mut written_files: Vec<String> = Vec::new();
    let mut file_events = Vec::new();
    let events = decode_capture_events(Some(input_path))?;
    for event in events {
        if let QbtFrameEvent::DataBlock(segment) = event
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

async fn run_live_mode(output_dir: &str, live: crate::LiveOptions) -> crate::error::CliResult<()> {
    std::fs::create_dir_all(output_dir)?;
    let output_dir_path = PathBuf::from(output_dir);
    let mut written_files = Vec::new();
    let mut file_events = Vec::new();

    match live.receiver {
        ReceiverKind::Qbt => {
            let LiveReceiverConfig::Qbt(config) = build_live_receiver_config(LiveConfigRequest {
                receiver: ReceiverKind::Qbt,
                username: live.username.clone(),
                password: live.password.clone(),
                raw_servers: live.servers.clone(),
                server_list_path: live.server_list_path.clone(),
                idle_timeout_secs: live.idle_timeout_secs,
                qbt_watchdog_timeout_secs: 20,
                username_context: "live mode",
                password_context: "live mode",
            })?
            else {
                unreachable!("qbt download must build qbt config");
            };

            let receiver = QbtReceiver::builder(config).build()?;
            let mut ingest = QbtIngestStream::new(receiver);
            ingest.start()?;
            let mut events = ingest.events()?;
            let mut seen = 0usize;
            let idle = Duration::from_secs(live.idle_timeout_secs.max(1));

            while seen < live.max_events {
                let next = tokio::time::timeout(idle, events.next()).await;
                let Some(item) = next.ok().flatten() else {
                    break;
                };

                match item {
                    Ok(IngestEvent::Product(product)) => {
                        seen += 1;
                        let completed = persist_completed_file(
                            output_dir_path.as_path(),
                            &product.filename,
                            &product.data,
                            product.source_timestamp_utc,
                        )?;
                        written_files.push(completed.path);
                        file_events.push(completed.event);
                    }
                    Ok(IngestEvent::Telemetry(_)) | Ok(IngestEvent::Warning(_)) => {
                        seen += 1;
                    }
                    Ok(IngestEvent::Connected { .. }) | Ok(IngestEvent::Disconnected) => {}
                    Ok(_) => {}
                    Err(err) => {
                        warn!(error = %err, "download live warning");
                    }
                }
            }

            drop(events);
            ingest.stop().await?;
        }
        ReceiverKind::Wxwire => {
            let LiveReceiverConfig::WxWire(config) =
                build_live_receiver_config(LiveConfigRequest {
                    receiver: ReceiverKind::Wxwire,
                    username: live.username.clone(),
                    password: live.password.clone(),
                    raw_servers: live.servers.clone(),
                    server_list_path: live.server_list_path.clone(),
                    idle_timeout_secs: live.idle_timeout_secs,
                    qbt_watchdog_timeout_secs: 0,
                    username_context: "wxwire live mode",
                    password_context: "wxwire live mode",
                })?
            else {
                unreachable!("wxwire download must build wxwire config");
            };

            let receiver = WxWireReceiver::builder(config).build()?;
            let mut ingest = WxWireIngestStream::new(receiver);
            ingest.start()?;
            let mut events = ingest.events()?;
            let mut seen = 0usize;
            let idle = Duration::from_secs(live.idle_timeout_secs.max(1));

            while seen < live.max_events {
                let next = tokio::time::timeout(idle, events.next()).await;
                let Some(item) = next.ok().flatten() else {
                    break;
                };

                match item {
                    Ok(IngestEvent::Product(product)) => {
                        seen += 1;
                        let completed = persist_completed_file(
                            output_dir_path.as_path(),
                            &product.filename,
                            &product.data,
                            product.source_timestamp_utc,
                        )?;
                        written_files.push(completed.path);
                        file_events.push(completed.event);
                    }
                    Ok(IngestEvent::Telemetry(_)) | Ok(IngestEvent::Warning(_)) => {
                        seen += 1;
                    }
                    Ok(IngestEvent::Connected { .. }) | Ok(IngestEvent::Disconnected) => {}
                    Ok(_) => {}
                    Err(err) => {
                        warn!(error = %err, "download wxwire live warning");
                    }
                }
            }

            drop(events);
            ingest.stop().await?;
        }
    }

    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "command":"download",
            "status":"ok",
            "mode":"live",
            "receiver": format!("{:?}", live.receiver).to_ascii_lowercase(),
            "output_dir":output_dir,
            "written_files": written_files,
            "file_events": file_events,
        }))?
    );

    Ok(())
}
