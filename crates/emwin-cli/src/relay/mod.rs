//! Low-latency TCP relay mode for EMWIN protocol.
//!
//! This module implements a relay server that:
//! - Receives the upstream EMWIN feed from configured servers
//! - Forwards raw wire bytes to downstream clients with minimal added latency
//! - Manages downstream client authentication (local, not forwarded upstream)
//! - Provides HTTP metrics endpoints for monitoring relay health
//!
//! ## Behavior
//!
//! The relay operates in passthrough mode with the following characteristics:
//! - No payload filtering or frame transformation
//! - Downstream clients must authenticate on connect and re-authenticate every 12 minutes
//! - Upstream server-list updates are forwarded as-is to downstream clients
//! - Quality monitoring: pauses forwarding when quality drops below 0.95, resumes at ≥0.97
//!
//! ## Components
//!
//! - [`config`]: Relay command-line parsing and mapping to core runtime config
//! - [`runtime`]: CLI adapter that runs the core relay engine and exposes metrics HTTP endpoints
//!
//! ## Endpoints
//!
//! - TCP listener on `0.0.0.0:2211` (configurable) for downstream EMWIN clients
//! - HTTP metrics on `127.0.0.1:9090` (configurable) for `/health` and `/metrics`
//!
//! ## Quality Control
//!
//! The relay maintains a rolling quality window (default 60 seconds) tracking
//! the ratio of successfully forwarded bytes to attempted bytes. This enables
//! backpressure-aware operation and protects downstream clients from receiving
//! corrupted or incomplete data.

pub(crate) mod config;
pub(crate) mod runtime;

pub use config::RelayArgs as RelayOptions;
