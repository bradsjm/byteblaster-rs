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
        ColorPolicy::Auto => std::io::stdout().is_terminal(),
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

pub fn style_ok(text: &str) -> String {
    paint(text, "32")
}

pub fn style_warn(text: &str) -> String {
    paint(text, "33")
}

pub fn style_meta(text: &str) -> String {
    paint(text, "36")
}

pub fn style_dim(text: &str) -> String {
    paint(text, "90")
}

pub fn emit_text_line(line: &str) {
    println!("{line}");
}

pub fn emit_json_line(value: &serde_json::Value) -> anyhow::Result<()> {
    let serialized = serde_json::to_string(value)?;
    println!("{serialized}");
    Ok(())
}
