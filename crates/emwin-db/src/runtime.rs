use crate::error::{PersistError, PersistResult};
use crate::writer::{BlobEntry, BlobWriter, BoxFuture, StoredBlob};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tracing::{info, warn};

#[derive(Debug, Clone, Copy)]
pub struct PersistenceConfig {
    /// Maximum number of queued requests kept in memory.
    pub queue_capacity: usize,
}

impl PersistenceConfig {
    /// Creates a persistence config, coercing zero capacity to one.
    pub fn new(queue_capacity: usize) -> Self {
        Self {
            queue_capacity: queue_capacity.max(1),
        }
    }
}

#[derive(Debug)]
pub struct PersistRequest<M> {
    /// Stable identifier used in logs, metrics, and eviction reporting.
    pub request_key: String,
    /// Caller-provided metadata handed to the sink after blob persistence succeeds.
    pub metadata: M,
    /// Raw payloads to persist before metadata commit.
    pub blobs: Vec<BlobEntry>,
}

/// Completed persistence request passed to the metadata sink.
#[derive(Debug)]
pub struct PersistedRequest<M> {
    /// Stable identifier copied from the original request.
    pub request_key: String,
    /// Caller-provided metadata.
    pub metadata: M,
    /// Stable references to the persisted blobs.
    pub blobs: Vec<StoredBlob>,
}

/// Persists metadata after all referenced blobs have been written successfully.
pub trait MetadataSink<M>: Send + Sync + 'static {
    /// Commits metadata and blob references for one completed request.
    fn persist<'a>(&'a self, request: PersistedRequest<M>) -> BoxFuture<'a, PersistResult<()>>;
}

/// Metadata sink that intentionally discards metadata writes.
#[derive(Debug, Default)]
pub struct NoopMetadataSink;

impl<M: Send + 'static> MetadataSink<M> for NoopMetadataSink {
    fn persist<'a>(&'a self, _request: PersistedRequest<M>) -> BoxFuture<'a, PersistResult<()>> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PersistenceStats {
    /// Number of requests currently waiting in the queue.
    pub queue_len: usize,
    /// Maximum number of requests the queue can hold before eviction starts.
    pub queue_capacity: usize,
    /// Number of requests accepted by producers.
    pub enqueued_total: u64,
    /// Number of queued requests evicted to admit newer work.
    pub evicted_total: u64,
    /// Number of requests fully persisted.
    pub persisted_total: u64,
    /// Number of requests that failed during blob or metadata persistence.
    pub failed_total: u64,
}

/// Result returned to producers after enqueueing a persistence request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueueResult {
    /// Whether the request was accepted into the queue.
    pub accepted: bool,
    /// Key of the evicted request when the queue was full.
    pub evicted_oldest_key: Option<String>,
    /// Queue length after enqueue processing completes.
    pub queue_len: usize,
}

#[derive(Debug)]
struct SharedQueue<M> {
    state: Mutex<QueueState<M>>,
    available: Semaphore,
    capacity: usize,
}

#[derive(Debug)]
struct QueueState<M> {
    pending: VecDeque<PersistRequest<M>>,
    closed: bool,
    stats: PersistenceStats,
}

/// Cloneable producer used by ingest code to enqueue background persistence work.
#[derive(Debug)]
pub struct PersistenceProducer<M> {
    shared: Arc<SharedQueue<M>>,
}

impl<M> Clone for PersistenceProducer<M> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<M> PersistenceProducer<M> {
    /// Attempts to enqueue a request without blocking the caller on backend I/O.
    pub fn enqueue(&self, request: PersistRequest<M>) -> EnqueueResult {
        let mut guard = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.closed {
            return EnqueueResult {
                accepted: false,
                evicted_oldest_key: None,
                queue_len: guard.pending.len(),
            };
        }

        let evicted_oldest_key = if guard.pending.len() == self.shared.capacity {
            guard.stats.evicted_total = guard.stats.evicted_total.saturating_add(1);
            guard.pending.pop_front().map(|item| item.request_key)
        } else {
            guard.stats.enqueued_total = guard.stats.enqueued_total.saturating_add(1);
            self.shared.available.add_permits(1);
            None
        };

        guard.pending.push_back(request);
        if evicted_oldest_key.is_some() {
            guard.stats.enqueued_total = guard.stats.enqueued_total.saturating_add(1);
        }

        EnqueueResult {
            accepted: true,
            evicted_oldest_key,
            queue_len: guard.pending.len(),
        }
    }

    /// Returns a point-in-time snapshot of queue depth and cumulative outcomes.
    pub fn stats_snapshot(&self) -> PersistenceStats {
        let guard = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        PersistenceStats {
            queue_len: guard.pending.len(),
            queue_capacity: self.shared.capacity,
            ..guard.stats
        }
    }

