//! # ForgeFS
//!
//! A file system abstraction layer that standardizes error handling for file
//! operations.
//!
//! ForgeFS wraps tokio's filesystem operations with consistent error context
//! using anyhow::Context. Each method provides standardized error messages in
//! the format "Failed to [operation] [path]", ensuring uniform error reporting
//! throughout the application while preserving the original error cause.

use std::path::Path;

use anyhow::{Context, Result};

pub struct ForgeFS;

impl ForgeFS {
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

    pub async fn read_utf8<T: AsRef<Path>>(path: T) -> Result<String> {
        ForgeFS::read(path)
            .await
            .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
    }

    pub async fn read<T: AsRef<Path>>(path: T) -> Result<Vec<u8>> {
        tokio::fs::read(path.as_ref())
            .await
            .with_context(|| format!("Failed to read file {}", path.as_ref().display()))
    }
    pub async fn remove_file<T: AsRef<Path>>(path: T) -> Result<()> {
        tokio::fs::remove_file(path.as_ref())
            .await
            .with_context(|| format!("Failed to remove file {}", path.as_ref().display()))
    }
    pub fn exists<T: AsRef<Path>>(path: T) -> bool {
        path.as_ref().exists()
    }
    pub fn is_file<T: AsRef<Path>>(path: T) -> bool {
        path.as_ref().is_file()
    }
}
