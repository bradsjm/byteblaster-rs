//! Authentication utilities for EMWIN protocol.
//!
//! This module provides functions for building authentication messages
//! and applying the XOR 0xFF wire encoding used by the protocol.

use bytes::Bytes;

use crate::qbt_receiver::protocol::model::QbtAuthMessage;

/// Interval between re-authentication messages in seconds.
///
/// The client must send periodic logon messages to maintain the connection.
pub const REAUTH_INTERVAL_SECS: u64 = 115;
pub const LOGON_PREFIX: &str = "ByteBlast Client|NM-";
pub const LOGON_SUFFIX: &str = "|V2";

/// Builds a logon message for authentication.
///
/// # Arguments
///
/// * `email` - The user's email address
///
/// # Returns
///
/// A formatted logon message string
///
/// # Example
///
/// ```
/// use emwin_protocol::unstable::qbt_receiver::build_logon_message;
///
/// let msg = build_logon_message("user@example.com");
/// assert_eq!(msg, "ByteBlast Client|NM-user@example.com|V2");
/// ```
pub fn build_logon_message(email: &str) -> String {
    format!("{LOGON_PREFIX}{email}{LOGON_SUFFIX}")
}

/// Parses a logon message string and extracts the authentication payload.
///
/// Expected format: `ByteBlast Client|NM-{email}|V2`
pub fn parse_logon_message(message: &str) -> Option<QbtAuthMessage> {
    let payload = message.strip_prefix(LOGON_PREFIX)?;
    let email = payload.strip_suffix(LOGON_SUFFIX)?.trim();
    if email.is_empty() {
        return None;
    }
    Some(QbtAuthMessage {
        email: email.to_string(),
    })
}

/// Applies XOR 0xFF encoding to data.
///
/// This is the wire encoding used by the EMWIN protocol.
/// Each byte is XORed with 0xFF to obfuscate the data.
///
/// # Arguments
///
/// * `data` - The raw bytes to encode
///
/// # Returns
///
/// Encoded bytes as a `Bytes` object
///
/// # Example
///
/// ```
/// use emwin_protocol::unstable::qbt_receiver::xor_ff;
///
/// let encoded = xor_ff(b"Hello");
/// // Each byte is XORed with 0xFF
/// assert_eq!(encoded[0], b'H' ^ 0xFF);
/// ```
pub fn xor_ff(data: &[u8]) -> Bytes {
    let encoded: Vec<u8> = data.iter().map(|b| b ^ 0xFF).collect();
    Bytes::from(encoded)
}

#[cfg(test)]
mod tests {
    use super::{build_logon_message, parse_logon_message};

    #[test]
    fn parse_logon_message_extracts_email() {
        let parsed =
            parse_logon_message("ByteBlast Client|NM-user@example.com|V2").expect("valid logon");
        assert_eq!(parsed.email, "user@example.com");
    }

    #[test]
    fn parse_logon_message_rejects_invalid_shapes() {
        assert!(parse_logon_message("ByteBlast Client|NM-|V2").is_none());
        assert!(parse_logon_message("ByteBlast Client|NM-user@example.com|V1").is_none());
    }

    #[test]
    fn build_and_parse_roundtrip() {
        let logon = build_logon_message("relay@example.com");
        let parsed = parse_logon_message(&logon).expect("roundtrip should parse");
        assert_eq!(parsed.email, "relay@example.com");
    }
}
