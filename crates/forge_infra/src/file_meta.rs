use std::path::Path;

use anyhow::Result;
use forge_app::FileMetaService;

pub struct ForgeFileMetaService;
#[async_trait::async_trait]
impl FileMetaService for ForgeFileMetaService {
    async fn is_file(&self, path: &Path) -> Result<bool> {
        Ok(forge_fs::ForgeFS::exists(path))
    }
}
