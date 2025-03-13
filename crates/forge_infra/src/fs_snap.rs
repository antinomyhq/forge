use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use forge_app::FsSnapshotService;
use forge_domain::Environment;
use forge_snaps::{SnapshotInfo, SnapshotMetadata};

pub struct ForgeFileSnapshotService {
    inner: Arc<forge_snaps::SnapshotService>,
}

impl ForgeFileSnapshotService {
    pub fn new(env: Environment) -> Self {
        Self {
            inner: Arc::new(forge_snaps::SnapshotService::new(env.snapshot_path())),
        }
    }
}

#[async_trait::async_trait]
impl FsSnapshotService for ForgeFileSnapshotService {
    fn snapshot_dir(&self) -> PathBuf {
        self.inner.snapshot_dir()
    }

    // Creation
    // FIXME: don't depend on forge_snaps::SnapshotInfo directly
    async fn create_snapshot(&self, file_path: &Path) -> Result<SnapshotInfo> {
        self.inner.create_snapshot(file_path).await
    }

    // Listing
    async fn list_snapshots(&self, file_path: &Path) -> Result<Vec<SnapshotInfo>> {
        self.inner.list_snapshots(file_path).await
    }

    // Timestamp-based restoration
    async fn restore_by_timestamp(&self, file_path: &Path, timestamp: &str) -> Result<()> {
        self.inner.restore_by_timestamp(file_path, timestamp).await
    }

    // Index-based restoration (0 = newest, 1 = previous version, etc.)
    async fn restore_by_index(&self, file_path: &Path, index: isize) -> Result<()> {
        self.inner.restore_by_index(file_path, index).await
    }

    // Convenient method to restore previous version
    async fn restore_previous(&self, file_path: &Path) -> Result<()> {
        self.inner.restore_by_index(file_path, 1).await
    }

    // Metadata access
    async fn get_snapshot_by_timestamp(
        &self,
        file_path: &Path,
        timestamp: &str,
    ) -> Result<SnapshotMetadata> {
        self.inner
            .get_snapshot_by_timestamp(file_path, timestamp)
            .await
    }
    async fn get_snapshot_by_index(
        &self,
        file_path: &Path,
        index: isize,
    ) -> Result<SnapshotMetadata> {
        self.inner.get_snapshot_by_index(file_path, index).await
    }

    // Global purge operation
    async fn purge_older_than(&self, days: u32) -> Result<usize> {
        self.inner.purge_older_than(days).await
    }
}
