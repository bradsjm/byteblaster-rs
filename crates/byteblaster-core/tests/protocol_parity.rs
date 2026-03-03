use byteblaster_core::{
    FrameDecoder, FrameEvent, ProtocolDecoder, ProtocolWarning, calculate_checksum,
};

const SYNC: &[u8; 6] = b"\0\0\0\0\0\0";

fn xor_encode(input: &[u8]) -> Vec<u8> {
    input.iter().map(|b| b ^ 0xFF).collect()
}

fn build_header(
    filename: &str,
    block: u32,
    total: u32,
    checksum: u32,
    dl: Option<usize>,
) -> [u8; 80] {
    let mut raw = if let Some(len) = dl {
        format!(
            "/PF{filename} /PN {block} /PT {total} /CS {checksum} /FD01/01/2024 01:00:00 AM /DL{len}\r\n"
        )
    } else {
        format!(
            "/PF{filename} /PN {block} /PT {total} /CS {checksum} /FD01/01/2024 01:00:00 AM\r\n"
        )
    };
    while raw.len() < 80 {
        raw.push(' ');
    }

    let mut out = [0u8; 80];
    out.copy_from_slice(&raw.as_bytes()[..80]);
    out
}

fn build_frame(header: [u8; 80], body: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::new();
    decoded.extend_from_slice(SYNC);
    decoded.extend_from_slice(&header);
    decoded.extend_from_slice(body);
    xor_encode(&decoded)
}

#[test]
fn sync_recovery() {
    let body = [b'A'; 1024];
    let checksum = calculate_checksum(&body) as u32;
    let header = build_header("sync.txt", 1, 1, checksum, None);

    let mut wire = xor_encode(b"garbage-before-sync");
    wire.extend(build_frame(header, &body));

    let mut decoder = ProtocolDecoder::default();
    let events = decoder
        .feed(&wire)
        .expect("decoder should recover and parse frame");

    assert!(events.iter().any(|e| matches!(e, FrameEvent::DataBlock(_))));
}

#[test]
fn v2_fixture_bounds() {
    let body = [0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01];
    let checksum = calculate_checksum(&body) as u32;
    let header = build_header("v2.dat", 1, 1, checksum, Some(2048));
    let wire = build_frame(header, &body);

    let mut decoder = ProtocolDecoder::default();
    let events = decoder
        .feed(&wire)
        .expect("invalid /DL bound should recover and continue");
    assert!(events.iter().any(|evt| matches!(
        evt,
        FrameEvent::Warning(ProtocolWarning::DecoderRecovered { .. })
    )));
    assert!(
        !events
            .iter()
            .any(|evt| matches!(evt, FrameEvent::DataBlock(_)))
    );
}

#[test]
fn server_update_simple() {
    let payload = b"/ServerList/a.example:2211|b.example:1000\0";
    let mut decoded = Vec::new();
    decoded.extend_from_slice(SYNC);
    decoded.extend_from_slice(payload);

    let mut decoder = ProtocolDecoder::default();
    let events = decoder
        .feed(&xor_encode(&decoded))
        .expect("simple server list should parse");

    let list = events.iter().find_map(|evt| match evt {
        FrameEvent::ServerListUpdate(list) => Some(list),
        _ => None,
    });

    let list = list.expect("expected server list update event");
    assert_eq!(list.servers.len(), 2);
    assert!(list.sat_servers.is_empty());
}

#[test]
fn server_update_full_format() {
    let payload =
        b"/ServerList/a.example:2211|bad-entry\\ServerList\\/SatServers/sat1:3000+sat2:3001\\SatServers\\\0";
    let mut decoded = Vec::new();
    decoded.extend_from_slice(SYNC);
    decoded.extend_from_slice(payload);

    let mut decoder = ProtocolDecoder::default();
    let events = decoder
        .feed(&xor_encode(&decoded))
        .expect("full server list should parse");

    let list = events.iter().find_map(|evt| match evt {
        FrameEvent::ServerListUpdate(list) => Some(list),
        _ => None,
    });

    let list = list.expect("expected server list update event");
    assert_eq!(list.servers.len(), 1);
    assert_eq!(list.sat_servers.len(), 2);
}

