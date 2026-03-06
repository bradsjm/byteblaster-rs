//! Command implementations for the ByteBlaster CLI.
//!
//! This module contains the implementation of each CLI subcommand.
//!
//! ## Available Commands
//!
//! ### `download <output_dir> [input]`
//! Downloads and assembles complete files from the ByteBlaster feed.
//! - Capture mode: processes a capture file and writes files to `output_dir`
//! - Live mode: connects to live servers and continuously downloads files
//! - Outputs structured JSON summaries to `stdout`
//!
//! ### `inspect [input]`
//! Decodes and displays events from a capture file.
//! - Supports both file input and stdin
//! - Outputs detailed event information in JSON format
//! - Useful for debugging and understanding capture file contents
//!
//! ### `server`
//! Runs an HTTP server with Server-Sent Events (SSE) endpoints.
//! - Provides real-time event streaming via `/events` SSE endpoint
//! - Offers file access via `/files` endpoints
//! - Exposes health and metrics endpoints for monitoring
//! - Delegates implementation details to `crate::live::server`
//!
//! ### `stream [input] [--output-dir]`
//! Streams decoded events from capture files or live servers.
//! - Capture mode: processes capture files and emits events
//! - Live mode: connects to live servers and streams real-time events
//! - Optional `--output-dir` writes completed files while streaming
//! - Outputs structured logs to `stderr` (no JSON payloads)
//!
//! ## Output Contract
//!
//! All commands follow a strict output separation:
//! - **`stdout`**: Command payloads (JSON for `inspect`, `download`; none for `stream`)
//! - **`stderr`**: Diagnostics, warnings, and structured logs
//!
//! This separation ensures that command output remains machine-readable
//! and parseable, while diagnostic information can be logged separately.
//!
//! ## Integration
//!
//! Most live-mode commands (`stream`, `download`, `server`) delegate to
//! the `crate::live` module, which provides the shared infrastructure
//! for connecting to ByteBlaster servers and managing client lifecycle.

pub mod capture;
pub mod download;
pub mod event_output;
pub mod inspect;
pub mod server;
pub mod stream;
