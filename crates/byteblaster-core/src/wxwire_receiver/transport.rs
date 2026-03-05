use crate::wxwire_receiver::config::{WXWIRE_PORT, WXWIRE_ROOM};
use crate::wxwire_receiver::error::{WxWireReceiverError, WxWireReceiverResult};
use futures::StreamExt;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Once;
use std::time::Duration;
use tokio_xmpp::Client;
use tokio_xmpp::Event;
use tokio_xmpp::connect::DnsConfig;
use tokio_xmpp::parsers::jid::BareJid;
use tokio_xmpp::parsers::message::{Message, MessageType};
use tokio_xmpp::parsers::muc::Muc;
use tokio_xmpp::parsers::muc::muc::History;
use tokio_xmpp::parsers::presence::Presence;
use tokio_xmpp::xmlstream::Timeouts;

/// Abstraction over weather wire transport.
pub trait WxWireTransport: Send {
    /// Label for diagnostics and client events.
    fn label(&self) -> String;

    /// Reads one next weather-wire groupchat message.
    fn next_message<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = WxWireReceiverResult<Message>> + Send + 'a>>;

    /// Disconnects and cleans up the transport.
    fn disconnect<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = WxWireReceiverResult<()>> + Send + 'a>>;
}

/// Real XMPP transport backed by tokio-xmpp.
#[derive(Debug)]
pub struct XmppWxWireTransport {
    client: Option<Client>,
    room_bare: BareJid,
    label: String,
}

impl XmppWxWireTransport {
    /// Connects and joins the fixed room.
    pub async fn connect(
        endpoint_host: &str,
        username: &str,
        password: &str,
        connect_timeout: Duration,
    ) -> WxWireReceiverResult<Self> {
        ensure_tls_provider();

        let bare_jid = BareJid::from_str(&format!("{username}@{endpoint_host}"))
            .map_err(|err| WxWireReceiverError::Transport(format!("invalid jid: {err}")))?;

        let dns = DnsConfig::no_srv(endpoint_host, WXWIRE_PORT);
        let timeouts = Timeouts {
            read_timeout: connect_timeout,
            ..Timeouts::default()
        };
        let mut client = Client::new_starttls(bare_jid, password.to_string(), dns, timeouts);

        let room_bare = BareJid::from_str(WXWIRE_ROOM)
            .map_err(|err| WxWireReceiverError::Transport(format!("invalid room jid: {err}")))?;

        let online = tokio::time::timeout(connect_timeout, async {
            loop {
                let Some(event) = client.next().await else {
                    return Err(WxWireReceiverError::Transport(
                        "xmpp stream ended before online".to_string(),
                    ));
                };
                match event {
                    Event::Online { .. } => return Ok(()),
                    Event::Disconnected(err) => {
                        return Err(WxWireReceiverError::Transport(format!(
                            "xmpp disconnected before online: {err}"
                        )));
                    }
                    Event::Stanza(_) => {}
                }
            }
        })
        .await
        .map_err(|_| WxWireReceiverError::Transport("xmpp connect timeout".to_string()))?;
        online?;

        let nick = format!("bb{}", chrono_like_suffix());
        let room_full = room_bare.with_resource_str(nick.as_str()).map_err(|err| {
            WxWireReceiverError::Transport(format!("invalid room resource jid: {err}"))
        })?;

        let join_presence = Presence::available()
            .with_to(room_full)
            .with_payload(Muc::new().with_history(History::new().with_maxstanzas(25)));
        client
            .send_stanza(join_presence.into())
            .await
            .map_err(|err| WxWireReceiverError::Transport(format!("failed to join room: {err}")))?;

        Ok(Self {
            client: Some(client),
            room_bare,
            label: format!("{endpoint_host}:{WXWIRE_PORT} room={WXWIRE_ROOM}"),
        })
    }
}

impl WxWireTransport for XmppWxWireTransport {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn next_message<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = WxWireReceiverResult<Message>> + Send + 'a>> {
        Box::pin(async move {
            let client = self.client.as_mut().ok_or_else(|| {
                WxWireReceiverError::Transport("xmpp client not connected".to_string())
            })?;
            loop {
                let Some(event) = client.next().await else {
                    return Err(WxWireReceiverError::Transport(
                        "xmpp stream ended".to_string(),
                    ));
                };

                match event {
                    Event::Online { .. } => {
                        continue;
                    }
                    Event::Disconnected(err) => {
                        return Err(WxWireReceiverError::Transport(format!(
                            "xmpp disconnected: {err}"
                        )));
                    }
                    Event::Stanza(stanza) => {
                        let Ok(message) = Message::try_from(stanza) else {
                            continue;
                        };

                        if message.type_ != MessageType::Groupchat {
                            continue;
                        }

                        let from_room = message
                            .from
                            .as_ref()
                            .map(|jid| jid.to_bare())
                            .map(|bare| bare == self.room_bare)
                            .unwrap_or(false);
                        if !from_room {
                            continue;
                        }

                        return Ok(message);
                    }
                }
            }
        })
    }

    fn disconnect<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = WxWireReceiverResult<()>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(client) = self.client.take() {
                client.send_end().await.map_err(|err| {
                    WxWireReceiverError::Transport(format!("disconnect failed: {err}"))
                })?;
            }
            Ok(())
        })
    }
}

fn chrono_like_suffix() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}

fn ensure_tls_provider() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tokio_xmpp::rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}
