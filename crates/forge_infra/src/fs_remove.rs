use std::path::Path;

use forge_services::FileRemoveService;

#[derive(Default)]
pub struct ForgeFileRemoveService {}

impl ForgeFileRemoveService {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl FileRemoveService for ForgeFileRemoveService {
    async fn remove(&self, path: &Path) -> anyhow::Result<()> {
        Ok(forge_fs::ForgeFS::remove_file(path).await?)
    }
}
