use std::path::Path;

use anyhow::Result;
use bytes::Bytes;
use forge_domain::{Point, Query, Suggestion};
use forge_snaps::Snapshot;

/// Repository for accessing system environment information
#[async_trait::async_trait]
pub trait EnvironmentService {
    /// Get the current environment information including:
    /// - Operating system
    /// - Current working directory
    /// - Home directory
    /// - Default shell
    fn get_environment(&self) -> forge_domain::Environment;
}

/// A service for reading files from the filesystem.
///
/// This trait provides an abstraction over file reading operations, allowing
/// for both real file system access and test mocking.
#[async_trait::async_trait]
pub trait FsReadService: Send + Sync {
    /// Reads the content of a file at the specified path.
    async fn read(&self, path: &Path) -> anyhow::Result<Bytes>;
}

#[async_trait::async_trait]
pub trait FsWriteService: Send + Sync {
    /// Writes the content of a file at the specified path.
    async fn write(&self, path: &Path, contents: Bytes) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait FileRemoveService: Send + Sync {
    /// Removes a file at the specified path.
    async fn remove(&self, path: &Path) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait VectorIndex<T>: Send + Sync {
    async fn store(&self, point: Point<T>) -> anyhow::Result<()>;
    async fn search(&self, query: Query) -> anyhow::Result<Vec<Point<T>>>;
}

#[async_trait::async_trait]
pub trait EmbeddingService: Send + Sync {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;
}

#[async_trait::async_trait]
pub trait FsMetaService: Send + Sync {
    async fn is_file(&self, path: &Path) -> anyhow::Result<bool>;
    async fn exists(&self, path: &Path) -> anyhow::Result<bool>;
}

#[async_trait::async_trait]
pub trait FsCreateDirsService {
    async fn create_dirs(&self, path: &Path) -> anyhow::Result<()>;
}

/// Service for managing file snapshots
#[async_trait::async_trait]
pub trait FsSnapshotService: Send + Sync {
    // Creation
    async fn create_snapshot(&self, file_path: &Path) -> Result<Snapshot>;
}

pub trait Infrastructure: Send + Sync + Clone + 'static {
    type EmbeddingService: EmbeddingService;
    type EnvironmentService: EnvironmentService;
    type FsMetaService: FsMetaService;
    type FsReadService: FsReadService;
    type FsRemoveService: FileRemoveService;
    type FsSnapshotService: FsSnapshotService;
    type FsWriteService: FsWriteService;
    type VectorIndex: VectorIndex<Suggestion>;
    type FsCreateDirsService: FsCreateDirsService;

    fn embedding_service(&self) -> &Self::EmbeddingService;
    fn environment_service(&self) -> &Self::EnvironmentService;
    fn file_meta_service(&self) -> &Self::FsMetaService;
    fn file_read_service(&self) -> &Self::FsReadService;
    fn file_remove_service(&self) -> &Self::FsRemoveService;
    fn file_snapshot_service(&self) -> &Self::FsSnapshotService;
    fn file_write_service(&self) -> &Self::FsWriteService;
    fn vector_index(&self) -> &Self::VectorIndex;
    fn create_dirs_service(&self) -> &Self::FsCreateDirsService;
}
