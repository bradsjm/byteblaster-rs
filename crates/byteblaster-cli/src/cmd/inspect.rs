//! Inspect command for decoding capture files.
//!
//! This module provides functionality to read and decode ByteBlaster
//! capture files, outputting the decoded events in text or JSON format.

use crate::cmd::event_output::frame_event_to_json;
use crate::output::{OutputFormat, emit_json_line, emit_text_line, label_ok};
use byteblaster_core::{FrameDecoder, ProtocolDecoder};
use std::io::Read;

/// Runs the inspect command.
///
/// Reads a capture file (or stdin) and decodes all frames, outputting
/// the results in the specified format.
///
/// # Arguments
///
/// * `format` - Output format (text or JSON)
/// * `input` - Optional path to capture file (reads stdin if None)
/// * `text_preview_chars` - Maximum characters for text content preview
///
/// # Returns
///
/// Ok on success, or an error if reading/decoding fails
pub async fn run(
    format: OutputFormat,
    input: Option<String>,
    text_preview_chars: usize,
) -> anyhow::Result<()> {
    let bytes = read_input(input.as_deref())?;

    let mut decoder = ProtocolDecoder::default();
    let events = decoder.feed(&bytes)?;

    match format {
        OutputFormat::Text => {
            emit_text_line(&format!(
                "{} inspect complete events={}",
                label_ok(),
                events.len()
            ));
        }
        OutputFormat::Json => {
            let event_payload: Vec<serde_json::Value> = events
                .iter()
                .map(|event| frame_event_to_json(event, text_preview_chars))
                .collect();
            emit_json_line(&serde_json::json!({
                "command":"inspect",
                "status":"ok",
                "event_count": event_payload.len(),
                "events": event_payload,
            }))?;
        }
    }

    Ok(())
}

/// Reads input from a file path or stdin.
fn read_input(path: Option<&str>) -> anyhow::Result<Vec<u8>> {
    if let Some(path) = path {
        return Ok(std::fs::read(path)?);
    }

    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes)?;
    Ok(bytes)
}
