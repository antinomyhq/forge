use std::path::{Path, PathBuf};

use bytes::Bytes;
use forge_services::FsWriteService;

#[derive(Default)]
pub struct ForgeFileWriteService {}

impl ForgeFileWriteService {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl FsWriteService for ForgeFileWriteService {
    async fn write(&self, path: &Path, contents: Bytes) -> anyhow::Result<()> {
        Ok(forge_fs::ForgeFS::write(path, contents.to_vec()).await?)
    }

    async fn write_temp(&self, prefix: &str, ext: &str, content: &str) -> anyhow::Result<PathBuf> {
        let path = tempfile::Builder::new()
            .keep(true)
            .prefix(prefix)
            .suffix(ext)
            .tempfile()?
            .into_temp_path()
            .to_path_buf();

        self.write(&path, content.to_string().into()).await?;

        Ok(path)
    }
}
