use std::path::Path;

use anyhow::Result;
use forge_services::FileInfoInfra;

pub struct ForgeFileMetaService;
#[async_trait::async_trait]
impl FileInfoInfra for ForgeFileMetaService {
    async fn is_file(&self, path: &Path) -> Result<bool> {
        Ok(forge_fs::ForgeFS::is_file(path))
    }

    async fn exists(&self, path: &Path) -> Result<bool> {
        Ok(forge_fs::ForgeFS::exists(path))
    }

    async fn file_size(&self, path: &Path) -> Result<u64> {
        forge_fs::ForgeFS::file_size(path).await
    }
}
