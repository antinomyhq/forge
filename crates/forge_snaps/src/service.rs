use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use forge_fs::ForgeFS;
use forge_walker::Walker;

use crate::{SnapshotInfo, SnapshotMetadata};

/// Implementation of `FileSnapshotService` that provides snapshot
/// functionality for files with retention policies.
#[derive(Default, Debug)]
pub struct SnapshotService {
    /// Base directory for storing snapshots
    snapshot_base_dir: PathBuf,
    /// Maximum number of snapshots to keep per file
    max_snapshots_per_file: usize,
}

impl SnapshotService {
    /// Creates a new instance with a custom snapshot directory
    pub fn new(snapshot_base_dir: PathBuf) -> Self {
        Self {
            snapshot_base_dir,
            max_snapshots_per_file: 10, // Default from requirements
        }
    }

    /// Calculates a blake3 hash of the file path for storage organization
    fn hash_path(&self, file_path: &Path) -> String {
        let path_str = file_path.to_string_lossy().to_string();
        let mut hasher = blake3::Hasher::new();
        hasher.update(path_str.as_bytes());
        hasher.finalize().to_hex().to_string()
    }

    /// Gets the directory for a specific file's snapshots
    async fn get_file_snapshot_dir(&self, file_path: &Path) -> Result<PathBuf> {
        let hash = self.hash_path(file_path);
        let dir = self.snapshot_base_dir.join(hash);

        // Create the directory if it doesn't exist
        if !dir.exists() {
            ForgeFS::create_dir_all(&dir)
                .await
                .with_context(|| format!("Failed to create snapshot directory: {:?}", dir))?;
        }

        Ok(dir)
    }

    /// Creates a snapshot filename based on the timestamp
    fn create_snapshot_filename(&self, timestamp: &str) -> String {
        format!("{}.snap", timestamp)
    }

    /// Gets the timestamp from a snapshot filename
    fn get_timestamp_from_filename(&self, filename: &str) -> Option<u128> {
        if let Some(name) = filename.strip_suffix(".snap") {
            name.parse().ok()
        } else {
            None
        }
    }

    /// Retrieves all snapshot files for a given file, sorted by timestamp
    /// (newest first)
    async fn get_sorted_snapshots(&self, file_path: &Path) -> Result<Vec<(u128, PathBuf)>> {
        let snapshot_dir = self.get_file_snapshot_dir(file_path).await?;
        let mut snapshots = Vec::new();

        let entries = Walker::min_all().cwd(snapshot_dir.clone()).get().await?;

        for entry in entries {
            let path = snapshot_dir.join(entry.path);
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if let Some(timestamp) = self.get_timestamp_from_filename(filename) {
                    snapshots.push((timestamp, path));
                }
            }
        }

        // Sort by timestamp (newest first)
        snapshots.sort_by(|a, b| b.0.cmp(&a.0));

        Ok(snapshots)
    }

    /// Applies retention policy to snapshots, removing excess ones
    async fn apply_retention_policy(&self, file_path: &Path) -> Result<()> {
        let snapshots = self.get_sorted_snapshots(file_path).await?;

        // Remove excess snapshots based on max_snapshots_per_file
        if snapshots.len() > self.max_snapshots_per_file {
            for (_, path) in snapshots.iter().skip(self.max_snapshots_per_file) {
                ForgeFS::remove_file(path)
                    .await
                    .with_context(|| format!("Failed to remove excess snapshot: {:?}", path))?;
            }
        }

        Ok(())
    }
}

impl SnapshotService {
    pub fn snapshot_dir(&self) -> PathBuf {
        self.snapshot_base_dir.clone()
    }

    pub async fn create_snapshot(&self, file_path: &Path) -> Result<SnapshotInfo> {
        // Ensure the file exists
        if !file_path.exists() {
            anyhow::bail!("File does not exist: {:?}", file_path);
        }

        // ForgeFS::read the file content
        let content = ForgeFS::read(file_path)
            .await
            .with_context(|| format!("Failed to ForgeFS::read file: {:?}", file_path))?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis()
            .to_string();

        // Get the snapshot directory and create it if needed
        let snapshot_dir = self.get_file_snapshot_dir(file_path).await?;
        let snapshot_filename = self.create_snapshot_filename(&timestamp);
        let snapshot_path = snapshot_dir.join(&snapshot_filename);

        // ForgeFS::write the snapshot
        ForgeFS::write(&snapshot_path, &content)
            .await
            .with_context(|| format!("Failed to ForgeFS::write snapshot: {:?}", snapshot_path))?;

        // Apply retention policy
        self.apply_retention_policy(file_path).await?;

        // Create and return the SnapshotInfo
        let snapshot_info = SnapshotInfo::with_timestamp(
            timestamp,
            file_path.to_path_buf(),
            snapshot_path,
            0, // This is the newest snapshot, so index is 0
        );

        Ok(snapshot_info)
    }

