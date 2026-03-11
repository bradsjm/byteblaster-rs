//! Live CLI modes backed by real EMWIN or Weather Wire connections.
//!
//! This subtree adapts the shared ingest runtime into CLI behaviors such as streaming output,
//! optional file persistence, and the HTTP/SSE server mode. It owns command-level orchestration
//! and leaves protocol details in `emwin-protocol`.

pub(crate) mod archive_postprocess;
pub(crate) mod config;
pub(crate) mod file_pipeline;
pub(crate) mod filter;
pub(crate) mod ingest;
pub mod server;
mod server_support;
pub mod shared;
pub mod stream;
