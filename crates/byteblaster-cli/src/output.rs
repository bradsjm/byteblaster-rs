use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy)]
pub enum ColorPolicy {
    Auto,
    Always,
    Never,
}

static COLOR_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn configure_color(policy: ColorPolicy) {
    let enabled = match policy {
        ColorPolicy::Auto => std::io::stdout().is_terminal() || std::io::stderr().is_terminal(),
        ColorPolicy::Always => true,
        ColorPolicy::Never => false,
    };
    COLOR_ENABLED.store(enabled, Ordering::Relaxed);
}

fn paint(text: &str, ansi: &str) -> String {
    if !COLOR_ENABLED.load(Ordering::Relaxed) {
        return text.to_string();
    }
    format!("\x1b[{ansi}m{text}\x1b[0m")
}

fn level_tag(level: &str, ansi: &str) -> String {
    paint(&format!("[{level}]"), ansi)
}

pub fn label_ok() -> String {
    level_tag("OK", "1;32")
}

pub fn label_info() -> String {
    level_tag("INFO", "1;36")
}

pub fn label_warn() -> String {
    level_tag("WARN", "1;33")
}

pub fn label_error() -> String {
    level_tag("ERROR", "1;31")
}

pub fn label_stats() -> String {
    level_tag("STATS", "1;90")
}

pub fn label_event() -> String {
    level_tag("EVENT", "1;34")
}

pub fn emit_text_line(line: &str) {
    println!("{line}");
}

pub fn emit_json_line(value: &serde_json::Value) -> anyhow::Result<()> {
    let serialized = serde_json::to_string(value)?;
    println!("{serialized}");
    Ok(())
}
