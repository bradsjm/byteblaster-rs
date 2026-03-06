//! Unified product event abstraction and ingestion adapters.
//!
//! This module provides a common interface for receiving products from different sources
//! (QBT satellite and Weather Wire) and normalizing them into a unified event stream.
//!
//! ## Core Types
//!
//! - [`ReceivedProduct`]: Enum representing product receipt events (in-progress, complete, warning)
//! - [`ProductOrigin`]: Source identifier for products (QBT or Weather Wire)
//! - [`IngestEvent`]: Combined stream of products and telemetry events
//!
//! ## Adapters
//!
//! - `adapt_qbt_events`: Converts QBT segment/file events into unified product events
//! - `adapt_wxwire_events`: Converts Weather Wire file events into unified product events
//!
//! Both adapters handle the specific nuances of their source protocols while emitting
//! a uniform stream of `ReceivedProduct` events that can be consumed by application code.

pub mod model;
#[cfg(feature = "qbt")]
pub mod qbt_adapter;
#[cfg(feature = "wxwire")]
pub mod wxwire_adapter;

pub use model::{
    IngestError, IngestEvent, IngestTelemetry, IngestWarning, ProductOrigin, ReceivedProduct,
};
#[cfg(feature = "qbt")]
pub use qbt_adapter::{QbtIngestStream, adapt_qbt_events};
#[cfg(feature = "wxwire")]
pub use wxwire_adapter::{WxWireIngestStream, adapt_wxwire_events};
