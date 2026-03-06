use crate::live::file_pipeline::persist_completed_file;
use crate::live::stream::common::{log_completed_file, log_frame_event};
use byteblaster_core::qbt_receiver::{
    QbtFileAssembler, QbtFrameDecoder, QbtFrameEvent, QbtProtocolDecoder, QbtSegmentAssembler,
};
use std::io::Read;
use std::path::PathBuf;
use tracing::info;

const CAPTURE_READ_BUFFER_BYTES: usize = 64 * 1024;

pub(super) fn run_capture_mode(
    input_path: &str,
    output_dir: Option<&str>,
    text_preview_chars: usize,
) -> crate::error::CliResult<()> {
    let mut reader = std::fs::File::open(input_path)?;
    let mut buf = vec![0u8; CAPTURE_READ_BUFFER_BYTES];
    let mut decoder = QbtProtocolDecoder::default();
    let output_dir_path = output_dir.map(PathBuf::from);
    if let Some(path) = &output_dir_path {
        std::fs::create_dir_all(path)?;
    }
    let mut assembler = output_dir_path.as_ref().map(|_| QbtFileAssembler::new(100));
    let mut written_files = Vec::new();
    let mut events_total = 0usize;

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }

        let events = decoder.feed(&buf[..n])?;
        for event in events {
            events_total += 1;
            log_frame_event(&event, text_preview_chars);
            if let Some(assembler) = assembler.as_mut()
                && let QbtFrameEvent::DataBlock(segment) = event
                && let Some(file) = assembler.push(segment)?
            {
                let completed = persist_completed_file(
                    output_dir_path
                        .as_deref()
                        .expect("output dir configured when assembler enabled"),
                    &file.filename,
                    &file.data,
                    file.timestamp_utc,
                )?;
                log_completed_file(&completed);
                written_files.push(completed.path);
            }
        }
    }

    info!(
        events = events_total,
        files = written_files.len(),
        status = "ok",
        "stream capture complete"
    );

    Ok(())
}
