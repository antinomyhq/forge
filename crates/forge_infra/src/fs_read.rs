use std::path::Path;

use anyhow::Result;
use forge_app::FileReaderInfra;

pub struct ForgeFileReadService;

impl Default for ForgeFileReadService {
    fn default() -> Self {
        Self
    }
}

impl ForgeFileReadService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl FileReaderInfra for ForgeFileReadService {
    async fn read_utf8(&self, path: &Path) -> Result<String> {
        forge_fs::ForgeFS::read_utf8(path).await
    }

    async fn read(&self, path: &Path) -> Result<Vec<u8>> {
        forge_fs::ForgeFS::read(path).await
    }

    async fn range_read_utf8(
        &self,
        path: &Path,
        start_line: u64,
        end_line: u64,
    ) -> Result<(String, forge_domain::FileInfo)> {
        forge_fs::ForgeFS::read_range_utf8(path, start_line, end_line).await
    }
}
