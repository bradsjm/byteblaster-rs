//! Server command for HTTP/SSE API.
//!
//! This module provides the `server` command that runs an HTTP server with
//! Server-Sent Events (SSE) endpoints for real-time ByteBlaster event streaming.
//!
//! The server command:
//! - Connects to live ByteBlaster servers using `byteblaster-core`
//! - Provides SSE endpoint `/events` for real-time event streaming
//! - Offers `/files` endpoints for accessing completed files
//! - Exposes `/health` and `/metrics` endpoints for monitoring
//! - Supports filtering events by filename pattern
//! - Retains completed files in memory for a configurable TTL
//!
//! Implementation is delegated to `crate::live::server`.

use crate::live;

pub use live::server::ServerOptions;

pub async fn run(options: ServerOptions) -> crate::error::CliResult<()> {
    live::server::run(options).await
}
