use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::Result;
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};

pub mod service;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub timestamp: u64,
    pub size: u64,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub info: SnapshotInfo,
    pub hash: String,
}

#[async_trait::async_trait]
pub trait FileSnapshotService {
    fn snapshot_dir(&self) -> PathBuf;

    async fn create_snapshot(&self, file_path: &Path) -> Result<SnapshotInfo>;

    async fn list_snapshots(&self, file_path: &Path) -> Result<Vec<SnapshotInfo>>;

    async fn restore_by_timestamp(&self, file_path: &Path, timestamp: u64) -> Result<()>;

    async fn restore_by_index(&self, file_path: &Path, index: usize) -> Result<()>;

    async fn restore_previous(&self, file_path: &Path) -> Result<()> {
        self.restore_by_index(file_path, 1).await
    }

    async fn get_snapshot_by_timestamp(&self, file_path: &Path, timestamp: u64) -> Result<SnapshotMetadata>;
    
    async fn get_snapshot_by_index(&self, file_path: &Path, index: usize) -> Result<SnapshotMetadata>;
    
    async fn purge_older_than(&self, days: u32) -> Result<usize>;

    async fn generate_diff(&self, file_path: &Path, timestamp: u64) -> Result<String>;
}

pub fn hash_path(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn format_timestamp(timestamp: u64) -> String {
    let datetime = chrono::DateTime::from_timestamp(timestamp as i64, 0)
        .unwrap_or_else(|| chrono::Utc::now());
    datetime.format("%Y-%m-%d %H:%M").to_string()
} 