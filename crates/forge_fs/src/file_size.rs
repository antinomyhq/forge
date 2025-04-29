use std::path::Path;

use anyhow::{Context, Result};

impl crate::ForgeFS {
    /// Gets file size without reading the entire file
    pub async fn get_file_size<T: AsRef<Path>>(path: T) -> Result<u64> {
        let metadata = tokio::fs::metadata(path.as_ref()).await.with_context(|| {
            format!(
                "Failed to get metadata for file {}",
                path.as_ref().display()
            )
        })?;

        Ok(metadata.len())
    }
}
