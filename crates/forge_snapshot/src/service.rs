use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use chrono::{Duration, Utc, TimeZone};
use tokio::fs as async_fs;
use async_trait::async_trait;
use anyhow::{Result, Context};

const MAX_SNAPSHOTS: usize = 10;

#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub timestamp: u64,
    pub path: PathBuf,
    pub size: u64,
}

#[async_trait]
pub trait FileSnapshotService {
    fn snapshot_dir(&self) -> PathBuf;
    async fn create_snapshot(&self, file_path: &Path) -> Result<SnapshotInfo>;
    async fn list_snapshots(&self, file_path: &Path) -> Result<Vec<SnapshotInfo>>;
    async fn restore_by_timestamp(&self, file_path: &Path, timestamp: u64) -> Result<()>;
    async fn restore_by_index(&self, file_path: &Path, index: usize) -> Result<()>;
    async fn get_snapshot_by_timestamp(&self, file_path: &Path, timestamp: u64) -> Result<SnapshotInfo>;
    async fn get_snapshot_by_index(&self, file_path: &Path, index: usize) -> Result<SnapshotInfo>;
    async fn purge_older_than(&self, days: u32) -> Result<usize>;
}

pub struct SnapshotService {
    base_dir: PathBuf,
}

impl SnapshotService {
    pub fn new(snapshot_dir: PathBuf) -> Self {
        Self { base_dir: snapshot_dir }
    }

    fn hash_path(file_path: &Path) -> String {
        let mut hasher = Sha256::new();
        hasher.update(file_path.to_string_lossy().as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[async_trait]
impl FileSnapshotService for SnapshotService {
    fn snapshot_dir(&self) -> PathBuf {
        self.base_dir.clone()
    }

    async fn create_snapshot(&self, file_path: &Path) -> Result<SnapshotInfo> {
        let mut snapshots = self.list_snapshots(file_path).await?;
        
        if snapshots.len() >= MAX_SNAPSHOTS {
            if let Some(oldest_snapshot) = snapshots.pop() {
                async_fs::remove_file(&oldest_snapshot.path).await.context("Failed to remove oldest snapshot")?;
            }
        }
        
        let file_hash = Self::hash_path(file_path);
        let timestamp = Utc::now().timestamp() as u64;
        let snapshot_path = self.base_dir.join(&file_hash).join(format!("{}.snap", timestamp));

        if let Some(parent) = snapshot_path.parent() {
            async_fs::create_dir_all(parent).await.context("Failed to create snapshot directory")?;
        }

        async_fs::copy(file_path, &snapshot_path).await.context("Failed to copy file for snapshot")?;
        let metadata = async_fs::metadata(&snapshot_path).await?;

        Ok(SnapshotInfo { timestamp, path: snapshot_path, size: metadata.len() })
    }

    async fn list_snapshots(&self, file_path: &Path) -> Result<Vec<SnapshotInfo>> {
        let file_hash = Self::hash_path(file_path);
        let snapshot_dir = self.base_dir.join(file_hash);
        let mut snapshots = vec![];

        let mut entries = async_fs::read_dir(&snapshot_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Ok(timestamp) = stem.parse::<u64>() {
                    let metadata = async_fs::metadata(&path).await?;
                    snapshots.push(SnapshotInfo { timestamp, path, size: metadata.len() });
                }
            }
        }
        
        snapshots.sort_by_key(|s| s.timestamp);
        Ok(snapshots)
    }
    async fn restore_by_timestamp(&self, file_path: &Path, timestamp: u64) -> Result<()> {
        let snapshots = self.list_snapshots(file_path).await?;
        if let Some(snapshot) = snapshots.iter().find(|s| s.timestamp == timestamp) {
            async_fs::copy(&snapshot.path, file_path)
                .await
                .context("Failed to restore snapshot by timestamp")?;
        }
        Ok(())
    }
    async fn restore_by_index(&self, file_path: &Path, index: usize) -> Result<()> {
        let snapshots = self.list_snapshots(file_path).await?;
        if let Some(snapshot) = snapshots.get(index) {  // Use `.get(index)` to avoid out-of-bounds
            async_fs::copy(&snapshot.path, file_path)
                .await
                .context("Failed to restore snapshot by index")?;
        }
        Ok(())
    }    

    async fn get_snapshot_by_timestamp(&self, file_path: &Path, timestamp: u64) -> Result<SnapshotInfo> {
        let snapshots = self.list_snapshots(file_path).await?;
        snapshots.into_iter().find(|s| s.timestamp == timestamp).context("Snapshot not found")
    }
    async fn get_snapshot_by_index(&self, file_path: &Path, index: usize) -> Result<SnapshotInfo> {
        let snapshots = self.list_snapshots(file_path).await?;
        snapshots.into_iter().nth(index).context("Snapshot index out of bounds")
    }

    async fn purge_older_than(&self, days: u32) -> Result<usize> {
        let duration = Duration::try_days(days as i64).expect("Invalid duration");
        let cutoff = Utc::now() - duration;
        let mut deleted = 0;

        let mut entries = async_fs::read_dir(&self.base_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Ok(timestamp) = stem.parse::<i64>() {
                    if Utc.timestamp_opt(timestamp, 0).single().map_or(false, |t| t < cutoff) {
                        async_fs::remove_file(&path).await.context("Failed to remove old snapshot")?;
                        deleted += 1;
                    }
                }
            }
        }
        Ok(deleted)
    }
}