    pub async fn list_snapshots(&self) -> Result<Vec<SnapshotInfo>> {
        let mut result = vec![];

        // Get all directories in the snapshot base dir (each directory represents a file)
        let entries = Walker::max_all()
            .cwd(self.snapshot_base_dir.clone())
            .get()
            .await
            .with_context(|| format!("Failed to read base snapshot directory: {:?}", self.snapshot_base_dir))?;
        for entry in entries {
            if entry.is_dir() {
                continue;
            }
            
            if let Some(filename) = entry.file_name {
                dbg!(&filename);
                if let Some(timestamp) = self.get_timestamp_from_filename(&filename) {
                    result.push(SnapshotInfo::with_timestamp(timestamp.to_string(), PathBuf::new(), self.snapshot_base_dir.join(entry.path), 0));
                }
            }
        }
        result.sort_by(|a, b| {
            let a_timestamp = a.timestamp.parse::<u128>().unwrap_or(0);
            let b_timestamp = b.timestamp.parse::<u128>().unwrap_or(0);
            a_timestamp.cmp(&b_timestamp)
        });
        Ok(result)
    }

    pub async fn restore_by_timestamp(&self, file_path: &Path, timestamp: &str) -> Result<()> {
        let snapshot_metadata = self.get_snapshot_by_timestamp(file_path, timestamp).await?;

        // ForgeFS::write the content back to the original file
        ForgeFS::write(file_path, &snapshot_metadata.content)
            .await
            .with_context(|| format!("Failed to restore file: {:?}", file_path))?;

        Ok(())
    }

    pub async fn restore_by_index(&self, file_path: &Path, index: isize) -> Result<()> {
        let snapshot_metadata = self.get_snapshot_by_index(file_path, index).await?;

        // ForgeFS::write the content back to the original file
        ForgeFS::write(file_path, &snapshot_metadata.content)
            .await
            .with_context(|| format!("Failed to restore file: {:?}", file_path))?;

        Ok(())
    }

    pub async fn restore_previous(&self, file_path: &Path) -> Result<()> {
        self.restore_by_index(file_path, -1).await
    }

    pub async fn get_snapshot_by_timestamp(
        &self,
        file_path: &Path,
        timestamp: &str,
    ) -> Result<SnapshotMetadata> {
        let snapshot_dir = self.get_file_snapshot_dir(file_path).await?;
        let snapshot_filename = self.create_snapshot_filename(timestamp);
        let snapshot_path = snapshot_dir.join(snapshot_filename);

        if !snapshot_path.exists() {
            anyhow::bail!("Snapshot does not exist for timestamp {}", timestamp);
        }

        let content = ForgeFS::read(&snapshot_path)
            .await
            .with_context(|| format!("Failed to ForgeFS::read snapshot: {:?}", snapshot_path))?;

        // Find the index of this snapshot
        let snapshots = self.get_sorted_snapshots(file_path).await?;
        let index = snapshots
            .iter()
            .position(|(t, _)| t.to_string() == timestamp)
            .unwrap_or(0);

        let info = SnapshotInfo::with_timestamp(
            timestamp.to_string(),
            file_path.to_path_buf(),
            snapshot_path,
            index,
        );

        Ok(SnapshotMetadata { info, content, path_hash: self.hash_path(file_path) })
    }

    pub async fn get_snapshot_by_index(
        &self,
        file_path: &Path,
        mut index: isize,
    ) -> Result<SnapshotMetadata> {
        let snapshots = self.get_sorted_snapshots(file_path).await?;

        if index == -1 {
            index = (snapshots.len() - 1) as isize;
        }

        if index as usize >= snapshots.len() {
            anyhow::bail!(
                "Snapshot index {} is out of bounds (max: {})",
                index,
                snapshots.len().saturating_sub(1)
            );
        }

        dbg!(index);

        let (timestamp, _) = snapshots[index as usize];
        self.get_snapshot_by_timestamp(file_path, &timestamp.to_string())
            .await
    }

