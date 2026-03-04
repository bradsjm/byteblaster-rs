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
