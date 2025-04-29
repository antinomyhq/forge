use std::path::Path;

use anyhow::Result;
use forge_services::FsReadService;

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
impl FsReadService for ForgeFileReadService {
    async fn read(&self, path: &Path) -> Result<String> {
        forge_fs::ForgeFS::read_utf8(path).await
    }

    async fn range_read(
        &self,
        path: &Path,
        start_char: u64,
        end_char: u64,
    ) -> Result<(String, forge_fs::FileInfo)> {
        forge_fs::ForgeFS::read_range_utf8(path, start_char, end_char).await
    }
}