    fn close(&self) {
        let mut guard = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.closed {
            return;
        }
        guard.closed = true;
        self.shared.available.add_permits(1);
    }
}

/// Background runtime draining queued persistence work.
#[derive(Debug)]
pub struct PersistenceRuntime<M> {
    producer: PersistenceProducer<M>,
    task: JoinHandle<PersistenceStats>,
}

impl<M: Send + 'static> PersistenceRuntime<M> {
    /// Spawns a background worker that drains queued requests until shutdown.
    pub fn spawn<W, S>(config: PersistenceConfig, writer: W, sink: S) -> Self
    where
        W: BlobWriter,
        S: MetadataSink<M>,
    {
        let shared = Arc::new(SharedQueue {
            state: Mutex::new(QueueState {
                pending: VecDeque::with_capacity(config.queue_capacity),
                closed: false,
                stats: PersistenceStats::default(),
            }),
            available: Semaphore::new(0),
            capacity: config.queue_capacity.max(1),
        });
        let producer = PersistenceProducer {
            shared: Arc::clone(&shared),
        };
        let worker_producer = producer.clone();
        let task = tokio::spawn(async move { run_worker(shared, writer, sink).await });

        info!(
            queue_capacity = config.queue_capacity.max(1),
            "persistence runtime started"
        );

        Self {
            producer: worker_producer,
            task,
        }
    }

    /// Returns a cloneable producer handle for hot-path enqueue operations.
    pub fn producer(&self) -> PersistenceProducer<M> {
        self.producer.clone()
    }

    /// Returns a point-in-time snapshot of queue depth and cumulative outcomes.
    pub fn stats_snapshot(&self) -> PersistenceStats {
        self.producer.stats_snapshot()
    }

    /// Closes the queue, drains remaining requests, and returns final runtime stats.
    pub async fn shutdown(self) -> PersistResult<PersistenceStats> {
        self.producer.close();
        Ok(self.task.await?)
    }
}

async fn run_worker<M, W, S>(shared: Arc<SharedQueue<M>>, writer: W, sink: S) -> PersistenceStats
where
    M: Send + 'static,
    W: BlobWriter,
    S: MetadataSink<M>,
{
    let producer = PersistenceProducer {
        shared: Arc::clone(&shared),
    };

    loop {
        match shared.available.acquire().await {
            Ok(permit) => permit.forget(),
            Err(_) => break,
        }

        let Some(request) = pop_request(&producer) else {
            if is_closed(&producer) {
                break;
            }
            continue;
        };

        match persist_request(&writer, &sink, request).await {
            Ok(request_key) => {
                let mut guard = producer
                    .shared
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.stats.persisted_total = guard.stats.persisted_total.saturating_add(1);
                info!(request_key = %request_key, "persistence request completed");
            }
            Err((request_key, err)) => {
                let mut guard = producer
                    .shared
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.stats.failed_total = guard.stats.failed_total.saturating_add(1);
                warn!(request_key = %request_key, error = %err, "persistence request failed");
            }
        }
    }

    let stats = producer.stats_snapshot();
    info!(
        queue_len = stats.queue_len,
        queue_capacity = stats.queue_capacity,
        enqueued_total = stats.enqueued_total,
        evicted_total = stats.evicted_total,
        persisted_total = stats.persisted_total,
        failed_total = stats.failed_total,
        "persistence runtime stopped"
    );
    stats
}

fn pop_request<M>(producer: &PersistenceProducer<M>) -> Option<PersistRequest<M>> {
    let mut guard = producer
        .shared
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.pending.pop_front()
}

fn is_closed<M>(producer: &PersistenceProducer<M>) -> bool {
    let guard = producer
        .shared
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.closed && guard.pending.is_empty()
}

async fn persist_request<M, W, S>(
    writer: &W,
    sink: &S,
    request: PersistRequest<M>,
) -> Result<String, (String, PersistError)>
where
    M: Send + 'static,
    W: BlobWriter,
    S: MetadataSink<M>,
{
    let PersistRequest {
        request_key,
        metadata,
        blobs,
    } = request;

    let mut stored_blobs = Vec::with_capacity(blobs.len());
    for blob in &blobs {
        let stored = writer
            .write(blob)
            .await
            .map_err(|err| (request_key.clone(), err))?;
        stored_blobs.push(stored);
    }

    if let Err(err) = sink
        .persist(PersistedRequest {
            request_key: request_key.clone(),
            metadata,
            blobs: stored_blobs,
        })
        .await
    {
        return Err((request_key, err));
    }

    Ok(request_key)
}

