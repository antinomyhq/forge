use std::path::Path;

use forge_app::FileRemoverInfra;

/// Low-level file remove service
///
/// Provides primitive file deletion operations without snapshot coordination.
/// Snapshot management should be handled at the service layer.
#[derive(Default)]
pub struct ForgeFileRemoveService;

impl ForgeFileRemoveService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl FileRemoverInfra for ForgeFileRemoveService {
    async fn remove(&self, path: &Path) -> anyhow::Result<()> {
        Ok(forge_fs::ForgeFS::remove_file(path).await?)
    }
}
