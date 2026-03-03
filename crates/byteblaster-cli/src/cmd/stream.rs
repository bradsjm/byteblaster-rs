use crate::output::{
    OutputFormat, emit_json_line, emit_text_line, style_dim, style_meta, style_ok, style_warn,
};
use byteblaster_core::{
    ByteBlasterClient, Client, ClientConfig, ClientEvent, DecodeConfig, FrameDecoder, FrameEvent,
    ProtocolDecoder, parse_server,
};
use futures::StreamExt;
use std::path::PathBuf;
use std::time::Duration;

pub async fn run(
    format: OutputFormat,
    input: Option<String>,
    live: crate::LiveOptions,
    text_preview_chars: usize,
) -> anyhow::Result<()> {
    if let Some(input_path) = input {
        return run_capture_mode(format, &input_path, text_preview_chars);
    }

    run_live_mode(format, live, text_preview_chars).await
}

fn run_capture_mode(
    format: OutputFormat,
    input_path: &str,
    text_preview_chars: usize,
) -> anyhow::Result<()> {
    let bytes = std::fs::read(input_path)?;

    let mut decoder = ProtocolDecoder::default();
    let events = decoder.feed(&bytes)?;

    match format {
        OutputFormat::Text => {
            for event in &events {
                emit_text_line(&event_to_text(event, text_preview_chars));
            }
            emit_text_line(&format!(
                "{} {} event(s)",
                style_ok("stream ok:"),
                events.len()
            ));
        }
        OutputFormat::Json => emit_json_line(&serde_json::json!({
            "command":"stream",
            "status":"ok",
            "event_count": events.len(),
            "events": events
                .iter()
                .map(|event| event_to_json(event, text_preview_chars))
                .collect::<Vec<_>>()
        }))?,
    }
    Ok(())
}

async fn run_live_mode(
    format: OutputFormat,
    live: crate::LiveOptions,
    text_preview_chars: usize,
) -> anyhow::Result<()> {
    let email = live
        .email
        .ok_or_else(|| anyhow::anyhow!("live mode requires --email"))?;

    let servers = parse_servers_or_default(&live.servers)?;
    let config = ClientConfig {
        email,
        servers,
        server_list_path: live.server_list_path.map(PathBuf::from),
        reconnect_delay_secs: 5,
        connection_timeout_secs: 5,
        watchdog_timeout_secs: 20,
        max_exceptions: 10,
        decode: DecodeConfig::default(),
    };

    let mut client = Client::builder(config).build()?;
    client.start()?;
    let mut events = client.events();

    let mut seen = 0usize;
    let mut payload = Vec::new();
    let mut connection_events = Vec::new();
    let idle = Duration::from_secs(live.idle_timeout_secs.max(1));

    while seen < live.max_events {
        let next = tokio::time::timeout(idle, events.next()).await;
        let Some(item) = next.ok().flatten() else {
            break;
        };

        match item {
            Ok(ClientEvent::Frame(frame)) => {
                seen += 1;
                if matches!(format, OutputFormat::Text) {
                    emit_text_line(&event_to_text(&frame, text_preview_chars));
                }
                if let FrameEvent::ServerListUpdate(list) = &frame {
                    connection_events.push(serde_json::json!({
                        "type": "server_list_update",
                        "servers": list.servers,
                        "sat_servers": list.sat_servers,
                    }));
                    if matches!(format, OutputFormat::Text) {
                        emit_text_line(&format!(
                            "{} servers={} sat_servers={}",
                            style_meta("server list update received"),
                            list.servers.len(),
                            list.sat_servers.len()
                        ));
                    }
                }
                if matches!(format, OutputFormat::Json) {
                    payload.push(event_to_json(&frame, text_preview_chars));
                }
            }
            Ok(ClientEvent::Connected(endpoint)) => {
                if matches!(format, OutputFormat::Text) {
                    emit_text_line(&format!("{} {endpoint}", style_ok("connected to")));
                }
                let event = serde_json::json!({
                    "type":"connected",
                    "endpoint": endpoint,
                });
                if matches!(format, OutputFormat::Json) {
                    connection_events.push(event);
                }
            }
            Ok(ClientEvent::Disconnected) => {
                if matches!(format, OutputFormat::Text) {
                    emit_text_line(&style_warn("disconnected; switching server"));
                }
                let event = serde_json::json!({
                    "type":"disconnected",
                });
                if matches!(format, OutputFormat::Json) {
                    connection_events.push(event);
                }
            }
            Ok(ClientEvent::Telemetry(snapshot)) => {
                seen += 1;
                if matches!(format, OutputFormat::Text) {
                    emit_text_line(&format!(
                        "{} bytes_in={} frames={} drops={}",
                        style_dim("telemetry"),
                        snapshot.bytes_in_total,
                        snapshot.frame_events_total,
                        snapshot.event_queue_drop_total
                    ));
                }
                let event = serde_json::json!({
                    "type":"telemetry",
                    "snapshot": snapshot,
                });
                if matches!(format, OutputFormat::Json) {
                    connection_events.push(event);
                }
            }
            Ok(_) => {}
            Err(err) => {
                let event = serde_json::json!({
                    "type":"error",
                    "error": err.to_string(),
                });
                if matches!(format, OutputFormat::Json) {
                    connection_events.push(event);
                }
            }
        }
    }

    drop(events);
    client.stop().await?;

    match format {
        OutputFormat::Text => {
            emit_text_line(&format!(
                "{} {} event(s)",
                style_ok("stream live ok:"),
                payload.len()
            ));
        }
        OutputFormat::Json => emit_json_line(&serde_json::json!({
            "command":"stream",
            "status":"ok",
            "mode":"live",
            "event_count": payload.len(),
            "events": payload,
            "connection_events": connection_events,
        }))?,
    }

    Ok(())
}

fn parse_servers_or_default(raw_servers: &[String]) -> anyhow::Result<Vec<(String, u16)>> {
    if raw_servers.is_empty() {
        return Ok(vec![
            ("emwin.weathermessage.com".to_string(), 2211),
            ("master.weathermessage.com".to_string(), 2211),
            ("emwin.interweather.net".to_string(), 1000),
            ("wxmesg.upstateweather.com".to_string(), 2211),
        ]);
    }

    raw_servers
        .iter()
        .map(|entry| {
            parse_server(entry).ok_or_else(|| {
                anyhow::anyhow!("invalid --server entry: {entry} (expected host:port)")
            })
        })
        .collect()
}

fn event_to_text(event: &FrameEvent, text_preview_chars: usize) -> String {
    match event {
        FrameEvent::DataBlock(seg) => {
            let mut line = format!(
                "{} file={} block={}/{} bytes={}",
                style_meta("data_block"),
                seg.filename,
                seg.block_number,
                seg.total_blocks,
                seg.content.len()
            );
            if let Some(preview) = text_preview(&seg.filename, &seg.content, text_preview_chars) {
                line.push_str(&format!(" preview={preview:?}"));
            }
            line
        }
        FrameEvent::ServerListUpdate(list) => format!(
            "{} servers={} sat_servers={}",
            style_meta("server_list"),
            list.servers.len(),
            list.sat_servers.len()
        ),
        FrameEvent::Warning(warning) => format!("{} {:?}", style_warn("warning"), warning),
        _ => "unknown".to_string(),
    }
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
