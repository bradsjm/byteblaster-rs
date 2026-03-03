//! Command implementations for the ByteBlaster CLI.
//!
//! This module contains the implementation of each CLI subcommand:
//! - `download`: Download and assemble files
//! - `inspect`: Inspect capture files
//! - `server`: Run HTTP server with SSE endpoints
//! - `stream`: Stream events from capture files or live servers

pub mod download;
pub mod event_output;
pub mod inspect;
pub mod server;
pub mod stream;
