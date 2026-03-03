use crate::file::assembler::CompletedFile;
use tokio::sync::mpsc;

pub struct FileStream {
    pub tx: mpsc::Sender<CompletedFile>,
    pub rx: mpsc::Receiver<CompletedFile>,
}

impl FileStream {
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self { tx, rx }
    }
}
