use std::path::Path;

use anyhow::Result;
use forge_services::FsMetaService;

pub struct ForgeFileMetaService;
#[async_trait::async_trait]
impl FsMetaService for ForgeFileMetaService {
    async fn is_file(&self, path: &Path) -> Result<bool> {
        Ok(forge_fs::ForgeFS::is_file(path))
    }

    async fn exists(&self, path: &Path) -> Result<bool> {
        Ok(forge_fs::ForgeFS::exists(path))
    }
}
