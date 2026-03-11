//! CLI relay mode for low-latency QBT passthrough.
//!
//! The relay command wires CLI configuration into the core relay runtime and exposes the small
//! HTTP surface used for health and metrics. Payload bytes stay untouched once they enter the
//! protocol crate.

pub(crate) mod config;
pub(crate) mod runtime;

pub use config::RelayArgs as RelayOptions;
