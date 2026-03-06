//! Inspect command for decoding capture files.
//!
//! This module provides functionality to read and decode ByteBlaster
//! capture files, outputting decoded events as JSON.

use crate::cmd::capture::decode_capture_events;
use crate::cmd::event_output::frame_event_to_json;

/// Runs the inspect command.
///
/// Reads a capture file (or stdin) and decodes all frames, outputting
/// the results in the specified format.
///
/// # Arguments
///
/// * `input` - Optional path to capture file (reads stdin if None)
/// * `text_preview_chars` - Maximum characters for text content preview
///
/// # Returns
///
/// Ok on success, or an error if reading/decoding fails
pub async fn run(input: Option<String>, text_preview_chars: usize) -> crate::error::CliResult<()> {
    let events = decode_capture_events(input.as_deref())?;

    let event_payload: Vec<serde_json::Value> = events
        .iter()
        .map(|event| frame_event_to_json(event, text_preview_chars))
        .collect();
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "command":"inspect",
            "status":"ok",
            "event_count": event_payload.len(),
            "events": event_payload,
        }))?
    );

    Ok(())
}
