//! Command implementations for the ByteBlaster CLI.
//!
//! This module contains the implementation of each CLI subcommand.
//!
//! ## Available Commands
//!
//! ### `download <output_dir>`
//! Downloads and assembles complete files from the ByteBlaster feed.
//! - Connects to live servers and continuously downloads files
//! - Outputs structured JSON summaries to `stdout`
//!
//! ### `server`
//! Runs an HTTP server with Server-Sent Events (SSE) endpoints.
//! - Provides real-time event streaming via `/events` SSE endpoint
//! - Offers file access via `/files` endpoints
//! - Exposes health and metrics endpoints for monitoring
//! - Delegates implementation details to `crate::live::server`
//!
//! ### `stream [--output-dir]`
//! Streams decoded events from live servers.
//! - Optional `--output-dir` writes completed files while streaming
//! - Outputs structured logs to `stderr` (no JSON payloads)
//!
//! ## Output Contract
//!
//! All commands follow a strict output separation:
//! - **`stdout`**: Command payloads (JSON for `download`; none for `stream`)
//! - **`stderr`**: Diagnostics, warnings, and structured logs
//!
//! This separation ensures that command output remains machine-readable
//! and parseable, while diagnostic information can be logged separately.
//!
//! ## Integration
//!
//! The `stream`, `download`, and `server` commands delegate to
//! the `crate::live` module, which provides the shared infrastructure
//! for connecting to ByteBlaster servers and managing client lifecycle.

pub mod download;
pub mod event_output;
pub mod server;
pub mod stream;
