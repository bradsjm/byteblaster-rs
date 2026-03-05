//! Segment stream for async segment processing.
//!
//! This module provides a simple channel-based stream for protocol segments.

use crate::qbt_receiver::protocol::model::QbtSegment;
use tokio::sync::mpsc;

/// A bidirectional channel for segment streaming.
///
/// This structure holds both the sender and receiver ends of a channel
/// for streaming QBT segments between components.
pub struct QbtSegmentStream {
    /// Sender end of the segment stream channel.
    pub tx: mpsc::Sender<QbtSegment>,
    /// Receiver end of the segment stream channel.
    pub rx: mpsc::Receiver<QbtSegment>,
}

impl QbtSegmentStream {
    /// Creates a new segment stream with the given buffer capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of segments to buffer
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self { tx, rx }
    }
}
