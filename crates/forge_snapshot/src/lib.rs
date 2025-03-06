pub mod service;

use std::path::{Path, PathBuf};
use std::time::SystemTime;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    /// Index of the snapshot (used for ordering or identification)
    pub index: usize,
    /// Timestamp when the snapshot was created
    pub timestamp: SystemTime,
    /// Original file path
    pub original_path: PathBuf,
    /// Path to the snapshot file
    pub snapshot_path: PathBuf,
    /// Size of the snapshot file in bytes
    pub file_size: u64,
}

impl SnapshotInfo {
    /// Creates a new `SnapshotInfo` instance
    pub fn new(
        index: usize,
        timestamp: SystemTime,
        original_path: PathBuf,
        snapshot_path: PathBuf,
        file_size: u64,
    ) -> Self {
        Self { 
            index, 
            timestamp, 
            original_path, 
            snapshot_path, 
            file_size,
        }
    }

    /// Returns a formatted timestamp string (ISO 8601)
    pub fn formatted_date(&self) -> String {
        let datetime: chrono::DateTime<chrono::Utc> = self.timestamp.into();
        datetime.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotMetadata {
    /// Basic info about the snapshot
    pub info: SnapshotInfo,
    /// Content of the snapshot file
    pub content: Vec<u8>,
    /// SHA-256 hash of the original file path, used for storage organization
    pub path_hash: String,
}

#[async_trait::async_trait]
pub trait FileSnapshotService<E: std::error::Error + Send + Sync> {
    fn snapshot_dir(&self) -> PathBuf;

    // Creation
    async fn create_snapshot(&self, file_path: &Path) -> Result<SnapshotInfo, E>;

    // Listing
    async fn list_snapshots(&self, file_path: &Path) -> Result<Vec<SnapshotInfo>, E>;

    // Timestamp-based restoration
    async fn restore_by_timestamp(&self, file_path: &Path, timestamp: &str) -> Result<(), E>;

    // Index-based restoration (0 = newest, 1 = previous version, etc.)
    async fn restore_by_index(&self, file_path: &Path, index: isize) -> Result<(), E>;

    // Convenient method to restore previous version
    async fn restore_previous(&self, file_path: &Path) -> Result<(), E>;

    // Metadata access
    async fn get_snapshot_by_timestamp(
        &self,
        file_path: &Path,
        timestamp: &str,
    ) -> Result<SnapshotMetadata, E>;
    async fn get_snapshot_by_index(
        &self,
        file_path: &Path,
        index: isize,
    ) -> Result<SnapshotMetadata, E>;

    // Global purge operation
    async fn purge_older_than(&self, days: u32) -> Result<usize, E>;
}



