use crate::protocol::model::QbtSegment;
use tokio::sync::mpsc;

pub struct SegmentStream {
    pub tx: mpsc::Sender<QbtSegment>,
    pub rx: mpsc::Receiver<QbtSegment>,
}

impl SegmentStream {
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self { tx, rx }
    }
}
