use bytes::Bytes;

pub const REAUTH_INTERVAL_SECS: u64 = 115;

pub fn build_logon_message(email: &str) -> String {
    format!("ByteBlast Client|NM-{email}|V2")
}

pub fn xor_ff(data: &[u8]) -> Bytes {
    let encoded: Vec<u8> = data.iter().map(|b| b ^ 0xFF).collect();
    Bytes::from(encoded)
}
