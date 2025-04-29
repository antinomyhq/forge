use std::path::Path;

use anyhow::{Context, Result};

impl crate::ForgeFS {
    pub async fn create_dir_all<T: AsRef<Path>>(path: T) -> Result<()> {
        tokio::fs::create_dir_all(path.as_ref())
            .await
            .with_context(|| format!("Failed to create dir {}", path.as_ref().display()))
    }

    pub async fn write<T: AsRef<Path>, U: AsRef<[u8]>>(path: T, contents: U) -> Result<()> {
        tokio::fs::write(path.as_ref(), contents)
            .await
            .with_context(|| format!("Failed to write file {}", path.as_ref().display()))
    }

    pub async fn remove_file<T: AsRef<Path>>(path: T) -> Result<()> {
        tokio::fs::remove_file(path.as_ref())
            .await
            .with_context(|| format!("Failed to remove file {}", path.as_ref().display()))
    }
}
