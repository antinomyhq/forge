use std::path::Path;

use anyhow::Result;
use forge_domain::MimeType as DomainMimeType;
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

    async fn file_size(&self, path: &Path) -> Result<u64> {
        forge_fs::ForgeFS::file_size(path).await
    }

    async fn mime_type(&self, path: &Path) -> anyhow::Result<DomainMimeType> {
        Ok(forge_fs::ForgeFS::mime_type(path)
            .await?
            .map_or(DomainMimeType::Text, |mime| {
                DomainMimeType::from(mime.as_str())
            }))
    }
}
