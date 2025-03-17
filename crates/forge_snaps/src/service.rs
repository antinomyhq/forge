use std::path::PathBuf;

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
    pub async fn create_snapshot(&self, path: PathBuf) -> anyhow::Result<Snapshot> {
        let snapshot = Snapshot::create(path).await?;

        // Create intermediary directories if they don't exist
        let snapshot_path = snapshot.snapshot_path(Some(self.snapshots_directory.clone()));
        if let Some(parent) = PathBuf::from(&snapshot_path).parent() {
            forge_fs::ForgeFS::create_dir_all(parent).await?;
        }

        snapshot
            .save(Some(self.snapshots_directory.clone()))
            .await?;

        Ok(snapshot)
    }
}
