//! Stateful parser for downstream relay authentication messages.

use crate::qbt_receiver::protocol::auth::{LOGON_PREFIX, LOGON_SUFFIX, parse_logon_message};

/// Incremental parser for relay logon messages.
#[derive(Default)]
pub(super) struct AuthParser {
    /// Accumulated decoded text buffer (capped at 8KB).
    decoded_text: String,
}

impl AuthParser {
    /// Consumes a wire chunk and extracts an authenticated email when a full logon arrives.
    ///
    /// The parser keeps only the trailing 8 KiB of decoded text so malformed or noisy clients
    /// cannot grow the buffer without bound.
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
