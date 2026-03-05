//! # byteblaster-core
//!
//! Core library for ByteBlaster protocol receivers.

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
