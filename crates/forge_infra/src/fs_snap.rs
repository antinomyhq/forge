use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use forge_app::FsSnapshotService;
use forge_domain::Environment;
use forge_snaps::Snapshot;

pub struct ForgeFileSnapshotService {
    inner: Arc<forge_snaps::SnapshotService>,
}

impl ForgeFileSnapshotService {
    pub fn new(env: Environment) -> Self {
        Self {
            inner: Arc::new(forge_snaps::SnapshotService::new(
                env.cwd.clone(),
                env.snapshot_path(),
            )),
        }
    }
}

#[async_trait::async_trait]
impl FsSnapshotService for ForgeFileSnapshotService {
    // Creation
    // FIXME: don't depend on forge_snaps::SnapshotInfo directly
    async fn create_snapshot(&self, file_path: &Path) -> Result<Snapshot> {
        self.inner.create_snapshot(file_path.to_path_buf()).await
    }

    // Listing
    async fn list_snapshots(&self, path: Option<&Path>) -> Result<Vec<Snapshot>> {
        self.inner
            .list_snapshots(path.map(|v| v.to_path_buf()))
            .await
    }

    // Timestamp-based restoration
    async fn restore_by_timestamp(&self, file_path: &Path, timestamp: u128) -> Result<()> {
        self.inner
            .restore_snapshot_with_timestamp(&file_path.display().to_string(), timestamp)
            .await
    }

    // Index-based restoration (0 = newest, 1 = previous version, etc.)
    async fn restore_by_hash(&self, file_path: &Path, hash: &str) -> Result<()> {
        self.inner
            .restore_snapshot_with_hash(&file_path.display().to_string(), hash)
            .await
    }

    // Get latest snapshot
    async fn get_latest(&self, file_path: &Path) -> Result<Snapshot> {
        self.inner.get_latest(file_path).await
    }

    // Convenient method to restore previous version
    async fn restore_previous(&self, file_path: &Path) -> Result<()> {
        self.inner.restore_previous(file_path).await
    }

    // Metadata access
    async fn get_snapshot_by_timestamp(
        &self,
        file_path: &Path,
        timestamp: u128,
    ) -> Result<Snapshot> {
        self.inner
            .get_snapshot_with_timestamp(&file_path.display().to_string(), timestamp)
            .await
    }
    async fn get_snapshot_by_hash(&self, file_path: &Path, hash: &str) -> Result<Snapshot> {
        self.inner
            .get_snapshot_with_hash(&file_path.display().to_string(), hash)
            .await
    }

    // Global purge operation
    async fn purge_older_than(&self, days: u32) -> Result<usize> {
        self.inner.purge_older_than(days).await
    }
}
