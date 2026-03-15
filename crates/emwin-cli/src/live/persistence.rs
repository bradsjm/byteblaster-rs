//! CLI adapters for the async persistence runtime.
//!
//! This module keeps storage runtime wiring out of command handlers while allowing live modes to
//! enqueue completed products without waiting for backend I/O.

use crate::live::file_pipeline::build_persist_request;
use emwin_db::{
    CompletedFileMetadata, FilesystemBlobWriter, IncidentCleanupResult, NoopMetadataSink,
    PersistRequest, PersistenceConfig, PersistenceProducer, PersistenceRuntime, PersistenceStats,
    PostgresConfig, PostgresMetadataSink,
};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::watch;
use tracing::warn;

pub(crate) type FilePersistenceRuntime = PersistenceRuntime<CompletedFileMetadata>;
pub(crate) type FilePersistenceProducer = PersistenceProducer<CompletedFileMetadata>;

pub(crate) const INCIDENT_CLEANUP_INTERVAL: Duration = Duration::from_secs(300);

pub(crate) struct StartedPersistenceRuntime {
    pub(crate) runtime: FilePersistenceRuntime,
    pub(crate) postgres_sink: Option<PostgresMetadataSink>,
}

pub(crate) async fn start_runtime_with_postgres(
    output_dir: PathBuf,
    queue_capacity: usize,
    postgres_database_url: Option<&str>,
    application_name: &str,
) -> crate::error::CliResult<StartedPersistenceRuntime> {
    let writer = FilesystemBlobWriter::new(output_dir);
    let (runtime, postgres_sink) = if let Some(database_url) = postgres_database_url {
        let mut config = PostgresConfig::new(database_url);
        config.application_name = application_name.to_string();
        let sink = PostgresMetadataSink::new(config);
        (
            PersistenceRuntime::spawn(PersistenceConfig::new(queue_capacity), writer, sink.clone()),
            Some(sink),
        )
    } else {
        (
            PersistenceRuntime::spawn(
                PersistenceConfig::new(queue_capacity),
                writer,
                NoopMetadataSink,
            ),
            None,
        )
    };
    Ok(StartedPersistenceRuntime {
        runtime,
        postgres_sink,
    })
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

pub(crate) async fn run_incident_cleanup_loop(
    sink: PostgresMetadataSink,
    mut shutdown_rx: watch::Receiver<bool>,
) -> crate::error::CliResult<()> {
    let mut interval = tokio::time::interval(INCIDENT_CLEANUP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await;

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                break;
            }
            _ = interval.tick() => {
                match sink.expire_active_incidents(chrono::Utc::now()).await {
                    Ok(IncidentCleanupResult { expired_count }) => {
                        if expired_count > 0 {
                            tracing::info!(expired_count, "expired stale incidents");
                        }
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "incident cleanup pass failed");
                    }
                }
            }
        }
    }

    Ok(())
}
