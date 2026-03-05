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
//! use byteblaster_core::wxwire_receiver::{WxWireReceiver, WxWireReceiverConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = WxWireReceiverConfig::builder()
//!         .email("you@example.com")
//!         .password("your-pass")
//!         .build()?;
//!
//!     let (mut receiver, mut stream) = WxWireReceiver::new(config).await?;
//!
//!     tokio::spawn(async move {
//!         while let Some(event) = stream.recv().await {
//!             println!("Received: {:?}", event);
//!         }
//!     });
//!
//!     receiver.run().await?;
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod codec;
pub mod config;
pub mod error;
pub mod model;
pub mod transport;

pub use client::{
    UnstableWxWireReceiverIngress, WxWireReceiver, WxWireReceiverBuilder, WxWireReceiverClient,
    WxWireReceiverEvent, WxWireReceiverEventHandler, WxWireReceiverTelemetrySnapshot,
};
pub use codec::{WxWireDecoder, WxWireFrameDecoder};
pub use config::{WXWIRE_PRIMARY_HOST, WxWireReceiverConfig};
pub use error::{WxWireReceiverError, WxWireReceiverResult};
pub use model::{WxWireReceiverFile, WxWireReceiverFrameEvent, WxWireReceiverWarning};
pub use transport::{WxWireTransport, XmppWxWireTransport};
