use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose;
use base64::Engine;

use crate::snapshot::{Snapshot, SnapshotId};

/// Implementation of the SnapshotService
#[derive(Debug)]
pub struct SnapshotService {
    /// Current Working Directory,
    cwd: PathBuf,
    /// Base directory for storing snapshots
    snapshot_base_dir: PathBuf,
}

impl SnapshotService {
    /// Create a new FileSystemSnapshotService with a specific home path
    pub fn new(cwd: PathBuf, snapshot_base_dir: PathBuf) -> Self {
        Self { cwd, snapshot_base_dir }
    }

    /// Helper method to handle relative paths by joining with cwd and
    /// canonicalizing
    fn canonicalize_path(&self, path: &Path) -> PathBuf {
        if path.is_relative() {
            // If the path is relative, join it with current working directory
            self.cwd.join(path)
        } else {
            // If it's already absolute, just use it as is
            path.to_path_buf()
        }
    }

    /// Create a snapshot filename from a hash ID
    fn create_snapshot_filename(&self, path: &str, now: u128) -> String {
        self.snapshot_base_dir
            .join(path)
            .join(format!("{}.json", now))
            .display()
            .to_string()
    }

    fn path_hash(path_str: &str) -> String {
        let mut hasher = fnv_rs::Fnv64::default();
        hasher.write(path_str.as_bytes());
        format!("{:x}", hasher.finish())
    }
}

impl SnapshotService {
    pub fn snapshot_dir(&self) -> PathBuf {
        self.snapshot_base_dir.clone()
    }
    pub async fn create_snapshot(&self, path: PathBuf) -> Result<Snapshot> {
        let absolute_path = self.canonicalize_path(&path);
        // Create timestamp
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("Failed to get timestamp")?
            .as_millis();
        let snapshot_path = self
            .create_snapshot_filename(&Self::path_hash(&absolute_path.display().to_string()), now);
        if let Some(parent) = PathBuf::from(&snapshot_path).parent() {
            forge_fs::ForgeFS::create_dir_all(parent).await?;
        }

        // Generate a unique ID using UUID
        let snapshot_id = SnapshotId::new();

        // Read content
        let content = forge_fs::ForgeFS::read(&path).await?;

        // Create JSON snapshot file
        let snapshot_info = Snapshot {
            id: snapshot_id,
            original_path: absolute_path.display().to_string(),
            timestamp: now,
            content: general_purpose::STANDARD.encode(content),
            snapshot_path: snapshot_path.clone(),
        };

        forge_fs::ForgeFS::write(snapshot_path, serde_json::to_string(&snapshot_info)?).await?;

        Ok(snapshot_info)
    }

    pub async fn list_snapshots(&self, path: Option<PathBuf>) -> Result<Vec<Snapshot>> {
        let path = path.map(|v| self.canonicalize_path(&v));
        if let Some(path) = path {
            let cwd = self
                .snapshot_base_dir
                .join(Self::path_hash(&path.display().to_string()));
            let snaps = forge_walker::Walker::max_all()
                .cwd(cwd.clone())
                .get()
                .await?;
            let files = futures::future::join_all(
                snaps
                    .into_iter()
                    .filter(|v| !v.is_dir())
                    .map(|v| forge_fs::ForgeFS::read(cwd.join(v.path))),
            )
            .await
            .into_iter()
            .flatten()
            .flat_map(|v| serde_json::from_slice::<Snapshot>(&v))
            .collect::<Vec<_>>();

            return Ok(files);
        }
        let cwd = self.snapshot_base_dir.clone();
        Ok(futures::future::join_all(
            forge_walker::Walker::max_all()
                .cwd(cwd.clone())
                .get()
                .await?
                .into_iter()
                .filter(|v| !v.is_dir())
                .map(|v| forge_fs::ForgeFS::read(cwd.join(v.path))),
        )
        .await
        .into_iter()
        .flatten()
        .flat_map(|v| serde_json::from_slice::<Snapshot>(&v))
        .collect::<Vec<_>>())
    }

    pub async fn get_snapshot_with_hash(&self, path: &str, hash: &str) -> Result<Snapshot> {
        let snaps = self.list_snapshots(Some(PathBuf::from(path))).await?;
        let id = SnapshotId::parse(hash).ok_or_else(|| anyhow!("Invalid snapshot ID format"))?;

        snaps
            .into_iter()
            .find(|v| v.id == id)
            .ok_or_else(|| anyhow!("Snapshot not found"))
    }

    pub async fn restore_snapshot_with_hash(&self, path: &str, hash: &str) -> Result<()> {
        let info = self.get_snapshot_with_hash(path, hash).await?;
        forge_fs::ForgeFS::write(
            info.original_path,
            general_purpose::STANDARD.decode(info.content)?,
        )
        .await
    }

    pub async fn get_snapshot_with_timestamp(
        &self,
        path: &str,
        timestamp: u128,
    ) -> Result<Snapshot> {
        let snaps = self.list_snapshots(Some(PathBuf::from(path))).await?;
        snaps
            .into_iter()
            .find(|v| v.timestamp == timestamp)
            .ok_or_else(|| anyhow!("Snapshot not found"))
    }

