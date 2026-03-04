//! File assembly from ByteBlaster protocol segments.
//!
//! This module provides functionality for assembling complete files from
//! individual data segments received over the ByteBlaster protocol.

use crate::error::CoreError;
use crate::protocol::model::QbtSegment;
use bytes::Bytes;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Default maximum number of files being assembled concurrently.
const DEFAULT_MAX_INFLIGHT_FILES: usize = 256;

/// Default TTL for inflight file entries (in seconds).
const DEFAULT_INFLIGHT_TTL_SECS: u64 = 90;

/// A completed file with its filename and data.
#[derive(Debug, Clone)]
pub struct CompletedFile {
    /// The filename of the completed file.
    pub filename: String,
    /// The complete file data.
    pub data: Bytes,
    /// Protocol timestamp from the /FD frame header (UTC).
    pub timestamp_utc: SystemTime,
}

/// Trait for segment assemblers that can collect and assemble file segments.
pub trait SegmentAssembler {
    /// Pushes a segment into the assembler.
    ///
    /// # Arguments
    ///
    /// * `segment` - The segment to add
    ///
    /// # Returns
    ///
    /// `Some(CompletedFile)` if this segment completes a file, `None` otherwise
    fn push(&mut self, segment: QbtSegment) -> Result<Option<CompletedFile>, CoreError>;

    /// Clears all in-progress and completed files.
    fn clear(&mut self);
}

/// File assembler that collects segments and produces complete files.
///
/// This assembler:
/// - Tracks in-progress files by filename and timestamp
/// - Handles out-of-order segment arrival
/// - Suppresses duplicate completed files
/// - Evicts stale entries based on TTL
/// - Limits concurrent inflight files
#[derive(Debug, Default)]
pub struct FileAssembler {
    /// Inflight files being assembled, keyed by file_key().
    by_key: HashMap<String, InflightFile>,
    /// Recently completed file keys (for duplicate suppression).
    completed_recent: VecDeque<String>,
    /// Set of completed file keys for fast lookup.
    completed_index: HashSet<String>,
    /// Maximum number of completed files to remember.
    duplicate_cache_size: usize,
    /// Maximum number of concurrent inflight files.
    max_inflight_files: usize,
    /// TTL for inflight file entries.
    inflight_ttl: Duration,
}

/// Inflight file being assembled from segments.
#[derive(Debug)]
struct InflightFile {
    /// Total number of blocks expected.
    total_blocks: u32,
    /// Received blocks keyed by block number.
    parts: BTreeMap<u32, QbtSegment>,
    /// Last time a segment was received for this file.
    last_seen: Instant,
}

impl FileAssembler {
    /// Creates a new file assembler with default limits.
    ///
    /// # Arguments
    ///
    /// * `duplicate_cache_size` - Number of completed files to remember for duplicate suppression
    pub fn new(duplicate_cache_size: usize) -> Self {
        Self::with_limits(
            duplicate_cache_size,
            DEFAULT_MAX_INFLIGHT_FILES,
            Duration::from_secs(DEFAULT_INFLIGHT_TTL_SECS),
        )
    }

    /// Creates a new file assembler with custom limits.
    ///
    /// # Arguments
    ///
    /// * `duplicate_cache_size` - Number of completed files to remember
    /// * `max_inflight_files` - Maximum concurrent files being assembled
    /// * `inflight_ttl` - Time-to-live for inactive inflight files
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

    /// Generates a unique key for a segment based on filename and timestamp.
    fn file_key(segment: &QbtSegment) -> String {
        let ts = segment
            .timestamp_utc
            .duration_since(UNIX_EPOCH)
            .map(|d| format!("{}.{:09}", d.as_secs(), d.subsec_nanos()))
            .unwrap_or_else(|_| "0.000000000".to_string());
        format!("{}_{}", segment.filename.to_lowercase(), ts)
    }

    /// Remembers a completed file key for duplicate suppression.
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

    /// Evicts inflight entries that have exceeded the TTL.
    fn evict_stale_inflight(&mut self, now: Instant) {
        let ttl = self.inflight_ttl;
        self.by_key
            .retain(|_, inflight| now.duration_since(inflight.last_seen) <= ttl);
    }

    /// Evicts oldest inflight entries to make room for new ones.
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

        let (filename, timestamp_utc) = match entry.parts.values().next() {
            Some(first) => (first.filename.clone(), first.timestamp_utc),
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
            timestamp_utc,
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
