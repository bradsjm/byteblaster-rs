#[derive(Default)]
pub struct AuthParser {
    decoded_text: String,
}

impl AuthParser {
    pub fn consume(&mut self, wire: &[u8]) -> Option<String> {
        let decoded = wire.iter().map(|byte| byte ^ 0xFF).collect::<Vec<_>>();
        self.decoded_text
            .push_str(&String::from_utf8_lossy(&decoded));

        const PREFIX: &str = "ByteBlast Client|NM-";
        const SUFFIX: &str = "|V2";

        if self.decoded_text.len() > 8192 {
            let keep = self.decoded_text.split_off(self.decoded_text.len() - 8192);
            self.decoded_text = keep;
        }

        let start = self.decoded_text.find(PREFIX)?;
        let after_prefix = start + PREFIX.len();
        let suffix_rel = self.decoded_text[after_prefix..].find(SUFFIX)?;
        let suffix = after_prefix + suffix_rel;
        let email = self.decoded_text[after_prefix..suffix].trim().to_string();

        let remaining = self.decoded_text.split_off(suffix + SUFFIX.len());
        self.decoded_text = remaining;

        if email.is_empty() { None } else { Some(email) }
    }
}
