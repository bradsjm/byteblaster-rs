use crate::error::CoreError;
use crate::protocol::model::QbtSegment;
use bytes::Bytes;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant, UNIX_EPOCH};

const DEFAULT_MAX_INFLIGHT_FILES: usize = 256;
const DEFAULT_INFLIGHT_TTL_SECS: u64 = 90;

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
    by_key: HashMap<String, InflightFile>,
    completed_recent: VecDeque<String>,
    completed_index: HashSet<String>,
    duplicate_cache_size: usize,
    max_inflight_files: usize,
    inflight_ttl: Duration,
}

#[derive(Debug)]
struct InflightFile {
    total_blocks: u32,
    parts: BTreeMap<u32, QbtSegment>,
    last_seen: Instant,
}

impl FileAssembler {
    pub fn new(duplicate_cache_size: usize) -> Self {
        Self::with_limits(
            duplicate_cache_size,
            DEFAULT_MAX_INFLIGHT_FILES,
            Duration::from_secs(DEFAULT_INFLIGHT_TTL_SECS),
        )
    }

    pub fn with_limits(
        duplicate_cache_size: usize,
        max_inflight_files: usize,
        inflight_ttl: Duration,
    ) -> Self {
        Self {
            by_key: HashMap::new(),
            completed_recent: VecDeque::new(),
            completed_index: HashSet::new(),
            duplicate_cache_size: duplicate_cache_size.max(1),
            max_inflight_files: max_inflight_files.max(1),
            inflight_ttl: inflight_ttl.max(Duration::from_millis(1)),
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

    fn evict_stale_inflight(&mut self, now: Instant) {
        let ttl = self.inflight_ttl;
        self.by_key
            .retain(|_, inflight| now.duration_since(inflight.last_seen) <= ttl);
    }

    fn evict_overflow_inflight(&mut self, min_capacity: usize) {
        while self.by_key.len() >= min_capacity {
            let Some(oldest_key) = self
                .by_key
                .iter()
                .min_by_key(|(_, inflight)| inflight.last_seen)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.by_key.remove(&oldest_key);
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

        let now = Instant::now();
        self.evict_stale_inflight(now);
        if !self.by_key.contains_key(&key) {
            self.evict_overflow_inflight(self.max_inflight_files);
        }

        let total_blocks = segment.total_blocks;

        let entry = self
            .by_key
            .entry(key.clone())
            .or_insert_with(|| InflightFile {
                total_blocks,
                parts: BTreeMap::new(),
                last_seen: now,
            });
        entry.total_blocks = total_blocks;
        entry.last_seen = now;
        entry.parts.insert(segment.block_number, segment);

        if entry.parts.len() as u32 != total_blocks {
            return Ok(None);
        }

        let filename = match entry.parts.values().next() {
            Some(first) => first.filename.clone(),
            None => return Ok(None),
        };

        let mut buffer = Vec::new();
        for part in entry.parts.values() {
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
    use std::time::{Duration, SystemTime};

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

    #[test]
    fn inflight_entries_expire_by_ttl() {
        let mut asm = FileAssembler::with_limits(10, 10, Duration::from_millis(1));
        assert!(
            asm.push(seg("ttl.txt", 1, 2, b"A"))
                .expect("push should succeed")
                .is_none()
        );

        std::thread::sleep(Duration::from_millis(5));
        let out = asm
            .push(seg("ttl.txt", 2, 2, b"B"))
            .expect("push should succeed");
        assert!(
            out.is_none(),
            "expired inflight should not complete old file"
        );
    }

    #[test]
    fn inflight_entries_are_bounded() {
        let mut asm = FileAssembler::with_limits(10, 1, Duration::from_secs(60));
        assert!(
            asm.push(seg("a.txt", 1, 2, b"A"))
                .expect("push should succeed")
                .is_none()
        );
        assert!(
            asm.push(seg("b.txt", 1, 2, b"B"))
                .expect("push should succeed")
                .is_none()
        );

        let out = asm
            .push(seg("a.txt", 2, 2, b"C"))
            .expect("push should succeed");
        assert!(
            out.is_none(),
            "older inflight should be evicted by max_inflight"
        );
    }
}
