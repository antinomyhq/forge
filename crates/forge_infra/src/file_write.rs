use std::path::Path;

use anyhow::{Context, Result};
use forge_app::FileWriteService;

pub struct ForgeFileWriteService;

impl Default for ForgeFileWriteService {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeFileWriteService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl FileWriteService for ForgeFileWriteService {
    async fn write(&self, path: &Path, content: &str) -> Result<()> {
        Ok(tokio::fs::write(path, content)
            .await
            .with_context(|| format!("Failed to write to the file: {}", path.display()))?)
    }
}
