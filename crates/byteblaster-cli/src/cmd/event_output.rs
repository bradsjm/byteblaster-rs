use crate::output::{label_event, label_warn};
use byteblaster_core::FrameEvent;

pub fn frame_event_name(event: &FrameEvent) -> &'static str {
    match event {
        FrameEvent::DataBlock(_) => "data_block",
        FrameEvent::ServerListUpdate(_) => "server_list",
        FrameEvent::Warning(_) => "warning",
        _ => "unknown",
    }
}

pub fn frame_event_filename(event: &FrameEvent) -> Option<&str> {
    match event {
        FrameEvent::DataBlock(seg) => Some(seg.filename.as_str()),
        _ => None,
    }
}

pub fn frame_event_to_text(event: &FrameEvent, text_preview_chars: usize) -> String {
    match event {
        FrameEvent::DataBlock(seg) => {
            let mut line = format!(
                "{} file={} block={}/{} bytes={}",
                label_event(),
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
            "{} server_list servers={} sat_servers={}",
            label_event(),
            list.servers.len(),
            list.sat_servers.len()
        ),
        FrameEvent::Warning(warning) => format!("{} {:?}", label_warn(), warning),
        _ => "unknown".to_string(),
    }
}

pub fn frame_event_to_json(event: &FrameEvent, text_preview_chars: usize) -> serde_json::Value {
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

pub fn text_preview(filename: &str, bytes: &[u8], max_chars: usize) -> Option<String> {
    if max_chars == 0 || !is_text_like(filename) {
        return None;
    }

    let mut normalized = String::new();
    for ch in String::from_utf8_lossy(bytes).chars() {
        if normalized.chars().count() >= max_chars {
            break;
        }
        if ch.is_ascii_graphic() {
            normalized.push(ch);
            continue;
        }
        if ch.is_ascii_whitespace() {
            normalized.push(' ');
        }
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

#[cfg(test)]
mod tests {
    use super::text_preview;

    #[test]
    fn preview_strips_non_printable_and_non_ascii() {
        let bytes = b"HELLO\x00\x1f\x7f\nWORLD\t\xf0\x9f\x98\x80";
        let preview = text_preview("sample.txt", bytes, 200).expect("preview should exist");
        assert_eq!(preview, "HELLO WORLD");
    }

    #[test]
    fn preview_returns_none_when_no_printable_content() {
        let bytes = b"\x00\x01\x02\x7f\n\t\r";
        let preview = text_preview("sample.txt", bytes, 200);
        assert_eq!(preview, None);
    }
}
