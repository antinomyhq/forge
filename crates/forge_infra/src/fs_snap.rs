use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use forge_domain::Environment;
use forge_services::FsSnapshotService;
use forge_snaps::Snapshot;

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
    // Creation
    async fn create_snapshot(&self, file_path: &Path) -> Result<Snapshot> {
        self.inner.create_snapshot(file_path.to_path_buf()).await
    }
}
