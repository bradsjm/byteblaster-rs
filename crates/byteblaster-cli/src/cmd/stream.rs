use crate::output::{OutputFormat, emit_json_line, emit_text_line};
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
) -> anyhow::Result<()> {
    if let Some(input_path) = input {
        return run_capture_mode(format, &input_path);
    }

    run_live_mode(format, live).await
}

fn run_capture_mode(format: OutputFormat, input_path: &str) -> anyhow::Result<()> {
    let bytes = std::fs::read(input_path)?;

    let mut decoder = ProtocolDecoder::default();
    let events = decoder.feed(&bytes)?;

    match format {
        OutputFormat::Text => {
            for event in &events {
                emit_text_line(&event_to_text(event));
            }
            emit_text_line(&format!("stream ok: {} event(s)", events.len()));
        }
        OutputFormat::Json => emit_json_line(&serde_json::json!({
            "command":"stream",
            "status":"ok",
            "event_count": events.len(),
            "events": events.iter().map(event_to_json).collect::<Vec<_>>()
        }))?,
    }
    Ok(())
}

async fn run_live_mode(format: OutputFormat, live: crate::LiveOptions) -> anyhow::Result<()> {
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
                    emit_text_line(&event_to_text(&frame));
                }
                payload.push(event_to_json(&frame));
            }
            Ok(ClientEvent::Connected(endpoint)) => {
                connection_events.push(serde_json::json!({
                    "type":"connected",
                    "endpoint": endpoint,
                }));
            }
            Ok(ClientEvent::Disconnected) => {
                connection_events.push(serde_json::json!({
                    "type":"disconnected",
                }));
            }
            Ok(ClientEvent::Telemetry(snapshot)) => {
                seen += 1;
                if matches!(format, OutputFormat::Text) {
                    emit_text_line(&format!(
                        "telemetry bytes_in={} frames={} drops={}",
                        snapshot.bytes_in_total,
                        snapshot.frame_events_total,
                        snapshot.event_queue_drop_total
                    ));
                }
                connection_events.push(serde_json::json!({
                    "type":"telemetry",
                    "snapshot": snapshot,
                }));
            }
            Ok(_) => {}
            Err(err) => {
                connection_events.push(serde_json::json!({
                    "type":"error",
                    "error": err.to_string(),
                }));
            }
        }
    }

    drop(events);
    client.stop().await?;

    match format {
        OutputFormat::Text => {
            emit_text_line(&format!("stream live ok: {} event(s)", payload.len()));
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

fn event_to_text(event: &FrameEvent) -> String {
    match event {
        FrameEvent::DataBlock(seg) => format!(
            "data_block file={} block={}/{} bytes={}",
            seg.filename,
            seg.block_number,
            seg.total_blocks,
            seg.content.len()
        ),
        FrameEvent::ServerListUpdate(list) => format!(
            "server_list servers={} sat_servers={}",
            list.servers.len(),
            list.sat_servers.len()
        ),
        FrameEvent::Warning(warning) => format!("warning {:?}", warning),
        _ => "unknown".to_string(),
    }
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
