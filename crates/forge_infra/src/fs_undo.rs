use std::path::Path;

use anyhow::Result;
use forge_domain::Environment;
use forge_services::FsUndoService;
use forge_snaps::SnapshotController;

pub struct ForgeFileUndoService {
    inner: SnapshotController,
}

impl Default for ForgeFileUndoService {
    fn default() -> Self {
        // In default implementation, we'll use a temporary directory path
        // This is mainly for testing purposes
        let temp_dir = std::env::temp_dir().join("forge-snapshots");
        Self { inner: SnapshotController::new(temp_dir) }
    }
}

impl ForgeFileUndoService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new ForgeFileUndoService with a specific environment
    pub fn with_env(env: Environment) -> Self {
        Self { inner: SnapshotController::new(env.snapshot_path()) }
    }
}

#[async_trait::async_trait]
impl FsUndoService for ForgeFileUndoService {
    async fn undo(&self, file_path: &Path) -> Result<()> {
        self.inner.undo_snapshot(file_path.to_path_buf()).await
    }
}