#[cfg(test)]
mod tests {
    use super::{
        BlobEntry, EnqueueResult, MetadataSink, NoopMetadataSink, PersistRequest, PersistedRequest,
        PersistenceConfig, PersistenceProducer, PersistenceRuntime, PersistenceStats, QueueState,
        SharedQueue,
    };
    use crate::error::{PersistError, PersistResult};
    use crate::writer::{BlobRole, BlobStorageKind, BlobWriter, BoxFuture, FilesystemBlobWriter};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
    use tokio::sync::Semaphore;

    #[derive(Debug, Default)]
    struct RecordingWriter {
        deletes: Arc<Mutex<Vec<String>>>,
        writes: Arc<Mutex<Vec<String>>>,
    }

    impl BlobWriter for RecordingWriter {
        fn write<'a>(
            &'a self,
            entry: &'a BlobEntry,
        ) -> BoxFuture<'a, PersistResult<crate::writer::StoredBlob>> {
            let writes = Arc::clone(&self.writes);
            Box::pin(async move {
                writes
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(entry.relative_path.clone());
                Ok(crate::writer::StoredBlob {
                    kind: BlobStorageKind::Filesystem,
                    role: entry.role,
                    location: entry.relative_path.clone(),
                    size_bytes: entry.bytes.len(),
                    content_type: entry.content_type.clone(),
                })
            })
        }

        fn delete<'a>(
            &'a self,
            blob: &'a crate::writer::StoredBlob,
        ) -> BoxFuture<'a, PersistResult<()>> {
            let deletes = Arc::clone(&self.deletes);
            Box::pin(async move {
                deletes
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(blob.location.clone());
                Ok(())
            })
        }
    }

    #[derive(Debug, Default)]
    struct RecordingSink {
        persisted: Arc<Mutex<Vec<String>>>,
    }

    impl MetadataSink<String> for RecordingSink {
        fn persist<'a>(
            &'a self,
            request: PersistedRequest<String>,
        ) -> BoxFuture<'a, PersistResult<()>> {
            let persisted = Arc::clone(&self.persisted);
            Box::pin(async move {
                persisted
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(request.request_key);
                Ok(())
            })
        }
    }

    #[derive(Debug, Default)]
    struct FailingWriter;

    impl BlobWriter for FailingWriter {
        fn write<'a>(
            &'a self,
            _entry: &'a BlobEntry,
        ) -> BoxFuture<'a, PersistResult<crate::writer::StoredBlob>> {
            Box::pin(async { Err(PersistError::Io(std::io::Error::other("boom"))) })
        }

        fn delete<'a>(
            &'a self,
            _blob: &'a crate::writer::StoredBlob,
        ) -> BoxFuture<'a, PersistResult<()>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[derive(Debug, Default)]
    struct FailingSink;

    impl MetadataSink<String> for FailingSink {
        fn persist<'a>(
            &'a self,
            _request: PersistedRequest<String>,
        ) -> BoxFuture<'a, PersistResult<()>> {
            Box::pin(async { Err(PersistError::InvalidRequest("sink failed".to_string())) })
        }
    }

    fn request(name: &str) -> PersistRequest<String> {
        PersistRequest {
            request_key: name.to_string(),
            metadata: name.to_string(),
            blobs: vec![BlobEntry::new(
                BlobRole::Payload,
                name,
                name.as_bytes().to_vec(),
                Some("text/plain"),
            )],
        }
    }

    #[test]
    fn stats_snapshot_reports_live_queue_state() {
        let producer = PersistenceProducer {
            shared: Arc::new(SharedQueue {
                state: Mutex::new(QueueState {
                    pending: VecDeque::from([request("queued")]),
                    closed: false,
                    stats: PersistenceStats {
                        queue_len: 0,
                        queue_capacity: 0,
                        enqueued_total: 4,
                        evicted_total: 1,
                        persisted_total: 2,
                        failed_total: 1,
                    },
                }),
                available: Semaphore::new(0),
                capacity: 8,
            }),
        };

        assert_eq!(
            producer.stats_snapshot(),
            PersistenceStats {
                queue_len: 1,
                queue_capacity: 8,
                enqueued_total: 4,
                evicted_total: 1,
                persisted_total: 2,
                failed_total: 1,
            }
        );
    }

    #[tokio::test]
    async fn queue_evicts_oldest_request_when_full() {
        let writer = RecordingWriter::default();
        let writes = Arc::clone(&writer.writes);
        let sink = RecordingSink::default();
        let persisted = Arc::clone(&sink.persisted);
        let runtime = PersistenceRuntime::spawn(PersistenceConfig::new(2), writer, sink);
        let producer = runtime.producer();

        assert_eq!(
            producer.enqueue(request("one")),
            EnqueueResult {
                accepted: true,
                evicted_oldest_key: None,
                queue_len: 1,
            }
        );
        assert_eq!(
            producer.enqueue(request("two")),
            EnqueueResult {
                accepted: true,
                evicted_oldest_key: None,
                queue_len: 2,
            }
        );
        let result = producer.enqueue(request("three"));
        assert_eq!(result.evicted_oldest_key.as_deref(), Some("one"));

        let stats = runtime.shutdown().await.expect("shutdown should succeed");
        assert_eq!(stats.queue_len, 0);
        assert_eq!(stats.queue_capacity, 2);
        assert_eq!(stats.evicted_total, 1);
        assert_eq!(stats.persisted_total, 2);
        assert_eq!(
            writes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_slice(),
            &["two", "three"]
        );
        assert_eq!(
            persisted
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_slice(),
            &["two", "three"]
        );
    }

    #[tokio::test]
    async fn writer_failure_does_not_persist_metadata() {
        let sink = RecordingSink::default();
        let persisted = Arc::clone(&sink.persisted);
        let runtime = PersistenceRuntime::spawn(PersistenceConfig::new(4), FailingWriter, sink);
        let producer = runtime.producer();

        let result = producer.enqueue(request("broken"));
        assert!(result.accepted);

        let stats = runtime.shutdown().await.expect("shutdown should succeed");
        assert_eq!(stats.queue_len, 0);
        assert_eq!(stats.failed_total, 1);
        assert!(
            persisted
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_empty()
        );
    }

    #[tokio::test]
    async fn sink_failure_keeps_written_blobs() {
        let writer = RecordingWriter::default();
        let deletes = Arc::clone(&writer.deletes);
        let runtime = PersistenceRuntime::spawn(PersistenceConfig::new(4), writer, FailingSink);
        let producer = runtime.producer();

        let result = producer.enqueue(request("broken"));
        assert!(result.accepted);

        let stats = runtime.shutdown().await.expect("shutdown should succeed");
        assert_eq!(stats.queue_len, 0);
        assert_eq!(stats.failed_total, 1);
        assert!(
            deletes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_empty()
        );
    }

    #[tokio::test]
    async fn filesystem_writer_keeps_blobs_when_sink_fails() {
        let temp = tempdir().expect("tempdir should succeed");
        let runtime = PersistenceRuntime::spawn(
            PersistenceConfig::new(4),
            FilesystemBlobWriter::new(temp.path().to_path_buf()),
            FailingSink,
        );
        let producer = runtime.producer();

        let result = producer.enqueue(PersistRequest {
            request_key: "product".to_string(),
            metadata: "product".to_string(),
            blobs: vec![
                BlobEntry::new(
                    BlobRole::Payload,
                    "nested/product.txt",
                    b"payload".to_vec(),
                    Some("text/plain"),
                ),
                BlobEntry::new(
                    BlobRole::MetadataSidecar,
                    "nested/product.JSON",
                    br#"{"ok":true}"#.to_vec(),
                    Some("application/json"),
                ),
            ],
        });
        assert!(result.accepted);

        let stats = runtime.shutdown().await.expect("shutdown should succeed");
        assert_eq!(stats.queue_len, 0);
        assert_eq!(stats.failed_total, 1);
        assert_eq!(
            std::fs::read_to_string(temp.path().join("nested/product.txt"))
                .expect("payload should exist"),
            "payload"
        );
        assert_eq!(
            std::fs::read_to_string(temp.path().join("nested/product.JSON"))
                .expect("metadata should exist"),
            "{\"ok\":true}"
        );
    }

    #[tokio::test]
    async fn filesystem_writer_persists_blobs() {
        let temp = tempdir().expect("tempdir should succeed");
        let runtime = PersistenceRuntime::spawn(
            PersistenceConfig::new(4),
            FilesystemBlobWriter::new(temp.path().to_path_buf()),
            NoopMetadataSink,
        );
        let producer = runtime.producer();

        let result = producer.enqueue(PersistRequest {
            request_key: "product".to_string(),
            metadata: (),
            blobs: vec![
                BlobEntry::new(
                    BlobRole::Payload,
                    "nested/product.txt",
                    b"payload".to_vec(),
                    Some("text/plain"),
                ),
                BlobEntry::new(
                    BlobRole::MetadataSidecar,
                    "nested/product.JSON",
                    br#"{"ok":true}"#.to_vec(),
                    Some("application/json"),
                ),
            ],
        });
        assert!(result.accepted);

        let stats = runtime.shutdown().await.expect("shutdown should succeed");
        assert_eq!(stats.queue_len, 0);
        assert_eq!(stats.persisted_total, 1);
        assert_eq!(
            std::fs::read_to_string(temp.path().join("nested/product.txt"))
                .expect("payload should exist"),
            "payload"
        );
        assert_eq!(
            std::fs::read_to_string(temp.path().join("nested/product.JSON"))
                .expect("metadata should exist"),
            "{\"ok\":true}"
        );
    }
}
