//! Channel-backed stream for completed QBT files.

use crate::qbt_receiver::file::assembler::QbtCompletedFile;
use tokio::sync::mpsc;

/// Sender and receiver halves for a bounded completed-file stream.
pub struct QbtFileStream {
    /// Sender end of the file stream channel.
    pub tx: mpsc::Sender<QbtCompletedFile>,
    /// Receiver end of the file stream channel.
    pub rx: mpsc::Receiver<QbtCompletedFile>,
}

impl QbtFileStream {
    /// Creates a completed-file stream with the requested channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self { tx, rx }
    }
}
