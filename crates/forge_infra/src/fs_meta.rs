use std::path::Path;

use anyhow::Result;
use forge_domain::MimeType;
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

    async fn mime_type(&self, path: &Path) -> anyhow::Result<MimeType> {
        forge_fs::ForgeFS::is_binary(path).await.map(|v| v.1).map(|v| MimeType::from(v.as_str()))
    }
}
