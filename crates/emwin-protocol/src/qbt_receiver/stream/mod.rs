//! Stream abstractions for EMWIN.
//!
//! This module provides async stream types that integrate with the Rust
//! ecosystem's streaming capabilities for file and segment processing.
//!
//! ## Stream Types
//!
//! - `segment_stream`: Provides an async stream of `QbtSegment` events
//!   from a client, filtering for data-bearing frames and converting them
//!   to segments with metadata
//! - `file_stream`: Provides an async stream of `QbtCompletedFile` events
//!   by combining a `QbtSegmentStream` with a `QbtFileAssembler`, emitting
//!   fully assembled files as they complete
//!
//! ## Usage Pattern
//!
//! These streams are designed to be used with Tokio's async runtime and
//! can be composed with other stream operations. The typical flow is:
//!
//! 1. Create stream channels with `QbtSegmentStream` and `QbtFileStream`
//! 2. Forward decoded segments into the segment stream sender
//! 3. Assemble and forward completed files into the file stream sender
//! 4. Process the resulting file stream receiver (write to disk, upload, etc.)
//!
//! ## Backpressure Handling
//!
//! Both stream types respect backpressure by relying on bounded channels
//! in the underlying client and event processing. This prevents memory
//! issues when processing high-throughput feeds.
//!
//! ## Example
//!
//! ```rust
//! use emwin_protocol::unstable::qbt_receiver::{QbtFileStream, QbtSegmentStream};
//!
//! let segment_stream = QbtSegmentStream::new(1_024);
//! let file_stream = QbtFileStream::new(256);
//!
//! let _segment_tx = segment_stream.tx.clone();
//! let _file_tx = file_stream.tx.clone();
//! ```

pub mod file_stream;
pub mod segment_stream;
