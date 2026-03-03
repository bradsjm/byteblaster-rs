use crate::output::{OutputFormat, emit_json_line, emit_text_line};
use byteblaster_core::{FrameDecoder, FrameEvent, ProtocolDecoder};
use std::io::Read;

pub async fn run(format: OutputFormat, input: Option<String>) -> anyhow::Result<()> {
    let bytes = read_input(input.as_deref())?;

    let mut decoder = ProtocolDecoder::default();
    let events = decoder.feed(&bytes)?;

    match format {
        OutputFormat::Text => {
            emit_text_line(&format!("inspect ok: {} event(s)", events.len()));
        }
        OutputFormat::Json => {
            let event_payload: Vec<serde_json::Value> = events.iter().map(event_to_json).collect();
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

fn event_to_json(event: &FrameEvent) -> serde_json::Value {
    match event {
        FrameEvent::DataBlock(seg) => serde_json::json!({
            "type":"data_block",
            "filename":seg.filename,
            "block_number":seg.block_number,
            "total_blocks":seg.total_blocks,
            "length":seg.content.len(),
            "version": format!("{:?}", seg.version),
        }),
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
