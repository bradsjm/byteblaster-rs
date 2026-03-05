use byteblaster_core::qbt_receiver::{
    QbtFrameDecoder, QbtFrameEvent, QbtProtocolDecoder, calculate_qbt_checksum,
};
use proptest::prelude::*;

const SYNC: &[u8; 6] = b"\0\0\0\0\0\0";

fn xor_encode(input: &[u8]) -> Vec<u8> {
    input.iter().map(|b| b ^ 0xFF).collect()
}

fn build_header(filename: &str, block: u32, total: u32, checksum: u32) -> [u8; 80] {
    let mut raw = format!(
        "/PF{filename} /PN {block} /PT {total} /CS {checksum} /FD01/01/2024 01:00:00 AM\r\n"
    );
    while raw.len() < 80 {
        raw.push(' ');
    }

    let mut out = [0u8; 80];
    out.copy_from_slice(&raw.as_bytes()[..80]);
    out
}

fn frame_with_body(filename: &str, body: &[u8]) -> Vec<u8> {
    let checksum = u32::from(calculate_qbt_checksum(body));
    let header = build_header(filename, 1, 1, checksum);
    let mut decoded = Vec::new();
    decoded.extend_from_slice(SYNC);
    decoded.extend_from_slice(&header);
    decoded.extend_from_slice(body);
    xor_encode(&decoded)
}

fn summary(events: &[QbtFrameEvent]) -> Vec<(String, usize)> {
    events
        .iter()
        .map(|evt| match evt {
            QbtFrameEvent::DataBlock(seg) => ("data".to_string(), seg.content.len()),
            QbtFrameEvent::ServerListUpdate(list) => (
                "servers".to_string(),
                list.servers.len() + list.sat_servers.len(),
            ),
            QbtFrameEvent::Warning(_) => ("warning".to_string(), 0),
            _ => ("unknown".to_string(), 0),
        })
        .collect()
}

proptest! {
    #[test]
    fn random_wire_input_never_panics(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let mut decoder = QbtProtocolDecoder::default();
        let _ = decoder.feed(&data);
    }

    #[test]
    fn chunked_vs_single_feed_equivalence(split in 0usize..1100usize) {
        let body = vec![b'X'; 1024];
        let wire = frame_with_body("prop.txt", &body);
        let split_idx = split.min(wire.len());

        let mut single = QbtProtocolDecoder::default();
        let single_events = single.feed(&wire).expect("single decode must succeed");

        let mut chunked = QbtProtocolDecoder::default();
        let mut events = Vec::new();
        events.extend(chunked.feed(&wire[..split_idx]).expect("first chunk decode must succeed"));
        events.extend(chunked.feed(&wire[split_idx..]).expect("second chunk decode must succeed"));

        prop_assert_eq!(summary(&single_events), summary(&events));
    }
}
