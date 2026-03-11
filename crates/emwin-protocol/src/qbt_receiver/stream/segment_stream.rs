//! Channel-backed stream for decoded QBT segments.

use crate::qbt_receiver::protocol::model::QbtSegment;
use tokio::sync::mpsc;

/// Sender and receiver halves for a bounded segment stream.
pub struct QbtSegmentStream {
    /// Sender end of the segment stream channel.
    pub tx: mpsc::Sender<QbtSegment>,
    /// Receiver end of the segment stream channel.
    pub rx: mpsc::Receiver<QbtSegment>,
}

impl QbtSegmentStream {
    /// Creates a segment stream with the requested channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self { tx, rx }
    }
}