#[test]
fn mixed_corruption_stream() {
    let body = [b'B'; 1024];
    let checksum = calculate_checksum(&body) as u32;
    let header = build_header("ok.txt", 1, 1, checksum, None);

    let mut decoded = Vec::new();
    decoded.extend_from_slice(SYNC);
    decoded.extend_from_slice(b"/XXcorrupted");
    decoded.extend_from_slice(SYNC);
    decoded.extend_from_slice(&header);
    decoded.extend_from_slice(&body);

    let mut decoder = ProtocolDecoder::default();
    let events = decoder
        .feed(&xor_encode(&decoded))
        .expect("decoder should resync after unknown frame type");

    assert!(events.iter().any(|e| matches!(e, FrameEvent::DataBlock(_))));
}

#[test]
fn inspect_valid_v1_fixture() {
    let body = [b'Z'; 1024];
    let checksum = calculate_checksum(&body) as u32;
    let header = build_header("valid-v1.txt", 1, 1, checksum, None);
    let wire = build_frame(header, &body);

    let mut decoder = ProtocolDecoder::default();
    let events = decoder.feed(&wire).expect("valid v1 frame should decode");
    assert!(events.iter().any(|e| matches!(e, FrameEvent::DataBlock(_))));
}

#[test]
fn checksum_fixture() {
    let body = [0x5Au8; 1024];
    let checksum = calculate_checksum(&body) as u32;
    let header = build_header("checksum.bin", 1, 1, checksum, None);
    let wire = build_frame(header, &body);

    let mut decoder = ProtocolDecoder::default();
    let events = decoder.feed(&wire).expect("checksum fixture should decode");
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], FrameEvent::DataBlock(_)));
}

#[test]
fn v2_corrupt_payload_policy() {
    let body = [0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00];
    let checksum = calculate_checksum(&body) as u32;
    let header = build_header("bad-v2.dat", 1, 1, checksum, Some(body.len()));
    let wire = build_frame(header, &body);

    let mut decoder = ProtocolDecoder::default();
    let events = decoder
        .feed(&wire)
        .expect("decoder should drop corrupted compressed payload and continue");

    assert!(events.iter().any(|evt| matches!(
        evt,
        FrameEvent::Warning(ProtocolWarning::DecompressionFailed { .. })
    )));
    assert!(
        !events
            .iter()
            .any(|evt| matches!(evt, FrameEvent::DataBlock(_)))
    );
}

#[test]
fn stream_policy_behavior() {
    let bad_body = [b'X'; 1024];
    let bad_checksum = (u32::from(calculate_checksum(&bad_body))).wrapping_add(1);
    let bad_header = build_header("bad.bin", 1, 2, bad_checksum, None);
    let bad_frame = build_frame(bad_header, &bad_body);

    let good_body = [b'Y'; 1024];
    let good_checksum = calculate_checksum(&good_body) as u32;
    let good_header = build_header("good.bin", 2, 2, good_checksum, None);
    let good_frame = build_frame(good_header, &good_body);

    let mut wire = Vec::new();
    wire.extend_from_slice(&bad_frame);
    wire.extend_from_slice(&good_frame);

    let mut decoder = ProtocolDecoder::default();
    let events = decoder
        .feed(&wire)
        .expect("stream should drop checksum mismatch and keep valid frames");

    assert!(events.iter().any(|evt| matches!(
        evt,
        FrameEvent::Warning(ProtocolWarning::ChecksumMismatch { .. })
    )));
    assert!(events.iter().any(|evt| {
        matches!(evt, FrameEvent::DataBlock(segment) if segment.filename == "good.bin")
    }));
    assert!(
        !events.iter().any(
            |evt| matches!(evt, FrameEvent::DataBlock(segment) if segment.filename == "bad.bin")
        )
    );
}
