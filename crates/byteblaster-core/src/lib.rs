//! # byteblaster-core
//!
//! Core library for ByteBlaster protocol receivers and unified product ingestion.
//!
//! This library provides:
//! - `qbt_receiver`: ByteBlaster QBT satellite receiver client with stateful decoder, server-list management, and file assembly
//! - `wxwire_receiver`: Weather Wire receiver client with custom XMPP transport
//! - `ingest`: Unified product event abstraction and adapters for QBT and Weather Wire sources
//!
//! ## Example
//!
//! ```rust,no_run
//! use byteblaster_core::{QbtReceiver, QbtReceiverConfig};
//! use byteblaster_core::ingest::{adapt_qbt_events, ReceivedProduct};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = QbtReceiverConfig::default();
//!     let (mut receiver, mut stream) = QbtReceiver::new(config).await?;
//!
//!     let mut ingest_stream = adapt_qbt_events(stream);
//!
//!     tokio::spawn(async move {
//!         while let Some(event) = ingest_stream.recv().await {
//!             if let ReceivedProduct::Complete(product) = event {
//!                 println!("Received product: {}", product.filename);
//!             }
//!         }
//!     });
//!
//!     Ok(())
//! }
//! ```

pub mod ingest;
pub mod qbt_receiver;
pub mod wxwire_receiver;

/// Unstable API surface. Items in this module may change without stability guarantees.
pub mod unstable {
    pub mod qbt_receiver {
        pub use crate::qbt_receiver::client::reconnect::{EndpointRotator, next_backoff_secs};
        pub use crate::qbt_receiver::client::watchdog::{HealthObserver, Watchdog};
        pub use crate::qbt_receiver::protocol::auth::{build_logon_message, xor_ff};
        pub use crate::qbt_receiver::protocol::server_list::{
            parse_server_list_frame, parse_simple_server_list,
        };
        pub use crate::qbt_receiver::stream::file_stream::QbtFileStream;
        pub use crate::qbt_receiver::stream::segment_stream::QbtSegmentStream;
    }

    pub mod wxwire_receiver {
        pub use crate::wxwire_receiver::client::UnstableWxWireReceiverIngress;
    }
}
