//! Download command for file retrieval and assembly.
//!
//! This module provides functionality to download and assemble files from
//! live ByteBlaster servers.

use crate::live::file_pipeline::persist_completed_file;
use crate::live::ingest::{LiveIngest, LiveIngestRequest};
use byteblaster_core::ingest::IngestEvent;
use futures::StreamExt;
use std::path::PathBuf;
use std::time::Duration;
use tracing::warn;

/// Runs the download command.
///
/// Downloads and assembles files from a live server into the specified output
/// directory.
///
/// # Arguments
///
/// * `output_dir` - Directory to write completed files
/// * `live` - Live mode connection options
/// * `_text_preview_chars` - Unused (for API compatibility)
///
/// # Returns
///
/// Ok on success, or an error if the operation fails
pub async fn run(
    output_dir: String,
    live: crate::LiveOptions,
    _text_preview_chars: usize,
) -> crate::error::CliResult<()> {
    run_live_mode(&output_dir, live).await
}

async fn run_live_mode(output_dir: &str, live: crate::LiveOptions) -> crate::error::CliResult<()> {
    std::fs::create_dir_all(output_dir)?;
    let output_dir_path = PathBuf::from(output_dir);
    let mut written_files = Vec::new();
    let mut file_events = Vec::new();
    let mut ingest = LiveIngest::build(LiveIngestRequest {
        live: &live,
        qbt_watchdog_timeout_secs: 20,
        username_context: "live mode",
        password_context: "live mode",
    })?;
    let receiver = ingest.receiver_kind();

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
                file_events.push(completed.metadata);
            }
            Ok(IngestEvent::Telemetry(_)) | Ok(IngestEvent::Warning(_)) => {
                seen += 1;
            }
            Ok(IngestEvent::Connected { .. }) | Ok(IngestEvent::Disconnected) => {}
            Ok(_) => {}
            Err(err) => {
                warn!(error = %err, "download warning");
            }
        }
    }

    drop(events);
    ingest.stop().await?;

    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "command":"download",
            "status":"ok",
            "mode":"live",
            "receiver": format!("{receiver:?}").to_ascii_lowercase(),
            "output_dir":output_dir,
            "written_files": written_files,
            "file_events": file_events,
        }))?
    );

    Ok(())
}