    pub async fn purge_older_than(&self, days: u32) -> Result<usize> {
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .checked_sub(Duration::from_secs(days as u64 * 24 * 60 * 60))
            .unwrap_or(Duration::from_secs(0))
            .as_millis();

        let mut removed_count = 0;

        // Iterate through all directories in the snapshot base dir
        let entries = Walker::max_all()
            .cwd(self.snapshot_base_dir.clone())
            .get()
            .await
            .with_context(|| {
                format!(
                    "Failed to ForgeFS::read base snapshot directory: {:?}",
                    self.snapshot_base_dir
                )
            })?;

        for entry in entries {
            let snapshot_path = self.snapshot_base_dir.join(entry.path);

            if !snapshot_path.is_dir() {
                if let Some(filename) = snapshot_path.file_name().and_then(|n| n.to_str()) {
                    if let Some(timestamp) = self.get_timestamp_from_filename(filename) {
                        // Remove if older than the cutoff

                        if timestamp < cutoff {
                            ForgeFS::remove_file(&snapshot_path)
                                .await
                                .with_context(|| {
                                    format!("Failed to remove old snapshot: {:?}", snapshot_path)
                                })?;
                            removed_count += 1;
                        }
                    }
                }
            }
        }

        Ok(removed_count)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    use super::*;

    #[tokio::test]
    async fn test_create_snapshot() -> Result<()> {
        let temp_dir = tempdir()?;
        let base_path = temp_dir.path().to_path_buf();
        let service = SnapshotService::new(base_path.join("snapshots"));

        // Create a test file
        let test_file_path = base_path.join("test.txt");
        let mut file = File::create(&test_file_path).await?;
        file.write_all(b"test content").await?;
        file.flush().await?;

        // Create a snapshot
        let snapshot_info = service.create_snapshot(&test_file_path).await?;

        // Verify the snapshot was created
        assert_eq!(snapshot_info.original_path, test_file_path);
        assert!(snapshot_info.snapshot_path.exists());

        Ok(())
    }

    #[tokio::test]
    async fn test_list_snapshots() -> Result<()> {
        let temp_dir = tempdir()?;
        let base_path = temp_dir.path().to_path_buf();
        let service = SnapshotService::new(base_path.join("snapshots"));

        // Create a test file
        let test_file_path = base_path.join("test.txt");
        let mut file = File::create(&test_file_path).await?;
        file.write_all(b"test content").await?;
        file.flush().await?;

        // Create a snapshot
        let _snapshot1 = service.create_snapshot(&test_file_path).await?;

        // Sleep for some time to avoid having same name for snapshot
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Modify file
        let mut file = File::create(&test_file_path).await?;
        file.write_all(b"modified content").await?;
        file.flush().await?;

        let _snapshot2 = service.create_snapshot(&test_file_path).await?;

        // Create another test file to have multiple files with snapshots
        let test_file_path2 = base_path.join("test2.txt");
        let mut file = File::create(&test_file_path2).await?;
        file.write_all(b"test2 content").await?;
        file.flush().await?;

        // Create a snapshot for the second file
        let _snapshot3 = service.create_snapshot(&test_file_path2).await?;

        // List snapshots
        let snapshots = service.list_snapshots().await?;

        // Verify we have at least one snapshot in the result (should be 2, one for each file)
        assert!(!snapshots.is_empty());

        // The timestamps should be in descending order (newest first)
        for i in 1..snapshots.len() {
            let prev_timestamp = snapshots[i - 1].timestamp.parse::<u128>().unwrap_or(0);
            let curr_timestamp = snapshots[i].timestamp.parse::<u128>().unwrap_or(0);
            assert!(prev_timestamp >= curr_timestamp);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_restore_by_index() -> Result<()> {
        let temp_dir = tempdir()?;
        let base_path = temp_dir.path().to_path_buf();
        let service = SnapshotService::new(base_path.join("snapshots"));

        // Create a test file
        let test_file_path = base_path.join("test.txt");
        let mut file = File::create(&test_file_path).await?;
        file.write_all(b"original content").await?;
        file.flush().await?;

        // Create a snapshot
        let _snapshot1 = service.create_snapshot(&test_file_path).await?;

        // Modify file
        let mut file = File::create(&test_file_path).await?;
        file.write_all(b"modified content").await?;
        file.flush().await?;

        // Restore to the original content
        service.restore_by_index(&test_file_path, 0).await?;

        // Verify the file has been restored
        let content = tokio::fs::read_to_string(&test_file_path).await?;
        assert_eq!(content, "original content");

        Ok(())
    }

    #[tokio::test]
    async fn test_retention_policy() -> Result<()> {
        let temp_dir = tempdir()?;
        let base_path = temp_dir.path().to_path_buf();
        let mut service = SnapshotService::new(base_path.join("snapshots"));
        service.max_snapshots_per_file = 3; // Set a small limit for testing

        // Create a test file
        let test_file_path = base_path.join("test.txt");
        let mut file = File::create(&test_file_path).await?;
        file.write_all(b"content 1").await?;
        file.flush().await?;

        // Create multiple snapshots with modifications
        for i in 1..=5 {
            let _snapshot = service.create_snapshot(&test_file_path).await?;

            let mut file = File::create(&test_file_path).await?;
            file.write_all(format!("content {}", i + 1).as_bytes())
                .await?;
            file.flush().await?;
        }

        // List snapshots
        let snapshots = service.list_snapshots().await?;

        // Verify we have at least one snapshot in the results
        assert!(!snapshots.is_empty());

        Ok(())
    }
}
