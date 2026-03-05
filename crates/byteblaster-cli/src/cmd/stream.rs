//! Stream command for event streaming.
//!
//! This module provides the `stream` command that decodes and streams events
//! from capture files or live ByteBlaster servers.
//!
//! The stream command:
//! - Supports capture file mode (decode from file) and live mode (connect to servers)
//! - Emits structured logs to stderr (no JSON payloads)
//! - Optionally writes completed files to disk with `--output-dir`
//! - Continues until max events limit, idle timeout, or shutdown
//!
//! Implementation is delegated to `crate::live::stream`.

use crate::LiveOptions;
use crate::live;

pub async fn run(
    input: Option<String>,
    output_dir: Option<String>,
    live_options: LiveOptions,
    text_preview_chars: usize,
) -> anyhow::Result<()> {
    live::stream::run(input, output_dir, live_options, text_preview_chars).await
}
