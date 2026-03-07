//! Authentication parser for QBT relay clients.
//!
//! This module provides stateful parsing of authentication messages from downstream
//! clients connecting to the relay. It accumulates decoded wire data and extracts
//! logon messages using the ByteBlast protocol format.

use crate::qbt_receiver::protocol::auth::{LOGON_PREFIX, LOGON_SUFFIX, parse_logon_message};

/// State machine for parsing authentication from wire chunks.
///
/// Accumulates XOR-decoded text and extracts complete logon messages
/// when the ByteBlast format is detected.
#[derive(Default)]
pub(super) struct AuthParser {
    /// Accumulated decoded text buffer (capped at 8KB).
    decoded_text: String,
}

impl AuthParser {
    /// Consumes a wire chunk and attempts to extract an authenticated email.
    ///
    /// XOR-decodes the wire data, appends it to the internal buffer, and
    /// scans for complete logon messages in ByteBlast format.
    ///
    /// The buffer is capped at 8KB to prevent unbounded growth.
    ///
    /// # Arguments
    ///
    /// * `wire` - Raw bytes from the wire (XOR-encoded)
    ///
    /// # Returns
    ///
    /// The authenticated email address if a valid logon message was found,
    /// `None` otherwise.
    pub(super) fn consume(&mut self, wire: &[u8]) -> Option<String> {
        let decoded = wire.iter().map(|byte| byte ^ 0xFF).collect::<Vec<_>>();
        self.decoded_text
            .push_str(&String::from_utf8_lossy(&decoded));

        if self.decoded_text.len() > 8192 {
            let keep = self.decoded_text.split_off(self.decoded_text.len() - 8192);
            self.decoded_text = keep;
        }

        let start = self.decoded_text.find(LOGON_PREFIX)?;
        let after_prefix = start + LOGON_PREFIX.len();
        let suffix_rel = self.decoded_text[after_prefix..].find(LOGON_SUFFIX)?;
        let suffix = after_prefix + suffix_rel;

        let message = self.decoded_text[start..suffix + LOGON_SUFFIX.len()].to_string();
        let remaining = self.decoded_text.split_off(suffix + LOGON_SUFFIX.len());
        self.decoded_text = remaining;

        parse_logon_message(&message).map(|message| message.email)
    }
}
