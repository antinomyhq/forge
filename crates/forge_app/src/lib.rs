mod app;
mod attachment;
mod conversation;
mod provider;
mod template;
mod tool_service;
mod tools;

use std::path::Path;

pub use app::*;
use bytes::Bytes;
use forge_domain::{Point, Query, Suggestion};
use forge_snaps::FileSnapshotService;

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
pub trait FileReadService: Send + Sync {
    /// Reads the content of a file at the specified path.
    async fn read(&self, path: &Path) -> anyhow::Result<Bytes>;
}

#[async_trait::async_trait]
pub trait FileWriteService: Send + Sync {
    /// Writes the content of a file at the specified path.
    async fn write(&self, path: &Path, contents: Bytes) -> anyhow::Result<()>;
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

pub trait Infrastructure: Send + Sync + 'static {
    type EnvironmentService: EnvironmentService;
    type FileReadService: FileReadService;
    type FileWriteService: FileWriteService;

    type VectorIndex: VectorIndex<Suggestion>;
    type EmbeddingService: EmbeddingService;
    type FileSnapshotService: FileSnapshotService;

    fn environment_service(&self) -> &Self::EnvironmentService;
    fn file_read_service(&self) -> &Self::FileReadService;
    fn file_write_service(&self) -> &Self::FileWriteService;
    fn vector_index(&self) -> &Self::VectorIndex;
    fn embedding_service(&self) -> &Self::EmbeddingService;
    fn file_snapshot_service(&self) -> &Self::FileSnapshotService;
}
