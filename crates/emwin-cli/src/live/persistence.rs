//! CLI adapters for the async persistence runtime.
//!
//! This module keeps storage runtime wiring out of command handlers while allowing live modes to
//! enqueue completed products without waiting for backend I/O.

use crate::live::file_pipeline::{CompletedFileMetadata, build_persist_request};
use emwin_db::{
    FilesystemBlobWriter, NoopMetadataSink, PersistRequest, PersistenceConfig, PersistenceProducer,
    PersistenceRuntime, PersistenceStats,
};
use std::path::PathBuf;
use tracing::warn;

pub(crate) type FilePersistenceRuntime = PersistenceRuntime<CompletedFileMetadata>;
pub(crate) type FilePersistenceProducer = PersistenceProducer<CompletedFileMetadata>;

pub(crate) fn start_filesystem_runtime(
    output_dir: PathBuf,
    queue_capacity: usize,
) -> FilePersistenceRuntime {
    PersistenceRuntime::spawn(
        PersistenceConfig::new(queue_capacity),
        FilesystemBlobWriter::new(output_dir),
        NoopMetadataSink,
    )
}

pub(crate) fn enqueue_completed_product(
    producer: &FilePersistenceProducer,
    filename: &str,
    data: &[u8],
    metadata: CompletedFileMetadata,
) -> crate::error::CliResult<bool> {
    let request: PersistRequest<CompletedFileMetadata> =
        build_persist_request(filename, data, metadata)?;
    let result = producer.enqueue(request);
    if let Some(evicted_oldest_key) = result.evicted_oldest_key {
        warn!(
            evicted_request = %evicted_oldest_key,
            queued_request = %filename,
            queue_len = result.queue_len,
            "persistence queue evicted oldest request"
        );
    }
    Ok(result.accepted)
}

pub(crate) async fn shutdown_runtime(
    runtime: FilePersistenceRuntime,
) -> crate::error::CliResult<PersistenceStats> {
    runtime.shutdown().await.map_err(Into::into)
}
