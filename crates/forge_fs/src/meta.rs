use std::path::Path;

use anyhow::{Context, Result};

impl crate::ForgeFS {
    pub fn exists<T: AsRef<Path>>(path: T) -> bool {
        path.as_ref().exists()
    }

    pub async fn is_binary_file<T: AsRef<Path>>(path: T) -> anyhow::Result<bool> {
        let mut file = tokio::fs::File::open(path.as_ref())
            .await
            .with_context(|| format!("Failed to open file {}", path.as_ref().display()))?;
        let (is_text, _) = crate::ForgeFS::is_binary(&mut file).await?;
        Ok(!is_text)
    }

    pub fn is_file<T: AsRef<Path>>(path: T) -> bool {
        path.as_ref().is_file()
    }

    pub async fn read_dir<T: AsRef<Path>>(path: T) -> Result<tokio::fs::ReadDir> {
        tokio::fs::read_dir(path.as_ref())
            .await
            .with_context(|| format!("Failed to read directory {}", path.as_ref().display()))
    }
}
