use byteblaster_core::unstable::xor_ff;
use bytes::Bytes;

#[derive(Default)]
pub struct ServerListScanner {
    decoded_buffer: Vec<u8>,
}

impl ServerListScanner {
    pub fn observe_wire_chunk(&mut self, wire: &[u8]) -> Option<Bytes> {
        self.decoded_buffer
            .extend(wire.iter().map(|byte| byte ^ 0xFF));
        if self.decoded_buffer.len() > 65_536 {
            let drop_count = self.decoded_buffer.len() - 65_536;
            self.decoded_buffer.drain(..drop_count);
        }

        let prefix = b"/ServerList/";
        let mut latest = None;
        while let Some(start) = find_subsequence(&self.decoded_buffer, prefix) {
            let slice = &self.decoded_buffer[start..];
            let Some(end_rel) = slice.iter().position(|byte| *byte == 0) else {
                break;
            };
            let end = start + end_rel + 1;
            let frame = Bytes::copy_from_slice(&self.decoded_buffer[start..end]);
            latest = Some(xor_ff(&frame));
            self.decoded_buffer.drain(..end);
        }

        latest
    }
}

pub fn build_server_list_wire(servers: &[(String, u16)]) -> Bytes {
    let entries = servers
        .iter()
        .map(|(host, port)| format!("{host}:{port}"))
        .collect::<Vec<_>>()
        .join("|");
    let payload = format!("/ServerList/{entries}\0");
    xor_ff(payload.as_bytes())
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
