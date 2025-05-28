use std::path::Path;

use anyhow::{Context, Result};

impl crate::ForgeFS {
    pub async fn read_utf8<T: AsRef<Path>>(path: T) -> Result<String> {
        Self::read(path)
            .await
            .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
    }

    pub async fn read<T: AsRef<Path>>(path: T) -> Result<Vec<u8>> {
        match tokio::fs::read(path.as_ref()).await {
            Ok(content) => Ok(content),
            Err(e) => {
                tracing::error!("Failed to read file {}: {}", path.as_ref().display(), e);
                Err(e).with_context(|| format!("Failed to read file {}", path.as_ref().display()))
            }
        }
    }

    pub async fn read_to_string<T: AsRef<Path>>(path: T) -> Result<String> {
        match tokio::fs::read_to_string(path.as_ref()).await {
            Ok(content) => Ok(content),
            Err(e) => {
                tracing::error!(
                    "Failed to read file as string {}: {}",
                    path.as_ref().display(),
                    e
                );
                Err(e).with_context(|| {
                    format!("Failed to read file as string {}", path.as_ref().display())
                })
            }
        }
    }
}
