use std::path::{Path, PathBuf};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sha2::{Sha256, Digest};
use colored::*;
use tokio::fs as async_fs;
use forge_api::{ForgeAPI, API};

use crate::{FileSnapshotService, Result, SnapshotError, SnapshotInfo, SnapshotMetadata};

pub struct FileSnapshotServiceImpl {
    snapshot_dir: PathBuf,
    max_snapshots: usize,
    retention_days: u32,
    sequence: std::sync::atomic::AtomicU64,
}

impl FileSnapshotServiceImpl {
    pub fn new(snapshot_dir: PathBuf) -> Self {
        Self {
            snapshot_dir,
            max_snapshots: 10,
            retention_days: 30,
            sequence: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn from_env() -> Self {
        let snapshot_dir = std::env::var("FORGE_SNAPSHOT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let env = ForgeAPI::init(false).environment();
                env.base_path.join("snapshots")
            });
        
        Self::new(snapshot_dir)
    }

    pub fn with_config(snapshot_dir: PathBuf, max_snapshots: usize, retention_days: u32) -> Self {
        Self {
            snapshot_dir,
            max_snapshots,
            retention_days,
            sequence: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn hash_path(&self, path: &Path) -> String {
        let mut hasher = Sha256::new();
        hasher.update(path.to_string_lossy().as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn get_snapshot_path(&self, file_path: &Path, timestamp: u64) -> PathBuf {
        let hash = self.hash_path(file_path);
        let timestamp_secs = timestamp / 1000;
        self.snapshot_dir
            .join(hash)
            .join(format!("{}.snap", timestamp_secs))
    }

    async fn ensure_snapshot_dir(&self, file_path: &Path) -> Result<PathBuf> {
        let hash = self.hash_path(file_path);
        let dir = self.snapshot_dir.join(hash);
        async_fs::create_dir_all(&dir).await?;
        Ok(dir)
    }

    async fn read_snapshot_content(&self, snapshot_path: &Path) -> Result<String> {
        Ok(async_fs::read_to_string(snapshot_path).await?)
    }
}

#[async_trait]
impl FileSnapshotService for FileSnapshotServiceImpl {
    fn snapshot_dir(&self) -> PathBuf {
        self.snapshot_dir.clone()
    }

    async fn create_snapshot(&self, file_path: &Path) -> Result<SnapshotInfo> {
        let content = async_fs::read_to_string(file_path).await?;
        let now = SystemTime::now();
        let base_timestamp = now.duration_since(UNIX_EPOCH)?
            .as_secs() as u64;
        
        // Add sequence number to ensure uniqueness within the same second
        let seq = self.sequence.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let timestamp = base_timestamp * 1000 + (seq % 1000);  // Store with millisecond precision internally
        
        // Ensure the snapshot directory exists
        self.ensure_snapshot_dir(file_path).await?;
        let snapshot_path = self.get_snapshot_path(file_path, timestamp);
        
        // Create parent directories if they don't exist
        if let Some(parent) = snapshot_path.parent() {
            async_fs::create_dir_all(parent).await?;
        }
        
        // Write the snapshot content
        async_fs::write(&snapshot_path, &content).await?;

        let metadata = async_fs::metadata(&snapshot_path).await?;
        let date = DateTime::<Utc>::from(now);
        let hash = self.hash_path(file_path);

        Ok(SnapshotInfo {
            timestamp,
            date,
            size: metadata.len(),
            path: file_path.to_path_buf(),
            hash,
        })
    }

    async fn list_snapshots(&self, file_path: &Path) -> Result<Vec<SnapshotInfo>> {
        let hash = self.hash_path(file_path);
        let dir = self.snapshot_dir.join(&hash);
        
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut snapshots = Vec::new();
        let mut entries = fs::read_dir(&dir)?;

        while let Some(entry) = entries.next() {
            let entry = entry?;
            let path = entry.path();
            
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if let Some(timestamp_str) = name.strip_suffix(".snap") {
                    if let Ok(timestamp) = timestamp_str.parse::<u64>() {
                        let metadata = async_fs::metadata(&path).await?;
                        // Convert timestamp to milliseconds if it's in seconds
                        let timestamp_ms = if timestamp < 32503680000 { // 3000-01-01 in seconds
                            timestamp * 1000
                        } else {
                            timestamp
                        };
                        let date = DateTime::<Utc>::from(
                            UNIX_EPOCH + std::time::Duration::from_millis(timestamp_ms)
                        );

                        snapshots.push(SnapshotInfo {
                            timestamp: timestamp_ms,
                            date,
                            size: metadata.len(),
                            path: file_path.to_path_buf(),
                            hash: hash.clone(),
                        });
                    }
                }
            }
        }

        snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(snapshots)
    }

    async fn restore_by_timestamp(&self, file_path: &Path, timestamp: u64) -> Result<()> {
        // Convert timestamp to seconds for file lookup
        let timestamp_secs = timestamp / 1000;
        let snapshot_path = self.get_snapshot_path(file_path, timestamp);
        
        if !snapshot_path.exists() {
            return Err(SnapshotError::NotFound(format!(
                "No snapshot found for timestamp {}",
                timestamp_secs
            )));
        }

        let content = self.read_snapshot_content(&snapshot_path).await?;
        async_fs::write(file_path, content).await?;
        Ok(())
    }

    async fn restore_by_index(&self, file_path: &Path, index: usize) -> Result<()> {
        let snapshots = self.list_snapshots(file_path).await?;
        let snapshot = snapshots.get(index).ok_or_else(|| {
            SnapshotError::NotFound(format!("No snapshot found at index {}", index))
        })?;

        self.restore_by_timestamp(file_path, snapshot.timestamp).await
    }

    async fn get_snapshot_by_timestamp(&self, file_path: &Path, timestamp: u64) -> Result<SnapshotMetadata> {
        let snapshot_path = self.get_snapshot_path(file_path, timestamp);
        if !snapshot_path.exists() {
            return Err(SnapshotError::NotFound(format!(
                "No snapshot found for timestamp {}",
                timestamp
            )));
        }

        let metadata = async_fs::metadata(&snapshot_path).await?;
        let date = DateTime::<Utc>::from(
            UNIX_EPOCH + std::time::Duration::from_millis(timestamp)
        );

        Ok(SnapshotMetadata {
            info: SnapshotInfo {
                timestamp,
                date,
                size: metadata.len(),
                path: file_path.to_path_buf(),
                hash: self.hash_path(file_path),
            },
            original_path: file_path.to_path_buf(),
            snapshot_path,
        })
    }

    async fn get_snapshot_by_index(&self, file_path: &Path, index: usize) -> Result<SnapshotMetadata> {
        let snapshots = self.list_snapshots(file_path).await?;
        let snapshot = snapshots.get(index).ok_or_else(|| {
            SnapshotError::NotFound(format!("No snapshot found at index {}", index))
        })?;

        self.get_snapshot_by_timestamp(file_path, snapshot.timestamp).await
    }

    async fn purge_older_than(&self, days: u32) -> Result<usize> {
        let mut count = 0;
        let cutoff = SystemTime::now() - std::time::Duration::from_secs(days as u64 * 24 * 60 * 60);

        for entry in fs::read_dir(&self.snapshot_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                for snapshot in fs::read_dir(path)? {
                    let snapshot = snapshot?;
                    let metadata = snapshot.metadata()?;
                    
                    if metadata.modified()? < cutoff {
                        fs::remove_file(snapshot.path())?;
                        count += 1;
                    }
                }
            }
        }

        Ok(count)
    }

    async fn diff_with_snapshot(&self, file_path: &Path, snapshot: &SnapshotMetadata) -> Result<String> {
        let current = async_fs::read_to_string(file_path).await?;
        let previous = self.read_snapshot_content(&snapshot.snapshot_path).await?;

        let mut config = similar::TextDiffConfig::default();
        let config = config.algorithm(similar::Algorithm::Patience);
        let diff_config = config.timeout(std::time::Duration::from_secs(1));
            
        let diff = diff_config.diff_lines(&previous, &current);
        let mut result = String::new();

        result.push_str(&format!("--- {} ({})\n", file_path.display(), snapshot.info.date));
        result.push_str(&format!("+++ {} (current)\n", file_path.display()));

        for group in diff.grouped_ops(3) {
            let line_old = group.first().unwrap().old_range().start;
            let line_new = group.first().unwrap().new_range().start;

            // Print group header
            result.push_str(&format!("@@ -{},{} +{},{} @@\n",
                line_old + 1,
                group.iter().map(|op| op.old_range().len()).sum::<usize>(),
                line_new + 1,
                group.iter().map(|op| op.new_range().len()).sum::<usize>()
            ));

            for op in group {
                for old_index in op.old_range() {
                    if let Some(line) = previous.lines().nth(old_index) {
                        result.push_str(&format!("-{}\n", line.trim_end().red()));
                    }
                }
                for new_index in op.new_range() {
                    if let Some(line) = current.lines().nth(new_index) {
                        result.push_str(&format!("+{}\n", line.trim_end().green()));
                    }
                }
            }
        }

        Ok(result)
    }

    async fn restore_previous(&self, file_path: &Path) -> Result<()> {
        self.restore_by_index(file_path, 1).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_snapshot_creation_and_listing() -> anyhow::Result<()> {
        let temp = assert_fs::TempDir::new()?;
        let snapshot_dir = temp.child("snapshots");
        let service = FileSnapshotServiceImpl::new(snapshot_dir.path().to_path_buf());

        let test_file = temp.child("test.txt");
        test_file.write_str("initial content")?;

        // Create first snapshot
        let snapshot1 = service.create_snapshot(test_file.path()).await?;
        println!("Created first snapshot with timestamp: {}", snapshot1.timestamp);
        
        // Modify file and wait to ensure different timestamp
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        test_file.write_str("modified content")?;
        
        // Create second snapshot
        let snapshot2 = service.create_snapshot(test_file.path()).await?;
        println!("Created second snapshot with timestamp: {}", snapshot2.timestamp);

        // List snapshots
        let snapshots = service.list_snapshots(test_file.path()).await?;
        println!("Found {} snapshots", snapshots.len());
        for (i, snap) in snapshots.iter().enumerate() {
            println!("Snapshot {}: timestamp {}", i, snap.timestamp);
        }

        assert_eq!(snapshots.len(), 2, "Expected 2 snapshots");
        assert!(snapshots[0].timestamp > snapshots[1].timestamp, "Latest snapshot should have higher timestamp");
        assert_eq!(snapshots[0].timestamp / 1000, snapshot2.timestamp / 1000, "Latest snapshot timestamp mismatch");
        assert_eq!(snapshots[1].timestamp / 1000, snapshot1.timestamp / 1000, "First snapshot timestamp mismatch");

        Ok(())
    }

    #[tokio::test]
    async fn test_restore_by_index() -> anyhow::Result<()> {
        let temp = assert_fs::TempDir::new()?;
        let snapshot_dir = temp.child("snapshots");
        let service = FileSnapshotServiceImpl::new(snapshot_dir.path().to_path_buf());

        let test_file = temp.child("test.txt");
        test_file.write_str("version 1")?;
        let snap1 = service.create_snapshot(test_file.path()).await?;
        println!("Created snapshot 1 with timestamp: {}", snap1.timestamp);

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        test_file.write_str("version 2")?;
        let snap2 = service.create_snapshot(test_file.path()).await?;
        println!("Created snapshot 2 with timestamp: {}", snap2.timestamp);

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        test_file.write_str("version 3")?;
        let snap3 = service.create_snapshot(test_file.path()).await?;
        println!("Created snapshot 3 with timestamp: {}", snap3.timestamp);

        // List snapshots before restore
        let snapshots = service.list_snapshots(test_file.path()).await?;
        println!("Found {} snapshots before restore", snapshots.len());
        for (i, snap) in snapshots.iter().enumerate() {
            println!("Snapshot {}: timestamp {}", i, snap.timestamp);
        }

        // Restore to version 2
        service.restore_by_index(test_file.path(), 1).await?;
        let content = async_fs::read_to_string(test_file.path()).await?;
        println!("Restored content: {}", content);
        assert_eq!(content, "version 2", "Restored content should match version 2");

        Ok(())
    }

    #[tokio::test]
    async fn test_purge_old_snapshots() -> anyhow::Result<()> {
        let temp = assert_fs::TempDir::new()?;
        let snapshot_dir = temp.child("snapshots");
        let service = FileSnapshotServiceImpl::new(snapshot_dir.path().to_path_buf());

        let test_file = temp.child("test.txt");
        test_file.write_str("old content")?;
        
        // Create old snapshot
        let snapshot_path = service.get_snapshot_path(test_file.path(), 1000000);
        std::fs::create_dir_all(snapshot_path.parent().unwrap())?;
        std::fs::write(&snapshot_path, "old content")?;
        
        // Set old modification time
        let old_time = std::time::SystemTime::now() - Duration::from_secs(31 * 24 * 60 * 60);
        filetime::set_file_mtime(&snapshot_path, filetime::FileTime::from_system_time(old_time))?;

        // Create new snapshot
        test_file.write_str("new content")?;
        service.create_snapshot(test_file.path()).await?;

        // Purge old snapshots
        let purged = service.purge_older_than(30).await?;
        assert_eq!(purged, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_diff_with_snapshot() -> anyhow::Result<()> {
        let temp = assert_fs::TempDir::new()?;
        let snapshot_dir = temp.child("snapshots");
        let service = FileSnapshotServiceImpl::new(snapshot_dir.path().to_path_buf());

        let test_file = temp.child("test.txt");
        test_file.write_str("line 1\nline 2\n")?;
        let snapshot = service.create_snapshot(test_file.path()).await?;
        println!("Created snapshot with content:\n{}", async_fs::read_to_string(test_file.path()).await?);

        // Get the metadata before modifying the file
        let metadata = service.get_snapshot_by_timestamp(test_file.path(), snapshot.timestamp).await?;

        // Now modify the file
        test_file.write_str("line 1\nmodified line 2\n")?;
        println!("Modified content:\n{}", async_fs::read_to_string(test_file.path()).await?);

        let diff = service.diff_with_snapshot(test_file.path(), &metadata).await?;
        println!("Diff output:\n{}", diff);
        
        // Strip ANSI color codes for comparison
        let clean_diff = strip_ansi_codes(&diff);
        assert!(clean_diff.contains("-line 2"));
        assert!(clean_diff.contains("+modified line 2"));

        Ok(())
    }

    fn strip_ansi_codes(s: &str) -> String {
        let re = regex::Regex::new(r"\x1B\[[0-9;]*[mK]").unwrap();
        re.replace_all(s, "").to_string()
    }
} 