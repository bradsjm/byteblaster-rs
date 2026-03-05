//! Inspect command for decoding capture files.
//!
//! This module provides functionality to read and decode ByteBlaster
//! capture files, outputting decoded events as JSON.

use crate::cmd::event_output::frame_event_to_json;
use byteblaster_core::qbt_receiver::{QbtFrameDecoder, QbtFrameEvent, QbtProtocolDecoder};
use std::io::Read;

const CAPTURE_READ_BUFFER_BYTES: usize = 64 * 1024;

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
pub async fn run(input: Option<String>, text_preview_chars: usize) -> anyhow::Result<()> {
    let events = decode_input(input.as_deref())?;

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

/// Reads and decodes input from a file path or stdin.
fn decode_input(path: Option<&str>) -> anyhow::Result<Vec<QbtFrameEvent>> {
    let mut reader: Box<dyn Read> = if let Some(path) = path {
        Box::new(std::fs::File::open(path)?)
    } else {
        Box::new(std::io::stdin())
    };

    let mut decoder = QbtProtocolDecoder::default();
    let mut events = Vec::new();
    let mut buf = vec![0u8; CAPTURE_READ_BUFFER_BYTES];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        events.extend(decoder.feed(&buf[..n])?);
    }

    Ok(events)
}
