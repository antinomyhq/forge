use std::path::Path;

use anyhow::{Context, Result};
use bytes::Bytes;
use forge_app::FileReadService;

pub struct ForgeFileReadService;

impl Default for ForgeFileReadService {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeFileReadService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl FileReadService for ForgeFileReadService {
    async fn read(&self, path: &Path) -> Result<Bytes> {
        Ok(forge_fs::ForgeFS::read(path).await.map(Bytes::from)?)
    }
}
