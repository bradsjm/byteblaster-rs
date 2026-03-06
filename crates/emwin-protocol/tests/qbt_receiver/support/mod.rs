use emwin_protocol::qbt_receiver::calculate_qbt_checksum;

const SYNC: &[u8; 6] = b"\0\0\0\0\0\0";

pub fn xor_encode(input: &[u8]) -> Vec<u8> {
    input.iter().map(|byte| byte ^ 0xFF).collect()
}

pub fn build_header(
    filename: &str,
    block: u32,
    total: u32,
    checksum: u32,
    dl: Option<usize>,
) -> [u8; 80] {
    let mut raw = if let Some(length) = dl {
        format!(
            "/PF{filename} /PN {block} /PT {total} /CS {checksum} /FD01/01/2024 01:00:00 AM /DL{length}\r\n"
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

pub fn build_frame(header: [u8; 80], body: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::new();
    decoded.extend_from_slice(SYNC);
    decoded.extend_from_slice(&header);
    decoded.extend_from_slice(body);
    xor_encode(&decoded)
}

#[allow(dead_code)]
pub fn build_single_block_frame(filename: &str, body: &[u8]) -> Vec<u8> {
    let checksum = u32::from(calculate_qbt_checksum(body));
    let header = build_header(filename, 1, 1, checksum, None);
    build_frame(header, body)
}
