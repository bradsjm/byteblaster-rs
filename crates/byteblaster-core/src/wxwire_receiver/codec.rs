use crate::wxwire_receiver::error::WxWireReceiverError;
use crate::wxwire_receiver::model::{
    WxWireReceiverFile, WxWireReceiverFrameEvent, WxWireReceiverWarning,
};
use bytes::Bytes;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio_xmpp::minidom::Element;
use tokio_xmpp::parsers::delay::Delay;
use tokio_xmpp::parsers::message::Message;

/// Trait for Weather Wire stanza decoders.
pub trait WxWireFrameDecoder {
    /// Feeds one stanza XML string into the decoder.
    fn feed(&mut self, stanza: &str) -> Result<Vec<WxWireReceiverFrameEvent>, WxWireReceiverError>;

    /// Feeds one parsed XMPP message into the decoder.
    fn feed_message(
        &mut self,
        message: &Message,
    ) -> Result<Vec<WxWireReceiverFrameEvent>, WxWireReceiverError>;

    /// Resets decoder state.
    fn reset(&mut self);
}

/// Weather Wire stanza decoder.
#[derive(Debug, Default)]
pub struct WxWireDecoder;

impl WxWireFrameDecoder for WxWireDecoder {
    fn feed(&mut self, stanza: &str) -> Result<Vec<WxWireReceiverFrameEvent>, WxWireReceiverError> {
        let elem: Element = stanza.trim().parse().map_err(|err| {
            WxWireReceiverError::InvalidStanza(format!("invalid xml stanza: {err}"))
        })?;
        let message = Message::try_from(elem).map_err(|_| {
            WxWireReceiverError::InvalidStanza("not an xmpp <message/> stanza".to_string())
        })?;
        self.feed_message(&message)
    }

    fn feed_message(
        &mut self,
        message: &Message,
    ) -> Result<Vec<WxWireReceiverFrameEvent>, WxWireReceiverError> {
        let mut out = Vec::new();
        match parse_message_to_file(message) {
            Ok(parsed) => {
                if let Some(warning) = parsed.warning {
                    out.push(WxWireReceiverFrameEvent::Warning(warning));
                }
                out.push(WxWireReceiverFrameEvent::File(parsed.file));
                Ok(out)
            }
            Err(WxWireReceiverError::InvalidStanza(reason)) => {
                let warning = if reason.contains("missing nwws-oi payload") {
                    WxWireReceiverWarning::MissingNwwsNamespace
                } else if reason.contains("empty nwws body") {
                    WxWireReceiverWarning::EmptyBody
                } else {
                    WxWireReceiverWarning::DecoderRecovered { error: reason }
                };
                out.push(WxWireReceiverFrameEvent::Warning(warning));
                Ok(out)
            }
            Err(err) => Err(err),
        }
    }

    fn reset(&mut self) {}
}

#[derive(Debug)]
struct ParsedMessage {
    file: WxWireReceiverFile,
    warning: Option<WxWireReceiverWarning>,
}

fn parse_message_to_file(message: &Message) -> Result<ParsedMessage, WxWireReceiverError> {
    let subject = message
        .get_best_body_cloned(vec![""])
        .map(|(_, body)| body)
        .or_else(|| {
            message
                .get_best_subject_cloned(vec![""])
                .map(|(_, subject)| subject)
        })
        .unwrap_or_default();

    let x_payload = message
        .payloads
        .iter()
        .find(|payload| payload.name() == "x" && payload.ns() == "nwws-oi")
        .ok_or_else(|| WxWireReceiverError::InvalidStanza("missing nwws-oi payload".to_string()))?;

    let body = x_payload.text().trim().to_string();
    if body.is_empty() {
        return Err(WxWireReceiverError::InvalidStanza(
            "empty nwws body".to_string(),
        ));
    }

    let id = x_payload.attr("id").unwrap_or_default().to_string();
    let issue_raw = x_payload.attr("issue").unwrap_or_default().to_string();
    let ttaaii = x_payload.attr("ttaaii").unwrap_or_default().to_string();
    let cccc = x_payload.attr("cccc").unwrap_or_default().to_string();
    let awipsid = x_payload
        .attr("awipsid")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("NONE")
        .to_string();

    let (issue_utc, warning) = parse_timestamp_or_now(issue_raw.as_str());

    let delay_stamp_utc = message.payloads.iter().find_map(|payload| {
        if payload.name() != "delay" || payload.ns() != "urn:xmpp:delay" {
            return None;
        }
        let delay = Delay::try_from(payload.clone()).ok()?;
        chrono_to_system_time(
            delay.stamp.0.timestamp(),
            delay.stamp.0.timestamp_subsec_nanos(),
        )
    });

    let filename = build_filename(&awipsid, &ttaaii, &cccc, &id, issue_utc);
    let noaaport = convert_to_noaaport(body.as_str());

    Ok(ParsedMessage {
        file: WxWireReceiverFile {
            filename,
            data: Bytes::from(noaaport.into_bytes()),
            subject,
            id,
            issue_utc,
            ttaaii,
            cccc,
            awipsid,
            delay_stamp_utc,
        },
        warning,
    })
}

