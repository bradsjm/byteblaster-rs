//! Weather Wire receiver client with custom XMPP transport.
//!
//! This module provides a client implementation for receiving products via the Weather Wire
//! service using XMPP (Extensible Messaging and Presence Protocol) with custom extensions.
//!
//! ## Architecture
//!
//! The receiver is organized into several components:
//!
//! - **Transport** (`transport`): Custom XMPP transport handling STARTTLS, SASL authentication,
//!   resource binding, MUC (Multi-User Chat) room joining, and stanza parsing
//! - **Codec** (`codec`): Decodes XMPP messages into weather product events with metadata extraction
//! - **Client** (`client`): Reconnect state machine with bounded backoff, XEP-0198 stream management
//!   for reliability, and handler isolation
//! - **Config** (`config`): Configuration with credential validation and debug redaction
//!
//! ## Protocol Details
//!
//! Weather Wire uses XMPP with custom extensions:
//! - Authentication via SASL PLAIN
//! - MUC room joining for product broadcasts
//! - XEP-0198 stream management for reliability (heartbeat/acks)
//! - Custom NWWS XML namespace for product payloads
//!
//! ## Example
//!
//! ```rust,no_run
//! use emwin_protocol::wxwire_receiver::{
//!     WxWireReceiver, WxWireReceiverClient, WxWireReceiverConfig,
//! };
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = WxWireReceiverConfig {
//!         username: "you@example.com".to_string(),
//!         password: "secret".to_string(),
//!         ..WxWireReceiverConfig::default()
//!     };
//!     let mut receiver = WxWireReceiver::builder(config).build()?;
//!     receiver.start()?;
//!     receiver.stop().await?;
//!     Ok(())
//! }
//! ```

mod client;
mod codec;
mod config;
mod error;
mod model;
mod transport;

pub use client::{
    UnstableWxWireReceiverIngress, WxWireReceiver, WxWireReceiverBuilder, WxWireReceiverClient,
    WxWireReceiverEvent, WxWireReceiverEventHandler, WxWireReceiverTelemetrySnapshot,
};
pub use codec::{WxWireDecoder, WxWireFrameDecoder};
pub use config::{WXWIRE_PRIMARY_HOST, WxWireReceiverConfig};
pub use error::{WxWireReceiverError, WxWireReceiverResult};
pub use model::{WxWireReceiverFile, WxWireReceiverFrameEvent, WxWireReceiverWarning};
pub use transport::{WxWireTransport, XmppWxWireTransport};

pub mod unstable {
    pub use super::client::UnstableWxWireReceiverIngress;
}
