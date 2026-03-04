use crate::LiveOptions;
use crate::OutputFormat;
use crate::live;

pub async fn run(
    format: OutputFormat,
    input: Option<String>,
    output_dir: Option<String>,
    live_options: LiveOptions,
    text_preview_chars: usize,
) -> anyhow::Result<()> {
    live::stream::run(format, input, output_dir, live_options, text_preview_chars).await
}
