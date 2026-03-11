//! Weather Wire receiver runtime built on a focused XMPP transport.
//!
//! This module combines the custom transport, decoder, and reconnect logic needed for NWWS/Weather
//! Wire reception. It keeps XMPP details local and exposes the same style of runtime API as the
//! QBT receiver.

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
