//! Live network mode implementation for EMWIN CLI commands.
//!
//! This module provides the infrastructure for running EMWIN commands
//! by connecting to real EMWIN servers.
//!
//! ## Components
//!
//! - [`server`]: HTTP/SSE server implementation with real-time event streaming
//!   and file access endpoints
//! - [`stream`]: Live event streaming from connected EMWIN servers
//! - [`file_pipeline`]: File assembly pipeline for constructing complete files
//!   from incoming data blocks
//! - [`shared`]: Shared types and utilities used across live mode components
//! - [`server_support`]: Internal support functions for server operations
//!
//! ## Runtime Behavior
//!
//! The CLI:
//! 1. Connects to configured EMWIN servers using `emwin-protocol`
//! 2. Maintains a persistent connection with reconnection and failover
//! 3. Streams decoded events in real-time to consumers
//! 4. Optionally assembles files from data blocks
//! 5. Provides HTTP/SSE endpoints when running `server` mode
//!
//! ## Integration
//!
//! This module builds on top of `emwin-protocol`'s client runtime,
//! using `QbtReceiverClient` to manage connections and receive events.
//! It adapts core client events into CLI-specific outputs and HTTP/SSE
//! responses.

pub(crate) mod config;
pub(crate) mod file_pipeline;
pub(crate) mod filter;
pub(crate) mod ingest;
pub mod server;
mod server_support;
pub mod shared;
pub mod stream;
