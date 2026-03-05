//! File stream for async completed file processing.
//!
//! This module provides a simple channel-based stream for completed files.

use crate::qbt_receiver::file::assembler::QbtCompletedFile;
use tokio::sync::mpsc;

/// A bidirectional channel for completed file streaming.
///
/// This structure holds both the sender and receiver ends of a channel
/// for streaming completed files between components.
pub struct QbtFileStream {
    /// Sender end of the file stream channel.
    pub tx: mpsc::Sender<QbtCompletedFile>,
    /// Receiver end of the file stream channel.
    pub rx: mpsc::Receiver<QbtCompletedFile>,
}

impl QbtFileStream {
    /// Creates a new file stream with the given buffer capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of files to buffer
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self { tx, rx }
    }
}
