//! Inspect command for decoding capture files.
//!
//! This module provides functionality to read and decode ByteBlaster
//! capture files, outputting the decoded events in text or JSON format.

use crate::output::{OutputFormat, emit_json_line, emit_text_line, label_ok};
use byteblaster_core::{FrameDecoder, FrameEvent, ProtocolDecoder};
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

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
                .map(|event| event_to_json(event, text_preview_chars))
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

/// Converts a frame event to JSON representation.
fn event_to_json(event: &FrameEvent, text_preview_chars: usize) -> serde_json::Value {
    match event {
        FrameEvent::DataBlock(seg) => {
            let mut value = serde_json::json!({
                "type":"data_block",
                "filename":seg.filename,
                "block_number":seg.block_number,
                "total_blocks":seg.total_blocks,
                "length":seg.content.len(),
                "version": format!("{:?}", seg.version),
                "timestamp_utc": unix_seconds(seg.timestamp_utc),
            });
            if let Some(preview) = text_preview(&seg.filename, &seg.content, text_preview_chars) {
                value["preview"] = serde_json::Value::String(preview);
            }
            value
        }
        FrameEvent::ServerListUpdate(list) => serde_json::json!({
            "type":"server_list",
            "servers": list.servers,
            "sat_servers": list.sat_servers,
        }),
        FrameEvent::Warning(w) => serde_json::json!({
            "type":"warning",
            "warning": format!("{:?}", w),
        }),
        _ => serde_json::json!({
            "type":"unknown",
        }),
    }
}

/// Generates a text preview for text-like files.
fn text_preview(filename: &str, bytes: &[u8], max_chars: usize) -> Option<String> {
    if max_chars == 0 || !is_text_like(filename) {
        return None;
    }

    let mut normalized = String::new();
    for ch in String::from_utf8_lossy(bytes).chars() {
        if normalized.chars().count() >= max_chars {
            break;
        }
        if ch.is_control() {
            if ch == '\n' || ch == '\r' || ch == '\t' {
                normalized.push(' ');
            }
            continue;
        }
        normalized.push(ch);
    }

    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

/// Checks if a filename indicates a text-like file type.
fn is_text_like(filename: &str) -> bool {
    let upper = filename.to_ascii_uppercase();
    upper.ends_with(".TXT")
        || upper.ends_with(".WMO")
        || upper.ends_with(".XML")
        || upper.ends_with(".JSON")
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
