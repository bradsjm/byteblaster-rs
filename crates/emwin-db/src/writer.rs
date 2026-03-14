use crate::error::PersistResult;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One blob to be written by a storage backend.
#[derive(Debug)]
pub struct BlobEntry {
    pub relative_path: String,
    pub bytes: Vec<u8>,
    pub content_type: Option<String>,
}

impl BlobEntry {
    /// Builds a blob entry using a backend-relative path and optional content type.
    pub fn new(
        relative_path: impl Into<String>,
        bytes: Vec<u8>,
        content_type: Option<&str>,
    ) -> Self {
        Self {
            relative_path: relative_path.into(),
            bytes,
            content_type: content_type.map(str::to_string),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobStorageKind {
    /// Blob stored on a local or mounted filesystem.
    Filesystem,
}

/// Stable reference returned after a blob has been persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredBlob {
    /// Storage backend that accepted the blob.
    pub kind: BlobStorageKind,
    /// Stable backend-specific location for later lookup.
    pub location: String,
    /// Number of persisted bytes.
    pub size_bytes: usize,
    /// Optional MIME type propagated from the enqueue request.
    pub content_type: Option<String>,
}

/// Writes raw payload blobs and returns stable references for metadata storage.
pub trait BlobWriter: Send + Sync + 'static {
    /// Persists a blob entry and returns the resulting storage reference.
    fn write<'a>(&'a self, entry: &'a BlobEntry) -> BoxFuture<'a, PersistResult<StoredBlob>>;
}

/// Filesystem-backed blob writer rooted at a configured directory.
#[derive(Debug, Clone)]
pub struct FilesystemBlobWriter {
    root: PathBuf,
}

impl FilesystemBlobWriter {
    /// Creates a filesystem writer rooted at the provided directory.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl BlobWriter for FilesystemBlobWriter {
    fn write<'a>(&'a self, entry: &'a BlobEntry) -> BoxFuture<'a, PersistResult<StoredBlob>> {
        let root = self.root.clone();
        let relative_path = entry.relative_path.clone();
        let bytes = entry.bytes.clone();
        let content_type = entry.content_type.clone();
        Box::pin(async move {
            let location = tokio::task::spawn_blocking(move || -> PersistResult<String> {
                let target = root.join(&relative_path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&target, &bytes)?;
                Ok(target.to_string_lossy().to_string())
            })
            .await??;

            Ok(StoredBlob {
                kind: BlobStorageKind::Filesystem,
                location,
                size_bytes: entry.bytes.len(),
                content_type,
            })
        })
    }
}
