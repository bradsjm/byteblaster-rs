mod capture;
mod common;
mod qbt;
mod wxwire;

use std::path::PathBuf;

pub async fn run(
    input: Option<String>,
    output_dir: Option<String>,
    live: crate::LiveOptions,
    text_preview_chars: usize,
) -> anyhow::Result<()> {
    if let Some(input_path) = input {
        return capture::run_capture_mode(&input_path, output_dir.as_deref(), text_preview_chars);
    }

    let output_dir_path = output_dir.map(PathBuf::from);
    if let Some(path) = &output_dir_path {
        std::fs::create_dir_all(path)?;
    }

    match live.receiver {
        crate::ReceiverKind::Qbt => {
            qbt::run_qbt_live_mode(output_dir_path.as_deref(), live, text_preview_chars).await
        }
        crate::ReceiverKind::Wxwire => {
            wxwire::run_wxwire_live_mode(output_dir_path.as_deref(), live, text_preview_chars).await
        }
    }
}
