//! Decode and encode the QBT/EMWIN wire protocol.
//!
//! This module holds the protocol-facing pieces that have to agree on framing, checksums,
//! compression, and server-list semantics. The decoder keeps wire recovery state local so the
//! rest of the runtime can work with typed events instead of partial transport details.

pub mod auth;
pub mod checksum;
pub mod codec;
pub mod compression;
pub mod model;
pub mod server_list;
pub mod server_list_wire;
