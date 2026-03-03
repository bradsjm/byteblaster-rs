//! Output formatting and color utilities for the CLI.
//!
//! This module provides helpers for emitting formatted text and JSON output
//! with configurable color support.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

/// Output format for CLI commands.
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    /// Human-readable text output.
    Text,
    /// Machine-readable JSON output.
    Json,
}

/// Color output policy.
#[derive(Debug, Clone, Copy)]
pub enum ColorPolicy {
    /// Enable colors when stdout/stderr is a terminal.
    Auto,
    /// Always enable colors.
    Always,
    /// Never enable colors.
    Never,
}

/// Global flag for color output enabled state.
static COLOR_ENABLED: AtomicBool = AtomicBool::new(false);

/// Configures color output based on the given policy.
///
/// # Arguments
///
/// * `policy` - The color policy to apply
pub fn configure_color(policy: ColorPolicy) {
    let enabled = match policy {
        ColorPolicy::Auto => std::io::stdout().is_terminal() || std::io::stderr().is_terminal(),
        ColorPolicy::Always => true,
        ColorPolicy::Never => false,
    };
    COLOR_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Applies ANSI color codes to text if colors are enabled.
fn paint(text: &str, ansi: &str) -> String {
    if !COLOR_ENABLED.load(Ordering::Relaxed) {
        return text.to_string();
    }
    format!("\x1b[{ansi}m{text}\x1b[0m")
}

/// Creates a colored level tag like "[OK]".
fn level_tag(level: &str, ansi: &str) -> String {
    paint(&format!("[{level}]"), ansi)
}

/// Returns a green "[OK]" label.
pub fn label_ok() -> String {
    level_tag("OK", "1;32")
}

/// Returns a cyan "[INFO]" label.
pub fn label_info() -> String {
    level_tag("INFO", "1;36")
}

/// Returns a yellow "[WARN]" label.
pub fn label_warn() -> String {
    level_tag("WARN", "1;33")
}

/// Returns a red "[ERROR]" label.
pub fn label_error() -> String {
    level_tag("ERROR", "1;31")
}

/// Returns a gray "[STATS]" label.
pub fn label_stats() -> String {
    level_tag("STATS", "1;90")
}

/// Returns a blue "[EVENT]" label.
pub fn label_event() -> String {
    level_tag("EVENT", "1;34")
}

/// Emits a line of text to stdout.
pub fn emit_text_line(line: &str) {
    println!("{line}");
}

/// Emits a JSON value as a single line to stdout.
///
/// # Arguments
///
/// * `value` - The JSON value to emit
///
/// # Errors
///
/// Returns an error if JSON serialization fails.
pub fn emit_json_line(value: &serde_json::Value) -> anyhow::Result<()> {
    let serialized = serde_json::to_string(value)?;
    println!("{serialized}");
    Ok(())
}
