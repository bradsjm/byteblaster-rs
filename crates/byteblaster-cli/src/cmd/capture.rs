use byteblaster_core::qbt_receiver::{QbtFrameDecoder, QbtFrameEvent, QbtProtocolDecoder};
use std::io::Read;

const CAPTURE_READ_BUFFER_BYTES: usize = 64 * 1024;

pub(crate) fn decode_capture_events(path: Option<&str>) -> anyhow::Result<Vec<QbtFrameEvent>> {
    let mut reader: Box<dyn Read> = if let Some(path) = path {
        Box::new(std::fs::File::open(path)?)
    } else {
        Box::new(std::io::stdin())
    };

    decode_capture_events_from_reader(&mut reader)
}

pub(crate) fn decode_capture_events_from_reader(
    reader: &mut dyn Read,
) -> anyhow::Result<Vec<QbtFrameEvent>> {
    let mut decoder = QbtProtocolDecoder::default();
    let mut events = Vec::new();
    let mut buf = vec![0u8; CAPTURE_READ_BUFFER_BYTES];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        events.extend(decoder.feed(&buf[..n])?);
    }

    Ok(events)
}
