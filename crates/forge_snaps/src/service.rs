use std::path::PathBuf;

use anyhow::{Context, Result};
use forge_fs::ForgeFS;
use tokio::fs;

use crate::snapshot::Snapshot;

/// Implementation of the SnapshotService
#[derive(Debug)]
pub struct SnapshotService {
    /// Base directory for storing snapshots
    snapshots_directory: PathBuf,
}

impl SnapshotService {
    /// Create a new FileSystemSnapshotService with a specific home path
    pub fn new(snapshot_base_dir: PathBuf) -> Self {
        Self { snapshots_directory: snapshot_base_dir }
    }
}

impl SnapshotService {
    pub async fn create_snapshot(&self, path: PathBuf) -> Result<Snapshot> {
        let snapshot = Snapshot::create(path).await?;

        // Create intermediary directories if they don't exist
        let snapshot_path = snapshot.snapshot_path(Some(self.snapshots_directory.clone()));
        if let Some(parent) = PathBuf::from(&snapshot_path).parent() {
            ForgeFS::create_dir_all(parent).await?;
        }

        snapshot
            .save(Some(self.snapshots_directory.clone()))
            .await?;

        Ok(snapshot)
    }

    pub async fn undo_snapshot(&self, path: PathBuf) -> Result<()> {
        // Create a temporary snapshot to get the hash directory
        let snapshot = Snapshot::create(path.clone()).await?;
        let hash_dir = self.snapshots_directory.join(snapshot.path_hash());

        // Check if snapshots exist
        if !ForgeFS::exists(&hash_dir) {
            return Err(anyhow::anyhow!("No snapshots found for {:?}", path));
        }

        // Find the most recent snapshot
        let mut latest_entry = None;
        let mut latest_modified = None;
        let mut dir = fs::read_dir(&hash_dir).await?;

        while let Some(entry) = dir.next_entry().await? {
            if entry.file_name().to_string_lossy().ends_with(".snap") {
                let metadata = entry.metadata().await?;
                let modified = metadata.modified()?;

                if latest_modified.is_none() || modified > latest_modified.unwrap() {
                    latest_entry = Some(entry);
                    latest_modified = Some(modified);
                }
            }
        }

        // Get the latest snapshot
        let latest = latest_entry.context(format!("No valid snapshots found for {:?}", path))?;

        // Restore the content
        let snapshot_path = latest.path();
        let content = ForgeFS::read(&snapshot_path).await?;
        ForgeFS::write(&path, content).await?;

        // Remove the used snapshot
        ForgeFS::remove_file(&snapshot_path).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    // Test helpers
    struct TestContext {
        _temp_dir: TempDir,
        _snapshots_dir: PathBuf,
        test_file: PathBuf,
        service: SnapshotService,
    }

    impl TestContext {
        async fn new() -> Result<Self> {
            let temp_dir = TempDir::new()?;
            let snapshots_dir = temp_dir.path().join("snapshots");
            let test_file = temp_dir.path().join("test.txt");
            let service = SnapshotService::new(snapshots_dir.clone());

            Ok(Self {
                _temp_dir: temp_dir,
                _snapshots_dir: snapshots_dir,
                test_file,
                service,
            })
        }

        async fn write_content(&self, content: &str) -> Result<()> {
            ForgeFS::write(&self.test_file, content.as_bytes()).await
        }

        async fn read_content(&self) -> Result<String> {
            let content = ForgeFS::read(&self.test_file).await?;
            Ok(String::from_utf8(content)?)
        }

        async fn create_snapshot(&self) -> Result<Snapshot> {
            self.service.create_snapshot(self.test_file.clone()).await
        }

        async fn undo_snapshot(&self) -> Result<()> {
            self.service.undo_snapshot(self.test_file.clone()).await
        }
    }

    #[tokio::test]
    async fn test_create_snapshot() -> Result<()> {
        // Arrange
        let ctx = TestContext::new().await?;
        let test_content = "Hello, World!";

        // Act
        ctx.write_content(test_content).await?;
        let snapshot = ctx.create_snapshot().await?;

        // Assert
        let snapshot_content = ForgeFS::read(&snapshot.path).await?;
        assert_eq!(String::from_utf8(snapshot_content)?, test_content);

        Ok(())
    }

    #[tokio::test]
    async fn test_undo_snapshot() -> Result<()> {
        // Arrange
        let ctx = TestContext::new().await?;
        let initial_content = "Initial content";
        let modified_content = "Modified content";

        // Act
        ctx.write_content(initial_content).await?;
        ctx.create_snapshot().await?;
        ctx.write_content(modified_content).await?;
        ctx.undo_snapshot().await?;

        // Assert
        assert_eq!(ctx.read_content().await?, initial_content);

        Ok(())
    }

    #[tokio::test]
    async fn test_undo_snapshot_no_snapshots() -> Result<()> {
        // Arrange
        let ctx = TestContext::new().await?;

        // Act
        ctx.write_content("test content").await?;
        let result = ctx.undo_snapshot().await;

        // Assert
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No snapshots found"));

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_snapshots() -> Result<()> {
        // Arrange
        let ctx = TestContext::new().await?;

        // Act
        ctx.write_content("Initial content").await?;
        ctx.create_snapshot().await?;

        ctx.write_content("Second content").await?;
        ctx.create_snapshot().await?;

        ctx.write_content("Final content").await?;
        ctx.undo_snapshot().await?;

        // Assert
        assert_eq!(ctx.read_content().await?, "Second content");

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_snapshots_undo_twice() -> Result<()> {
        // Arrange
        let ctx = TestContext::new().await?;

        // Act
        ctx.write_content("Initial content").await?;
        ctx.create_snapshot().await?;

        ctx.write_content("Second content").await?;
        ctx.create_snapshot().await?;

        ctx.write_content("Final content").await?;
        ctx.undo_snapshot().await?;
        ctx.undo_snapshot().await?;

        // Assert
        assert_eq!(ctx.read_content().await?, "Initial content");

        Ok(())
    }
}
