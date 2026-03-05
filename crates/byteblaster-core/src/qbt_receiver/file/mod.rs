//! File assembly module for ByteBlaster.
//!
//! This module provides functionality for reconstructing complete files from
//! the segmented data blocks transmitted over the ByteBlaster protocol.
//!
//! ## File Assembly Process
//!
//! The ByteBlaster protocol splits files into multiple blocks (segments) that
//! may arrive:
//! - Out of order (non-sequential block numbers)
//! - Interleaved with blocks from other files
//! - Across multiple reconnection sessions
//!
//! This module handles these complexities by:
//! - Tracking incomplete files in memory
//! - Accumulating segments regardless of arrival order
//! - Detecting file completion when all expected blocks are received
//! - Emitting [`QbtCompletedFile`] events for fully assembled files
//!
//! ## Duplicate Suppression
//!
//! The assembly system automatically suppresses duplicate blocks:
//! - If a block with the same filename and block number is already received,
//!   it is ignored
//! - This protects against retransmissions and prevents data corruption
//!
//! ## Types
//!
//! - [`QbtSegmentAssembler`]: Low-level assembler that accumulates segments
//!   and emits completion events for individual files
//! - [`QbtFileAssembler`]: Higher-level convenience wrapper around [`QbtSegmentAssembler`]
//!   with built-in file writing to disk
//! - [`QbtCompletedFile`]: Result of successful assembly containing filename,
//!   size, timestamp, and the assembled bytes
//!
//! ## Memory Management
//!
//! Incomplete files are retained in memory until:
//! - All expected blocks are received (file completion)
//! - The assembler is explicitly reset
//! - Memory limits are reached (when using higher-level managers)
//!
//! This approach ensures that files can be reconstructed even if segments
//! arrive over extended periods or across connection interruptions.

pub mod assembler;

pub use assembler::{QbtCompletedFile, QbtFileAssembler, QbtSegmentAssembler};
