mod service;
mod cli;

pub use service::FileSnapshotServiceImpl;
pub use cli::SnapshotCli;

use std::path::{Path, PathBuf};
use std::time::SystemTimeError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Snapshot not found: {0}")]
    NotFound(String),
    #[error("Invalid snapshot path: {0}")]
    InvalidPath(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("System time error: {0}")]
    SystemTime(#[from] SystemTimeError),
}

pub type Result<T> = std::result::Result<T, SnapshotError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub timestamp: u64,
    pub date: DateTime<Utc>,
    pub size: u64,
    pub path: PathBuf,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub info: SnapshotInfo,
    pub original_path: PathBuf,
    pub snapshot_path: PathBuf,
}

#[async_trait]
pub trait FileSnapshotService: Send + Sync {
    fn snapshot_dir(&self) -> PathBuf;

    /// Creates a new snapshot of the given file
    async fn create_snapshot(&self, file_path: &Path) -> Result<SnapshotInfo>;

    /// Lists all snapshots for a given file
    async fn list_snapshots(&self, file_path: &Path) -> Result<Vec<SnapshotInfo>>;

    /// Restores a file to a specific timestamp
    async fn restore_by_timestamp(&self, file_path: &Path, timestamp: u64) -> Result<()>;

    /// Restores a file to a specific index (0 = newest, 1 = previous version, etc.)
    async fn restore_by_index(&self, file_path: &Path, index: usize) -> Result<()>;

    /// Convenient method to restore previous version
    async fn restore_previous(&self, file_path: &Path) -> Result<()>;

    /// Gets snapshot metadata by timestamp
    async fn get_snapshot_by_timestamp(&self, file_path: &Path, timestamp: u64) -> Result<SnapshotMetadata>;

    /// Gets snapshot metadata by index
    async fn get_snapshot_by_index(&self, file_path: &Path, index: usize) -> Result<SnapshotMetadata>;

    /// Purges snapshots older than the specified number of days
    async fn purge_older_than(&self, days: u32) -> Result<usize>;

    /// Shows differences between current file and a specific snapshot
    async fn diff_with_snapshot(&self, file_path: &Path, snapshot: &SnapshotMetadata) -> Result<String>;

    /// Shows differences with previous version
    async fn diff_with_previous(&self, file_path: &Path) -> Result<String> {
        let snapshots = self.list_snapshots(file_path).await?;
        if snapshots.len() < 2 {
            return Err(SnapshotError::NotFound("No previous version found".to_string()));
        }
        let previous = self.get_snapshot_by_index(file_path, 1).await?;
        self.diff_with_snapshot(file_path, &previous).await
    }
}

// Re-export important types
pub mod prelude {
    pub use super::{FileSnapshotService, Result, SnapshotError, SnapshotInfo, SnapshotMetadata};
}

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