    pub async fn restore_snapshot_with_timestamp(&self, path: &str, timestamp: u128) -> Result<()> {
        let info = self.get_snapshot_with_timestamp(path, timestamp).await?;
        forge_fs::ForgeFS::write(
            info.original_path,
            general_purpose::STANDARD.decode(info.content)?,
        )
        .await
    }
    pub async fn get_latest(&self, path: &Path) -> Result<Snapshot> {
        let snaps = self.list_snapshots(Some(path.to_path_buf())).await?;
        snaps
            .into_iter()
            .min_by_key(|v| v.timestamp)
            .context("No snapshots found")
    }

    pub async fn restore_previous(&self, path: &Path) -> Result<()> {
        let info = self.get_latest(path).await?;
        forge_fs::ForgeFS::write(
            info.original_path,
            general_purpose::STANDARD.decode(info.content)?,
        )
        .await
    }
    pub async fn purge_older_than(&self, days: u32) -> Result<usize> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("Failed to get timestamp")?
            .as_millis();
        let threshold = now - (days as u128 * 24 * 60 * 60 * 1000);

        let snaps = self.list_snapshots(None).await?;
        let to_delete = snaps
            .into_iter()
            .filter(|v| v.timestamp < threshold)
            .collect::<Vec<_>>();

        let deleted = futures::future::join_all(
            to_delete
                .into_iter()
                .map(|v| forge_fs::ForgeFS::remove_file(v.snapshot_path)),
        )
        .await
        .into_iter()
        .filter(|v| v.is_ok())
        .count();

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use tempfile::{tempdir, TempDir};

    use super::*;

    fn modify_file<T: AsRef<[u8]>>(file: &mut File, content: T) -> Result<()> {
        file.write_all(content.as_ref())?;
        Ok(())
    }

    #[tokio::test]
    async fn test_create_snapshot() -> Result<()> {
        let temp_dir = tempdir()?;
        let home_path = temp_dir.path().to_path_buf();
        let service = SnapshotService::new(home_path.clone(), home_path.join("snaps"));

        // Create a test file
        let test_file_path = temp_dir.path().join("test.txt");
        let test_content = "Hello, world!";
        let modified_content = "Good bye cruel world!";
        let mut file = File::create(&test_file_path)?;
        modify_file(&mut file, test_content)?;

        // Create snapshot
        let info = service.create_snapshot(test_file_path.clone()).await?;
        modify_file(&mut file, modified_content)?;

        // Verify ID is valid
        let id_str = info.id.to_string();
        assert!(!id_str.is_empty());

        // Find snapshots
        let snapshots = service.list_snapshots(Some(test_file_path.clone())).await?;
        assert_eq!(snapshots.len(), 1);

        // Restore by hash
        service
            .restore_snapshot_with_hash(&test_file_path.display().to_string(), &id_str)
            .await?;

        let updated = std::fs::read_to_string(&test_file_path)?;
        assert_eq!(updated, test_content);
        modify_file(&mut file, modified_content)?;

        // Restore by index
        service
            .restore_snapshot_with_timestamp(&test_file_path.display().to_string(), info.timestamp)
            .await?;
        let updated = std::fs::read_to_string(test_file_path)?;
        assert_eq!(updated, test_content);

        Ok(())
    }

    struct Snaps {
        service: SnapshotService,
        infos: Vec<Snapshot>,
    }

    async fn init_multiple(temp_dir: &TempDir, test_contents: &[&str]) -> Result<Snaps> {
        let home_path = temp_dir.path();
        let service = SnapshotService::new(home_path.to_path_buf(), home_path.join("snaps"));
        let mut snapshots = vec![];

        // Create a test file
        let test_file_path = temp_dir.path().join("test.txt");

        for content in test_contents {
            // Update the file
            let mut file = File::create(&test_file_path)?;
            modify_file(&mut file, content)?;

            // Create snapshot
            let info = service.create_snapshot(test_file_path.clone()).await?;
            snapshots.push(info);
            // Small delay to ensure different timestamps
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        assert_eq!(
            service
                .list_snapshots(Some(test_file_path.clone()))
                .await?
                .len(),
            3
        );

        Ok(Snaps { service, infos: snapshots })
    }

    #[tokio::test]
    async fn test_multiple_snapshots_hash_restoration() -> Result<()> {
        let test_contents = vec!["First version", "Second version", "Third version"];
        let temp_dir = tempdir()?;

        let snaps = init_multiple(&temp_dir, &test_contents).await?;

        // Verify restore by hash works for all snapshots
        for (i, info) in snaps.infos.iter().enumerate() {
            let id_str = info.id.to_string();
            snaps
                .service
                .restore_snapshot_with_hash(&info.original_path, &id_str)
                .await?;
            assert_eq!(
                std::fs::read_to_string(&info.original_path)?,
                test_contents[i]
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_snapshots_timestamp_restoration() -> Result<()> {
        let test_contents = vec!["First version", "Second version", "Third version"];
        let temp_dir = tempdir()?;

        let snaps = init_multiple(&temp_dir, &test_contents).await?;

        // Verify restore by timestamp works for all snapshots
        for (i, info) in snaps.infos.iter().enumerate() {
            snaps
                .service
                .restore_snapshot_with_timestamp(&info.original_path, info.timestamp)
                .await?;
            assert_eq!(
                std::fs::read_to_string(&info.original_path)?,
                test_contents[i]
            );
        }

        Ok(())
    }
}
