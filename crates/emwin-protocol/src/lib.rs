//! Core receivers, protocol code, and ingest adapters for EMWIN feeds.
//!
//! The crate exposes two transport-specific receivers, `qbt_receiver` and `wxwire_receiver`, plus
//! the `ingest` layer that normalizes their output into one event stream. Most users should start
//! at `ingest` unless they need transport-specific controls.

#[cfg(any(feature = "qbt", feature = "wxwire"))]
pub mod ingest;
#[cfg(feature = "qbt")]
pub mod qbt_receiver;
mod runtime_support;
#[cfg(feature = "wxwire")]
pub mod wxwire_receiver;

/// Unstable API surface. Items in this module may change without stability guarantees.
pub mod unstable {
    #[cfg(feature = "qbt")]
    pub mod qbt_receiver {
        pub use crate::qbt_receiver::unstable::*;
    }

    #[cfg(feature = "wxwire")]
    pub mod wxwire_receiver {
        pub use crate::wxwire_receiver::unstable::*;
    }
}
