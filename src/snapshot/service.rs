use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::Result;
use tokio::fs as tokio_fs;
use sha2::{Sha256, Digest};
use similar::{ChangeTag, TextDiff};
use crate::snapshot::{FileSnapshotService, SnapshotInfo, SnapshotMetadata};
use crate::snapshot::{hash_path, get_current_timestamp, format_timestamp};

#[derive(Clone)]
pub struct DefaultSnapshotService {
    base_dir: PathBuf,
    max_snapshots: usize,
    retention_days: u32,
}

impl DefaultSnapshotService {
    pub fn new(base_dir: PathBuf, max_snapshots: usize, retention_days: u32) -> Self {
        Self {
            base_dir,
            max_snapshots,
            retention_days,
        }
    }

    fn get_snapshot_path(&self, file_path: &Path, timestamp: u64) -> PathBuf {
        let hash = hash_path(file_path);
        self.base_dir
            .join(hash)
            .join(format!("{}.snap", timestamp))
    }

    async fn ensure_snapshot_dir(&self, file_path: &Path) -> Result<PathBuf> {
        let hash = hash_path(file_path);
        let dir = self.base_dir.join(hash);
        tokio_fs::create_dir_all(&dir).await?;
        Ok(dir)
    }

    async fn cleanup_old_snapshots(&self, file_path: &Path) -> Result<()> {
        let snapshots = self.list_snapshots(file_path).await?;
        if snapshots.len() > self.max_snapshots {
            let oldest = snapshots.last().unwrap();
            let snapshot_path = self.get_snapshot_path(file_path, oldest.timestamp);
            tokio_fs::remove_file(snapshot_path).await?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl FileSnapshotService for DefaultSnapshotService {
    fn snapshot_dir(&self) -> PathBuf {
        self.base_dir.clone()
    }

    async fn create_snapshot(&self, file_path: &Path) -> Result<SnapshotInfo> {
        let timestamp = get_current_timestamp();
        let snapshot_path = self.get_snapshot_path(file_path, timestamp);
        
        self.ensure_snapshot_dir(file_path).await?;
        tokio_fs::copy(file_path, &snapshot_path).await?;
        let metadata = tokio_fs::metadata(&snapshot_path).await?;
        self.cleanup_old_snapshots(file_path).await?;

        Ok(SnapshotInfo {
            timestamp,
            size: metadata.len(),
            path: file_path.to_path_buf(),
        })
    }

    async fn list_snapshots(&self, file_path: &Path) -> Result<Vec<SnapshotInfo>> {
        let hash = hash_path(file_path);
        let dir = self.base_dir.join(hash);
        
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut snapshots = Vec::new();
        let mut entries = tokio_fs::read_dir(dir).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            if let Some(file_name) = entry.file_name().to_str() {
                if let Some(timestamp) = file_name.strip_suffix(".snap").and_then(|s| s.parse::<u64>().ok()) {
                    let metadata = entry.metadata().await?;
                    snapshots.push(SnapshotInfo {
                        timestamp,
                        size: metadata.len(),
                        path: file_path.to_path_buf(),
                    });
                }
            }
        }

        snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(snapshots)
    }

    async fn restore_by_timestamp(&self, file_path: &Path, timestamp: u64) -> Result<()> {
        let snapshot_path = self.get_snapshot_path(file_path, timestamp);
        if !snapshot_path.exists() {
            anyhow::bail!("Snapshot not found for timestamp {}", timestamp);
        }

        tokio_fs::copy(&snapshot_path, file_path).await?;
        Ok(())
    }

    async fn restore_by_index(&self, file_path: &Path, index: usize) -> Result<()> {
        let snapshots = self.list_snapshots(file_path).await?;
        if snapshots.is_empty() {
            anyhow::bail!("No snapshots found for file: {}", file_path.display());
        }
        if index >= snapshots.len() {
            anyhow::bail!("Snapshot index {} out of range. Available snapshots: {}", index, snapshots.len());
        }

        let snapshot = &snapshots[index];
        self.restore_by_timestamp(file_path, snapshot.timestamp).await
    }

    async fn get_snapshot_by_timestamp(&self, file_path: &Path, timestamp: u64) -> Result<SnapshotMetadata> {
        let snapshot_path = self.get_snapshot_path(file_path, timestamp);
        if !snapshot_path.exists() {
            anyhow::bail!("Snapshot not found for timestamp {}", timestamp);
        }

        let metadata = tokio_fs::metadata(&snapshot_path).await?;
        let mut file = File::open(&snapshot_path)?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        
        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let hash = format!("{:x}", hasher.finalize());

        Ok(SnapshotMetadata {
            info: SnapshotInfo {
                timestamp,
                size: metadata.len(),
                path: file_path.to_path_buf(),
            },
            hash,
        })
    }

    async fn get_snapshot_by_index(&self, file_path: &Path, index: usize) -> Result<SnapshotMetadata> {
        let snapshots = self.list_snapshots(file_path).await?;
        if snapshots.is_empty() {
            anyhow::bail!("No snapshots found for file: {}", file_path.display());
        }
        if index >= snapshots.len() {
            anyhow::bail!("Snapshot index {} out of range. Available snapshots: {}", index, snapshots.len());
        }

        let snapshot = &snapshots[index];
        self.get_snapshot_by_timestamp(file_path, snapshot.timestamp).await
    }

    async fn purge_older_than(&self, days: u32) -> Result<usize> {
        // Use the default retention period if no specific days provided
        let days_to_keep = if days == 0 { self.retention_days } else { days };
        
        let mut purged_count = 0;
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs()
            .saturating_sub(days_to_keep as u64 * 24 * 60 * 60);

        let mut entries = tokio_fs::read_dir(&self.base_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let mut snapshots = tokio_fs::read_dir(entry.path()).await?;
                while let Some(snapshot) = snapshots.next_entry().await? {
                    if let Some(file_name) = snapshot.file_name().to_str() {
                        if let Some(timestamp) = file_name.strip_suffix(".snap").and_then(|s| s.parse::<u64>().ok()) {
                            if timestamp < cutoff {
                                tokio_fs::remove_file(snapshot.path()).await?;
                                purged_count += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok(purged_count)
    }

    async fn generate_diff(&self, file_path: &Path, timestamp: u64) -> Result<String> {
        let current_content = tokio_fs::read_to_string(file_path).await?;
        let snapshot_path = self.get_snapshot_path(file_path, timestamp);
        
        if !snapshot_path.exists() {
            anyhow::bail!("Snapshot not found for timestamp {}", timestamp);
        }

        let snapshot_content = tokio_fs::read_to_string(&snapshot_path).await?;
        let diff = TextDiff::from_lines(&snapshot_content, &current_content);

        let mut diff_output = String::new();
        diff_output.push_str(&format!("--- {} ({})\n", file_path.display(), format_timestamp(timestamp)));
        diff_output.push_str(&format!("+++ {} (current)\n", file_path.display()));

        for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
            if idx > 0 {
                diff_output.push_str("...\n");
            }

            for op in group {
                for change in diff.iter_inline_changes(op) {
                    let sign = match change.tag() {
                        ChangeTag::Delete => "-",
                        ChangeTag::Insert => "+",
                        ChangeTag::Equal => " ",
                    };
                    let value = change.to_string();
                    diff_output.push_str(&format!("{}{}", sign, value));
                }
            }
        }

        Ok(diff_output)
    }
} 