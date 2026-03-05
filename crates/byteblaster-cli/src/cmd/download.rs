//! Download command for file retrieval and assembly.
//!
//! This module provides functionality to download and assemble files from
//! capture files or live ByteBlaster servers.

use crate::ReceiverKind;
use crate::live::file_pipeline::persist_completed_file;
use crate::live::shared::parse_servers_or_default;
use byteblaster_core::qbt_receiver::{
    QbtDecodeConfig, QbtFileAssembler, QbtFrameDecoder, QbtFrameEvent, QbtProtocolDecoder,
    QbtReceiver, QbtReceiverClient, QbtReceiverConfig, QbtReceiverEvent, QbtSegmentAssembler,
};
use byteblaster_core::wxwire_receiver::client::{
    WxWireReceiver, WxWireReceiverClient, WxWireReceiverEvent,
};
use byteblaster_core::wxwire_receiver::config::WxWireReceiverConfig;
use byteblaster_core::wxwire_receiver::model::WxWireReceiverFrameEvent;
use futures::StreamExt;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;
use tracing::warn;

const CAPTURE_READ_BUFFER_BYTES: usize = 64 * 1024;

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
    let mut reader = std::fs::File::open(input_path)?;
    let mut decoder = QbtProtocolDecoder::default();
    let mut buf = vec![0u8; CAPTURE_READ_BUFFER_BYTES];

    std::fs::create_dir_all(output_dir)?;
    let output_dir_path = PathBuf::from(output_dir);
    let mut assembler = QbtFileAssembler::new(100);
    let mut written_files: Vec<String> = Vec::new();
    let mut file_events = Vec::new();

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }

        let events = decoder.feed(&buf[..n])?;
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
    std::fs::create_dir_all(output_dir)?;
    let output_dir_path = PathBuf::from(output_dir);
    let mut written_files = Vec::new();
    let mut file_events = Vec::new();

    match live.receiver {
        ReceiverKind::Qbt => {
            let email = live
                .email
                .ok_or_else(|| anyhow::anyhow!("live mode requires --email"))?;
            if live.password.is_some() {
                return Err(anyhow::anyhow!(
                    "--password is not supported with --receiver qbt"
                ));
            }

            let servers = parse_servers_or_default(&live.servers)?;
            let config = QbtReceiverConfig {
                email,
                servers,
                server_list_path: live.server_list_path.map(PathBuf::from),
                follow_server_list_updates: true,
                reconnect_delay_secs: 5,
                connection_timeout_secs: 5,
                watchdog_timeout_secs: 20,
                max_exceptions: 10,
                decode: QbtDecodeConfig::default(),
            };

            let mut client = QbtReceiver::builder(config).build()?;
            client.start()?;
            let mut events = client.events();
            let mut assembler = QbtFileAssembler::new(100);
            let mut seen = 0usize;
            let idle = Duration::from_secs(live.idle_timeout_secs.max(1));

            while seen < live.max_events {
                let next = tokio::time::timeout(idle, events.next()).await;
                let Some(item) = next.ok().flatten() else {
                    break;
                };

                match item {
                    Ok(QbtReceiverEvent::Frame(QbtFrameEvent::DataBlock(segment))) => {
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
                    Ok(QbtReceiverEvent::Frame(_)) => {
                        seen += 1;
                    }
                    Ok(QbtReceiverEvent::Telemetry(_)) => {
                        seen += 1;
                    }
                    Ok(QbtReceiverEvent::Connected(_)) | Ok(QbtReceiverEvent::Disconnected) => {}
                    Ok(_) => {}
                    Err(err) => {
                        warn!(error = %err, "download live warning");
                    }
                }
            }

            drop(events);
            client.stop().await?;
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
            let mut seen = 0usize;
            let idle = Duration::from_secs(live.idle_timeout_secs.max(1));

            while seen < live.max_events {
                let next = tokio::time::timeout(idle, events.next()).await;
                let Some(item) = next.ok().flatten() else {
                    break;
                };

                match item {
                    Ok(WxWireReceiverEvent::Frame(WxWireReceiverFrameEvent::File(file))) => {
                        seen += 1;
                        let completed = persist_completed_file(
                            output_dir_path.as_path(),
                            &file.filename,
                            &file.data,
                            file.issue_utc,
                        )?;
                        written_files.push(completed.path);
                        file_events.push(completed.event);
                    }
                    Ok(WxWireReceiverEvent::Frame(_)) => {
                        seen += 1;
                    }
                    Ok(WxWireReceiverEvent::Telemetry(_)) => {
                        seen += 1;
                    }
                    Ok(WxWireReceiverEvent::Connected(_))
                    | Ok(WxWireReceiverEvent::Disconnected) => {}
                    Ok(_) => {}
                    Err(err) => {
                        warn!(error = %err, "download wxwire live warning");
                    }
                }
            }

            drop(events);
            client.stop().await?;
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
