use crate::error::CoreError;
use crate::protocol::model::QbtSegment;
use bytes::Bytes;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone)]
pub struct CompletedFile {
    pub filename: String,
    pub data: Bytes,
}

pub trait SegmentAssembler {
    fn push(&mut self, segment: QbtSegment) -> Result<Option<CompletedFile>, CoreError>;
    fn clear(&mut self);
}

#[derive(Debug, Default)]
pub struct FileAssembler {
    by_key: HashMap<String, BTreeMap<u32, QbtSegment>>,
    completed_recent: VecDeque<String>,
    completed_index: HashSet<String>,
    duplicate_cache_size: usize,
}

impl FileAssembler {
    pub fn new(duplicate_cache_size: usize) -> Self {
        Self {
            by_key: HashMap::new(),
            completed_recent: VecDeque::new(),
            completed_index: HashSet::new(),
            duplicate_cache_size: duplicate_cache_size.max(1),
        }
    }

    fn file_key(segment: &QbtSegment) -> String {
        let ts = segment
            .timestamp_utc
            .duration_since(UNIX_EPOCH)
            .map(|d| format!("{}.{:09}", d.as_secs(), d.subsec_nanos()))
            .unwrap_or_else(|_| "0.000000000".to_string());
        format!("{}_{}", segment.filename.to_lowercase(), ts)
    }

    fn remember_completed(&mut self, key: String) {
        if self.completed_index.contains(&key) {
            return;
        }
        self.completed_recent.push_back(key.clone());
        self.completed_index.insert(key);
        while self.completed_recent.len() > self.duplicate_cache_size {
            if let Some(old) = self.completed_recent.pop_front() {
                self.completed_index.remove(&old);
            }
        }
    }
}

impl SegmentAssembler for FileAssembler {
    fn push(&mut self, segment: QbtSegment) -> Result<Option<CompletedFile>, CoreError> {
        if segment.filename.eq_ignore_ascii_case("FILLFILE.TXT") {
            return Ok(None);
        }
        if segment.total_blocks == 0
            || segment.block_number == 0
            || segment.block_number > segment.total_blocks
        {
            return Ok(None);
        }

        let key = Self::file_key(&segment);
        if self.completed_index.contains(&key) {
            return Ok(None);
        }
        let total_blocks = segment.total_blocks;

        let entry = self.by_key.entry(key.clone()).or_default();
        entry.insert(segment.block_number, segment);

        if entry.len() as u32 != total_blocks {
            return Ok(None);
        }

        let filename = match entry.values().next() {
            Some(first) => first.filename.clone(),
            None => return Ok(None),
        };

        let mut buffer = Vec::new();
        for part in entry.values() {
            buffer.extend_from_slice(&part.content);
        }

        self.by_key.remove(&key);
        self.remember_completed(key);
        Ok(Some(CompletedFile {
            filename,
            data: Bytes::from(buffer),
        }))
    }

    fn clear(&mut self) {
        self.by_key.clear();
        self.completed_recent.clear();
        self.completed_index.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{FileAssembler, SegmentAssembler};
    use crate::protocol::model::{ProtocolVersion, QbtSegment};
    use bytes::Bytes;
    use std::time::SystemTime;

    fn seg(file: &str, block: u32, total: u32, content: &'static [u8]) -> QbtSegment {
        QbtSegment {
            filename: file.to_string(),
            block_number: block,
            total_blocks: total,
            content: Bytes::from_static(content),
            checksum: 0,
            length: content.len(),
            version: ProtocolVersion::V1,
            timestamp_utc: SystemTime::UNIX_EPOCH,
            source: None,
        }
    }

    #[test]
    fn reconstructs_when_all_blocks_arrive() {
        let mut asm = FileAssembler::new(100);
        assert!(
            asm.push(seg("a.txt", 1, 2, b"ABC"))
                .expect("push should succeed")
                .is_none()
        );
        let file = asm
            .push(seg("a.txt", 2, 2, b"DEF"))
            .expect("push should succeed")
            .expect("file should complete");
        assert_eq!(file.filename, "a.txt");
        assert_eq!(file.data, Bytes::from_static(b"ABCDEF"));
    }

    #[test]
    fn suppresses_duplicate_completed_files() {
        let mut asm = FileAssembler::new(10);
        let _ = asm
            .push(seg("dup.txt", 1, 1, b"X"))
            .expect("push should succeed");
        let dup = asm
            .push(seg("dup.txt", 1, 1, b"X"))
            .expect("push should succeed");
        assert!(dup.is_none());
    }

    #[test]
    fn skips_fillfile() {
        let mut asm = FileAssembler::new(10);
        let out = asm
            .push(seg("FILLFILE.TXT", 1, 1, b"ignored"))
            .expect("push should succeed");
        assert!(out.is_none());
    }
}
