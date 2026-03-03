use crate::output::{OutputFormat, emit_json_line, emit_text_line};
use byteblaster_core::{FrameDecoder, FrameEvent, ProtocolDecoder};
use std::io::Read;

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
            emit_text_line(&format!("inspect ok: {} event(s)", events.len()));
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

fn read_input(path: Option<&str>) -> anyhow::Result<Vec<u8>> {
    if let Some(path) = path {
        return Ok(std::fs::read(path)?);
    }

    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes)?;
    Ok(bytes)
}

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

fn is_text_like(filename: &str) -> bool {
    let upper = filename.to_ascii_uppercase();
    upper.ends_with(".TXT")
        || upper.ends_with(".WMO")
        || upper.ends_with(".XML")
        || upper.ends_with(".JSON")
}
