//! Async persistence runtime and pluggable blob storage backends.
//!
//! This crate keeps ingest producers off the storage hot path by queueing persistence requests for
//! a background worker. Producers enqueue completed products without awaiting filesystem, object
//! storage, or database I/O.

mod error;
mod metadata;
mod postgres;
mod runtime;
mod writer;

pub use error::{PersistError, PersistResult};
pub use metadata::CompletedFileMetadata;
pub use postgres::{IncidentCleanupResult, PostgresConfig, PostgresMetadataSink};
pub use runtime::{
    EnqueueResult, MetadataSink, NoopMetadataSink, PersistRequest, PersistedRequest,
    PersistenceConfig, PersistenceProducer, PersistenceRuntime, PersistenceStats,
};
pub use writer::{
    BlobEntry, BlobRole, BlobStorageKind, BlobWriter, FilesystemBlobWriter, S3BlobWriter,
    StoredBlob,
};
