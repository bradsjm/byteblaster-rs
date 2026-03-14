use thiserror::Error;

/// Result type used by persistence runtime operations.
pub type PersistResult<T> = std::result::Result<T, PersistError>;

/// Errors produced by the async persistence runtime and blob writers.
#[derive(Debug, Error)]
pub enum PersistError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("persistence runtime is closed")]
    Closed,
    #[error("invalid persistence config: {0}")]
    InvalidConfig(String),
    #[error("invalid persistence request: {0}")]
    InvalidRequest(String),
}