fn parse_timestamp_or_now(raw: &str) -> (SystemTime, Option<WxWireReceiverWarning>) {
    match parse_rfc3339_to_system_time(raw) {
        Some(ts) => (ts, None),
        None => {
            let warning = if raw.is_empty() {
                None
            } else {
                Some(WxWireReceiverWarning::TimestampParseFallback {
                    raw: raw.to_string(),
                })
            };
            (SystemTime::now(), warning)
        }
    }
}

fn parse_rfc3339_to_system_time(raw: &str) -> Option<SystemTime> {
    let parsed = OffsetDateTime::parse(raw, &Rfc3339).ok()?;
    chrono_to_system_time(parsed.unix_timestamp(), parsed.nanosecond())
}

fn chrono_to_system_time(seconds: i64, nanos: u32) -> Option<SystemTime> {
    if seconds >= 0 {
        Some(UNIX_EPOCH + Duration::new(seconds as u64, nanos))
    } else {
        Some(UNIX_EPOCH - Duration::new(seconds.unsigned_abs(), nanos))
    }
}

fn build_filename(
    awipsid: &str,
    ttaaii: &str,
    cccc: &str,
    id: &str,
    issue_utc: SystemTime,
) -> String {
    if !awipsid.is_empty() && awipsid != "NONE" {
        return format!("{awipsid}.txt");
    }

    if !ttaaii.is_empty() && !cccc.is_empty() {
        return format!("{ttaaii}_{cccc}.txt");
    }

    if !id.is_empty() {
        return format!("{id}.txt");
    }

    let secs = issue_utc
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    format!("wxwire_{secs}.txt")
}

fn convert_to_noaaport(text: &str) -> String {
    let mut noaaport = format!("\x01{}", text.replace("\n\n", "\r\r\n"));
    if !noaaport.ends_with('\n') {
        noaaport.push_str("\r\r\n");
    }
    noaaport.push('\x03');
    noaaport
}

#[cfg(test)]
mod tests {
    use super::{WxWireDecoder, WxWireFrameDecoder};
    use crate::wxwire_receiver::model::{WxWireReceiverFrameEvent, WxWireReceiverWarning};

    #[test]
    fn decode_valid_stanza_to_file_event() {
        let stanza = r#"
            <message xmlns='jabber:client' type='groupchat'>
              <body>TEST SUBJECT</body>
              <x xmlns="nwws-oi" id="a1" issue="2026-03-05T00:00:00Z" ttaaii="NOUS41" cccc="KOKX" awipsid="AFDOKX">line1

line2</x>
            </message>
        "#;

        let mut decoder = WxWireDecoder;
        let events = decoder.feed(stanza).expect("decode should not fail");

        let file = events.iter().find_map(|event| match event {
            WxWireReceiverFrameEvent::File(file) => Some(file),
            _ => None,
        });

        let file = file.expect("expected file event");
        assert_eq!(file.filename, "AFDOKX.txt");
        assert_eq!(file.awipsid, "AFDOKX");
        assert!(file.data.starts_with(&[0x01]));
        assert!(file.data.ends_with(&[0x03]));
    }

    #[test]
    fn missing_nwws_payload_emits_warning() {
        let stanza = r#"<message xmlns='jabber:client'><body>bad</body></message>"#;
        let mut decoder = WxWireDecoder;
        let events = decoder.feed(stanza).expect("decode should not fail");
        assert!(events.iter().any(|event| {
            matches!(
                event,
                WxWireReceiverFrameEvent::Warning(WxWireReceiverWarning::MissingNwwsNamespace)
            )
        }));
    }

    #[test]
    fn invalid_issue_timestamp_emits_fallback_warning() {
        let stanza = r#"
            <message xmlns='jabber:client'>
              <subject>S</subject>
              <x xmlns="nwws-oi" id="a1" issue="not-a-time" ttaaii="NOUS41" cccc="KOKX" awipsid="AFDOKX">line</x>
            </message>
        "#;

        let mut decoder = WxWireDecoder;
        let events = decoder.feed(stanza).expect("decode should not fail");

        assert!(events.iter().any(|event| {
            matches!(
                event,
                WxWireReceiverFrameEvent::Warning(
                    WxWireReceiverWarning::TimestampParseFallback { .. }
                )
            )
        }));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, WxWireReceiverFrameEvent::File(_)))
        );
    }
}
